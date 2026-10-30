#!/usr/bin/env python3
"""Validate and aggregate production native packed LOD replay evidence."""

import copy
import hashlib
from functools import lru_cache
from pathlib import Path
import json
import math
import re
import statistics

from perf_resources import (
    gpu_pass_metrics,
    native_metrics as native_resources,
    nonnegative,
)

PROTOCOL = "dimensify-native-lod-offscreen-v1"
STOCHASTIC_PROTOCOL = "dimensify-native-lod-stochastic-offscreen-v1"
CASES = {"static", "moving", "near-far-return"}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)


def positive(value):
    value = nonnegative(value)
    if value <= 0:
        raise ValueError("Expected positive completed measurement")
    return value


def distribution(values):
    values = sorted(positive(value) for value in values)
    return {
        "median": statistics.median(values),
        "p95": values[math.ceil(len(values) * 0.95) - 1],
        "p99": values[math.ceil(len(values) * 0.99) - 1],
    }


def producer(report):
    env = report["environment"]
    return {
        key: env.get(key)
        for key in (
            "git_commit",
            "build_commit",
            "build_profile",
            "build_opt_level",
            "build_rustc",
            "executable_sha256",
        )
    }


@lru_cache(maxsize=1)
def raw_validator():
    from jsonschema import Draft202012Validator

    schema = json.loads(
        (
            Path(__file__).resolve().parents[1] / "tools/perf/src/lod/raw.schema.json"
        ).read_text()
    )
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


def validate_capture(report, case, result_path):
    if result_path is None:
        raise ValueError(
            "Native LOD capture verification requires the saved result path"
        )
    capture = case["capture"]
    directory = Path(result_path).resolve().parent
    filename = capture["file"]
    if Path(filename).name != filename or filename in (".", ".."):
        raise ValueError("Unsafe native LOD capture path")
    path = directory / filename
    if path.is_symlink():
        raise ValueError("Unsafe native LOD capture symlink")
    try:
        data = path.read_bytes()
    except OSError as error:
        raise ValueError("Missing or unreadable native LOD capture") from error
    # The producer writes canonical binary PPM with opaque RGBA8 source pixels.
    header = re.match(rb"P6\n([0-9]+) ([0-9]+)\n255\n", data)
    if header is None:
        raise ValueError("Malformed native LOD capture PPM")
    width, height = (int(part) for part in header.groups())
    rgb = data[header.end() :]
    if [width, height] != report["render"]["physical_resolution"] or len(
        rgb
    ) != width * height * 3:
        raise ValueError("Native LOD capture dimensions or byte count mismatch")
    rgba = bytearray(width * height * 4)
    rgba[0::4], rgba[1::4], rgba[2::4], rgba[3::4] = (
        rgb[0::3],
        rgb[1::3],
        rgb[2::3],
        b"\xff" * (width * height),
    )
    if hashlib.sha256(rgba).hexdigest() != capture["rgba_sha256"]:
        raise ValueError("Native LOD capture RGBA hash mismatch")
    count = sum(max(rgb[index : index + 3]) > 2 for index in range(0, len(rgb), 3))
    if count == 0 or count != capture["non_background_pixels"]:
        raise ValueError("Native LOD capture is blank or non-background count mismatch")


def validate(report, *, result_path=None):
    # Before explicit transparency metadata, this protocol meant sorted only.
    # Normalize that unambiguous legacy default without changing saved evidence.
    schema_report = report
    render = report.get("render")
    if (
        report.get("protocol") == PROTOCOL
        and isinstance(render, dict)
        and "transparency" not in render
    ):
        schema_report = {**report, "render": {**render, "transparency": "sorted"}}
    error = next(raw_validator().iter_errors(schema_report), None)
    if error is not None:
        location = "/".join(str(part) for part in error.absolute_path)
        raise ValueError(
            f"Native LOD raw schema violation at {location}: {error.message}"
        )
    env = report["environment"]
    commit = env.get("git_commit", "")
    adapter = env["adapter"]
    if (
        report.get("protocol") not in (PROTOCOL, STOCHASTIC_PROTOCOL)
        or report.get("schema_version") != 1
        or report.get("classification") != "hardware_performance"
        or report.get("publication_eligible") is not True
        or env.get("git_dirty") is not False
        or env.get("git_dirty_after") is not False
        or env.get("build_dirty") != "false"
        or env.get("debug_assertions") is not False
        or env.get("build_opt_level") != "3"
        or env.get("build_commit") != commit
        or env.get("git_commit_after") != commit
        or not re.fullmatch(r"[a-f0-9]{40}", commit)
        or not re.fullmatch(r"[a-f0-9]{64}", env.get("executable_sha256", ""))
        or adapter.get("device_type") not in ("IntegratedGpu", "DiscreteGpu")
        or re.search(
            r"swiftshader|llvmpipe|lavapipe|software", canonical(adapter), re.I
        )
    ):
        raise ValueError(
            "Native LOD report is not clean optimized hardware performance evidence"
        )
    render = report["render"]
    mode = render.get("transparency", "sorted")
    stochastic = report["protocol"] == STOCHASTIC_PROTOCOL
    if mode not in ("sorted", "stochastic") or stochastic != (mode == "stochastic"):
        raise ValueError(
            "Native LOD transparency must match its sorted/stochastic protocol"
        )
    if render.get("backend") != "packed" or render.get("projection_cache") is not True:
        raise ValueError(
            "Native LOD requires production packed projection configuration"
        )
    if report["source"].get("generated") is not False:
        raise ValueError("Native LOD source must explicitly be non-generated")
    if not re.fullmatch(r"[a-f0-9]{64}", report["source"]["sha256"]):
        raise ValueError("Native LOD manifest identity is invalid")
    cases = report["cases"]
    if len(cases) != 3 or {case["name"] for case in cases} != CASES:
        raise ValueError("Missing native LOD camera case")
    for case in cases:
        validate_capture(report, case, result_path)
        rows = case["rows"]
        if len(rows) < 240 or positive(case["capture"]["non_background_pixels"]) < 1:
            raise ValueError("Missing real native LOD frames or nonblank capture")
        ids = [row["sample_id"] for row in rows]
        if any(
            isinstance(value, bool) or not isinstance(value, int) or value <= 0
            for value in ids
        ) or len(set(ids)) != len(ids):
            raise ValueError("Repeated or invalid native LOD originating frame IDs")
        if sum(positive(row["completed_frame_ns"]) for row in rows) < 2_000_000_000:
            raise ValueError("Require two seconds of completed LOD work per case")
        for row in rows:
            draws = row.get("render_status", [])
            if stochastic:
                if not draws or any(
                    draw.get("projected_stochastic_ready") is not True
                    or draw.get("stochastic_ready") is not True
                    or draw.get("ready") is not False
                    or draw.get("projected_ready") is not False
                    or draw.get("sort_encoded") != 0
                    or nonnegative(draw.get("cloud_count", 0)) < 1
                    or draw["cloud_count"] != draw.get("stochastic_draw_encoded")
                    or draw["cloud_count"] != draw.get("draw_encoded")
                    or draw["cloud_count"] != draw.get("projected_draw_encoded")
                    for draw in draws
                ):
                    raise ValueError(
                        "Stochastic LOD requires projected direct draws and zero sorts"
                    )
            elif (
                row.get("acknowledged_projected_sort_and_draw") is not True
                or not draws
                or any(
                    draw.get("projected_ready") is not True
                    or draw.get("ready") is not True
                    or nonnegative(draw.get("cloud_count", 0)) < 1
                    or draw["cloud_count"] != draw.get("sort_encoded")
                    or draw["cloud_count"] != draw.get("draw_encoded")
                    or draw["cloud_count"] != draw.get("projected_draw_encoded")
                    for draw in draws
                )
            ):
                raise ValueError(
                    "Native LOD measured frame lacks actual projected draw acknowledgement"
                )
            snapshot = row.get("leaf_snapshot", {})
            stream = row.get("stream", {})
            if (
                (not stochastic and row.get("acknowledged_sort_and_draw") is not True)
                or snapshot.get("backend") != "packed"
                or not snapshot.get("scenes")
                or nonnegative(snapshot.get("gaussian_cameras", 0)) < 1
                or nonnegative(stream.get("displayed", 0)) < 1
            ):
                raise ValueError("Missing production packed LOD ready-frame evidence")
        reports = case["gpu_pass_reports"]
        coverage = case["gpu_pass_coverage"]
        if not isinstance(reports, list) or not isinstance(coverage, dict):
            raise ValueError("GPU pass availability and coverage must be explicit")
        for gpu in reports:
            if gpu["scope"] not in coverage or coverage[gpu["scope"]].get(
                "measured_frames"
            ) != len(rows):
                raise ValueError(
                    "GPU pass coverage must explicitly match measured LOD frames"
                )


def device_resource_metrics(report):
    usage = report.get("device_resource_usage")
    if not usage:
        return {}
    result = {}
    seen = set()
    for point in usage["checkpoints"]:
        phase = point["phase"]
        if phase in seen:
            raise ValueError("Duplicate device-resource checkpoint")
        seen.add(phase)
        measurement = point["measurement"]
        value = measurement.get("bytes")
        if value is None:
            if not measurement.get("reason"):
                raise ValueError(
                    "Missing device-resource counter needs unavailable reason"
                )
            continue
        probe = measurement.get("allocation_probe") or {}
        original = usage.get("allocation_probe", {}).get("allocation_probe")
        adapter = report["environment"]["adapter"]
        if (
            measurement.get("metric") != "metal_device_resource_allocated_bytes"
            or measurement.get("reason") is not None
            or adapter.get("backend") != "Metal"
            or measurement.get("matched_device_name") != adapter["name"]
            or measurement.get("method")
            != "MTLCreateSystemDefaultDevice().currentAllocatedSize()"
            or probe.get("passed") is not True
            or probe != original
            or probe.get("requested_bytes") != 1024 * 1024
            or probe.get("observed_delta_bytes") != 1024 * 1024
            or nonnegative(probe.get("after_bytes"))
            - nonnegative(probe.get("before_bytes"))
            != 1024 * 1024
        ):
            raise ValueError("Unverified native Metal device-resource counter")
        result[f"memory/native-device-resource-allocated/{phase}"] = {
            "device-resource-bytes": {"value": positive(value)}
        }
    return result


def resource_metrics(report):
    # The common helper verifies OS executable binding. The LOD inventory has
    # exact shared-buffer identities rather than the flat decoder plane schema.
    common = dict(report)
    common.pop("resource_usage", None)
    result = native_resources(common)
    result.update(device_resource_metrics(report))
    resources = report.get("resource_usage", {})
    if not resources:
        return result
    if resources.get("schema_version") != 1:
        raise ValueError("Unknown native LOD resource schema")

    def add(name, value, measure):
        if value is not None:
            result[name] = {measure: {"value": nonnegative(value)}}

    seen = set()
    for point in resources.get("checkpoints", []):
        phase = point["phase"]
        if phase in seen:
            raise ValueError("Duplicate memory checkpoint")
        seen.add(phase)
        add(
            f"memory/process-rss/{phase}",
            point.get("rss_bytes"),
            "process-memory-bytes",
        )
        add(
            f"memory/process-virtual/{phase}",
            point.get("virtual_memory_bytes"),
            "process-memory-bytes",
        )
        cpu = point.get("accumulated_cpu_ms")
        if cpu is not None and nonnegative(cpu) > 0:
            add(f"processing/process-cpu-cumulative/{phase}", cpu * 1e6, "latency")
    seen = set()
    for point in resources.get("gpu_checkpoints", []):
        phase = point["phase"]
        if phase in seen:
            raise ValueError("Duplicate GPU buffer checkpoint")
        seen.add(phase)
        buffers = point.get("telemetry", {}).get("buffers")
        if buffers is None:
            continue
        entries = buffers.get("entries", [])
        ids = [entry["id"] for entry in entries]
        if len(set(ids)) != len(ids):
            raise ValueError("GPU buffer inventory contains duplicate identities")
        total = sum(nonnegative(entry["bytes"]) for entry in entries)
        live = nonnegative(buffers["live_bytes"])
        count = nonnegative(buffers["live_buffers"])
        if buffers.get("entries_truncated", 0) == 0 and (
            total != live or len(entries) != count
        ):
            raise ValueError(
                "GPU buffer live capacity does not match deduplicated inventory"
            )
        if (
            nonnegative(buffers["peak_live_bytes"]) < live
            or nonnegative(buffers["observed_bytes_total"]) < live
        ):
            raise ValueError(
                "GPU buffer lifetime capacity is smaller than current inventory"
            )
        add(f"memory/tracked-gpu-buffers/{phase}", live, "gpu-buffer-bytes")
        add(
            f"memory/peak-observed-gpu-buffers/{phase}",
            buffers["peak_live_bytes"],
            "gpu-buffer-bytes",
        )
        add(
            f"memory/lifetime-observed-gpu-buffers/{phase}",
            buffers["observed_bytes_total"],
            "gpu-buffer-bytes",
        )
        add(f"memory/tracked-gpu-buffer-count/{phase}", count, "count")
        add(
            f"memory/gpu-source-payload/{phase}",
            buffers.get("packed_source_payload_bytes"),
            "cache-payload-bytes",
        )
        # Category totals overlap through aliased BufferIds; publish separately.
        # They must never be added to deduplicated live_bytes or source payload.
        for category, observation in buffers.get("categories", {}).items():
            add(
                f"memory/gpu-buffer-category/{category}/{phase}",
                observation["bytes"],
                "gpu-buffer-bytes",
            )
    return result


def metrics(report):
    result = resource_metrics(report)
    for key, source in (
        ("loading/read-manifest", report["source"].get("source_prepare_ns")),
        ("loading/renderer-setup", report["environment"].get("renderer_setup_ns")),
    ):
        if source is not None and nonnegative(source) > 0:
            result[key] = {"latency": {"value": source}}
    for case in report["cases"]:
        name = case["name"]
        frames = case["rows"]
        for field, label in (
            ("completed_frame_ns", "completed-frame"),
            ("app_update_wall_ns", "app-update-wall"),
        ):
            for quantile, value in distribution(
                [frame[field] for frame in frames]
            ).items():
                result[f"{name}/{label}/{quantile}"] = {"latency": {"value": value}}
        result.update(gpu_pass_metrics(case))
        first = case.get("first_acknowledged_completed_draw_ns")
        if first is not None and nonnegative(first) > 0:
            result[f"{name}/loading/first-completed-draw"] = {
                "latency": {"value": first}
            }
        for field in (
            "displayed",
            "selected",
            "effective",
            "requested",
            "pending_downloads",
        ):
            values = [
                nonnegative(frame["stream"][field])
                for frame in frames
                if field in frame["stream"]
            ]
            if values and len(values) != len(frames):
                raise ValueError("Incomplete LOD quality observations")
            if values:
                result[f"{name}/quality/{field}/median"] = {
                    "count": {"value": statistics.median(values)}
                }
        for field in ("cache_ready_bytes", "cache_reserved_bytes"):
            values = [
                nonnegative(frame["stream"][field])
                for frame in frames
                if field in frame["stream"]
            ]
            if values and len(values) != len(frames):
                raise ValueError("Incomplete LOD cache observations")
            if values:
                result[f"{name}/memory/{field.replace('_', '-')}/peak"] = {
                    "cache-payload-bytes": {"value": max(values)}
                }
        stages = frames[-1].get("cpu_timing_report", {}).get("cpu_stages") or {}
        for stage, observation in stages.items():
            prefix = (
                f"{name}/cpu-stage/{stage.replace('_', '-')}/cumulative-at-case-end"
            )
            result[prefix + "/attempts"] = {
                "count": {"value": nonnegative(observation["attempts"])}
            }
            wall = observation.get("total_ms")
            if wall is not None and nonnegative(wall) > 0:
                result[prefix + "/wall-sum"] = {"latency": {"value": wall * 1e6}}
    return result


def results(reports, *, aggregate, identity, report_paths=None):
    if len(reports) < 3:
        raise ValueError("Require at least three independent native LOD processes")
    if report_paths is None:
        report_paths = [None] * len(reports)
    if len(report_paths) != len(reports):
        raise ValueError("Missing native LOD saved result paths")
    first = reports[0]
    expected = None
    values = []
    for report, result_path in zip(reports, report_paths):
        validate(report, result_path=result_path)
        if expected is None:
            expected = copy.deepcopy(identity(report))
        if (
            identity(report) != expected
            or producer(report) != producer(first)
            or report["source"]["sha256"] != first["source"]["sha256"]
        ):
            raise ValueError(
                "Incomparable native LOD runs: producer, source or environment changed"
            )
        values.append(metrics(report))
    return {
        **aggregate(values),
        "identity": expected,
        "commit": first["environment"]["git_commit"],
    }
