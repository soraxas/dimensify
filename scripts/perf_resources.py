#!/usr/bin/env python3
"""Scoped resource metrics and direct-child OS accounting; never infer VRAM."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys

BYTE_MEASURES = {name: "bytes" for name in (
    "cache-payload-bytes", "process-memory-bytes", "buffer-capacity-bytes",
    "gpu-buffer-bytes", "wasm-memory-bytes", "js-heap-bytes")}


def nonnegative(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise ValueError("Expected finite nonnegative resource measurement")
    return value


def peak_rss_bytes(value, system):
    value = nonnegative(value)
    return int(value) if system == "darwin" else int(value * 1024) if system.startswith("linux") else None


def supervisor(command, output):
    executable = Path(command[0]).resolve(strict=True)
    with executable.open("rb") as stream:
        identity = hashlib.file_digest(stream, "sha256").hexdigest()
    report = {"schema": "dimensify-process-usage/1", "executable_sha256": identity,
              "method": "os.wait4 direct-child resource usage; max RSS is OS high-water accounting, not GPU residency",
              "platform": sys.platform, "peak_rss_bytes": None, "cpu_user_ns": None,
              "cpu_system_ns": None, "unavailable_reason": None}
    child = subprocess.Popen(command)
    if hasattr(os, "wait4") and sys.platform in ("darwin", "linux"):
        _, status, usage = os.wait4(child.pid, 0)
        child.returncode = os.waitstatus_to_exitcode(status)
        report.update(peak_rss_bytes=peak_rss_bytes(usage.ru_maxrss, sys.platform),
                      cpu_user_ns=round(usage.ru_utime * 1e9), cpu_system_ns=round(usage.ru_stime * 1e9))
    else:
        child.wait()
        report["unavailable_reason"] = "OS direct-child high-water accounting unavailable on this platform"
    report["exit_code"] = child.returncode
    with executable.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != identity:
            raise ValueError("Measured executable changed during run")
    Path(output).write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    return child.returncode


def native_metrics(report):
    result = {}

    def add(name, value, measure):
        if value is not None:
            result[name] = {measure: {"value": nonnegative(value)}}

    resources = report.get("resource_usage")
    if resources:
        if resources["schema_version"] != 1:
            raise ValueError("Unknown native resource schema")
        seen = set()
        for point in resources["checkpoints"]:
            phase = point["phase"]
            if phase in seen:
                raise ValueError("Duplicate memory checkpoint")
            seen.add(phase)
            add(f"memory/process-rss/{phase}", point.get("rss_bytes"), "process-memory-bytes")
            add(f"memory/process-virtual/{phase}", point.get("virtual_memory_bytes"), "process-memory-bytes")
        decoded = resources["decoded"]
        if "planes" in decoded and sum(nonnegative(row["capacity_bytes"]) for row in decoded["planes"]) != decoded["capacity_bytes"]:
            raise ValueError("Decoded capacity does not match planes")
        add("memory/decoded-vector-capacity", decoded["capacity_bytes"], "buffer-capacity-bytes")
        for point in resources["gpu_checkpoints"]:
            value = point["tracked_buffer_bytes"]
            if value is not None and sum(nonnegative(row["size_bytes"]) for row in point["buffers"]) != value:
                raise ValueError("GPU tracked capacity does not match buffers")
            add(f"memory/tracked-gpu-buffers/{point['phase']}", value, "gpu-buffer-bytes")
    usage = report.get("process_usage")
    if usage:
        if usage["schema"] != "dimensify-process-usage/1" or usage["exit_code"] != 0:
            raise ValueError("Failed or unsupported process accounting")
        if usage["executable_sha256"] != report["environment"]["executable_sha256"]:
            raise ValueError("Resource sampler observed a different executable")
        add("memory/process-os-peak-rss", usage["peak_rss_bytes"], "process-memory-bytes")
        for field in ("cpu_user_ns", "cpu_system_ns"):
            value = usage[field]
            if value is not None and nonnegative(value) > 0:
                add("processing/whole-process-" + field.removesuffix("_ns").replace("_", "-"), value, "latency")
    return result


def browser_metrics(runs):
    result = {}
    rounds = sorted({run["round"] for run in runs})
    fields = (("mainWasmLinearMemoryBytes", "main-wasm-capacity", "wasm-memory-bytes"),
              ("mainJsHeapUsedBytes", "main-js-heap-used", "js-heap-bytes"),
              ("mainJsHeapTotalBytes", "main-js-heap-total", "js-heap-bytes"),
              ("mainJsHeapLimitBytes", "main-js-heap-limit", "js-heap-bytes"),
              ("workerPeakWasmLinearMemoryBytes", "single-worker-wasm-capacity", "wasm-memory-bytes"))
    for field, name, measure in fields:
        peaks = []
        for number in rounds:
            samples = [nonnegative(obs[field]) for run in runs if run["round"] == number
                       for obs in run.get("observations", []) if obs.get(field) is not None]
            if samples:
                peaks.append(max(samples))
        if peaks and len(peaks) != len(rounds):
            raise ValueError("Memory capability changed between rounds")
        if peaks:
            result[f"browser-observed-{name}-peak"] = {measure: {"value": statistics.median(peaks)}}
    workers = [run.get("final", {}).get("worker") or {} for run in runs if run["mode"] == "warm"]
    if any(not isinstance(worker, dict) for worker in workers):
        raise ValueError("Invalid worker telemetry")
    for field in ("started", "completed", "failed"):
        counts = [nonnegative(worker[field]) for worker in workers if worker.get(field) is not None]
        if counts and len(counts) != len(rounds):
            raise ValueError("Incomplete worker lifecycle counts")
        if counts:
            result[f"browser-worker-{field}"] = {"count": {"value": statistics.median(counts)}}
    for field, label in (("handlerElapsedMs", "handler"), ("decodeMs", "decode"),
                         ("roundTripMs", "roundtrip"), ("dispatchToHandlerMs", "dispatch-to-handler"),
                         ("responseDeliveryMs", "response-delivery"), ("postMessageMs", "post-message"),
                         ("admissionElapsedMs", "admission")):
        values = [nonnegative(worker[field]) for worker in workers
                  if worker.get(field) is not None and worker.get("timingSamples", {}).get(field, 0) > 0]
        counts = [nonnegative(worker.get("timingSamples", {}).get(field, 0)) for worker in workers]
        if any(counts):
            result[f"browser-worker-{label}-observed-samples"] = {"count": {"value": statistics.median(counts)}}
        if values and len(values) != len(rounds):
            raise ValueError("Incomplete worker timing across rounds")
        if values:
            value = statistics.median(values)
            # Coarse browser clocks may measure zero. Keep raw zero, never invent epsilon timing.
            if value > 0:
                result[f"browser-worker-observed-{label}-wall-sum"] = {"latency": {"value": value * 1e6}}
    return result


def gpu_pass_metrics(case):
    """Same-scope segments sum per originating frame; scopes never become total GPU."""
    reports = case.get("gpu_pass_reports")
    if not reports:
        return {}
    result = {}
    for report in reports:
        scope = report["scope"]
        values = []
        for row in case["rows"]:
            samples = [sample for sample in row.get("gpu_pass_samples", []) if sample["scope"] == scope]
            if not samples:
                continue
            measured = []
            for sample in samples:
                if str(sample["identity"]["selection"]) != str(row["sample_id"]):
                    raise ValueError("GPU pass sample belongs to another frame")
                measurement = sample["measurement"]
                if measurement.get("failure") is None and measurement.get("duration_ms") is not None:
                    measured.append(nonnegative(measurement["duration_ms"]) * 1e6)
            if len(measured) == len(samples):
                values.append(sum(measured))
        prefix = f"{case['name']}/gpu-pass/{scope.replace('_', '-')}"
        result[prefix + "/completed-frame-samples"] = {"count": {"value": len(values)}}
        if len(values) == len(case["rows"]) and all(value > 0 for value in values):
            ordered = sorted(values)
            for label, value in (("median", statistics.median(ordered)),
                                 ("p95", ordered[math.ceil(len(ordered) * .95) - 1]),
                                 ("p99", ordered[math.ceil(len(ordered) * .99) - 1])):
                result[prefix + "/" + label] = {"latency": {"value": value}}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        raise ValueError("Missing child executable")
    return supervisor(command, args.output)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError) as error:
        print(f"Process measurement failed: {error}", file=sys.stderr)
        sys.exit(1)
