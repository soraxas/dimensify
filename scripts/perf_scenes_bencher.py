"""Name, publish and check real-scene histories on Bencher."""

import hashlib
import json
import math
import os
import re
import subprocess

from perf import validate_report as check_bencher_report
from perf_scenes_common import CUSTOM_MEASURES, canonical, validate_bmf
from perf_testbeds import scene_name


def testbed(identity, legacy=False):
    if legacy:
        return (
            f"real-{identity['kind']}-v1-"
            + hashlib.sha256(canonical(identity).encode()).hexdigest()[:20]
        )
    return scene_name(identity)


def benchmark_prefix(summary):
    version = 2 if summary.get("schema") == "dimensify-real-scene/2" else 1
    prefix = f"real-scene-v{version}/{summary['dataset_id']}/{summary['asset_identity'][7:19]}/{summary['preset']}"
    if summary["identity"].get("render", {}).get("transparency") == "stochastic":
        prefix += "/stochastic"
    camera = summary["identity"].get("render", {}).get("camera_input")
    if camera:
        prefix += "/camera-" + camera["sha256"][:12]
    if version == 2:
        identity = summary["identity"]
        keys = (
            ("render", "protocol", "gpu_timestamps", "rustc")
            if identity["kind"] == "native"
            else ("protocol", "settings")
        )
        workload = {key: identity.get(key) for key in keys}
        if "resource_instrumentation" in identity:
            workload["resource_instrumentation"] = identity["resource_instrumentation"]
        if identity["kind"] == "browser":
            environment = identity["browser_environment"]
            workload["viewport"] = {
                key: environment.get(key) for key in ("dpr", "canvas")
            }
        prefix += (
            "/workload-" + hashlib.sha256(canonical(workload).encode()).hexdigest()[:12]
        )
    return prefix


def bencher_name(full):
    """Avoid the service's lossy long-name slug generation and collisions."""
    parts = full.split("/")
    dataset = parts[1].split("-")[0][:9]
    label = (
        "-".join(
            part
            for part in parts[4:]
            if not re.fullmatch(r"(?:camera|workload)-[a-f0-9]{12}", part)
        )
        .removesuffix("-v1")
        .removeprefix("browser-")
    )
    for before, after in (
        ("renderer-encoded-interval", "encoded"),
        ("renderer-encoded-ready", "encoded-ready"),
        ("browser-raf-interval", "raf"),
        ("app-interval", "app"),
        ("static-near-side", "near0"),
        ("moving-near-side", "near1"),
        ("static-main", "main0"),
        ("moving-main", "main1"),
        ("gpu-radix-and-compaction", "gpu-sort"),
        ("completed-frame", "frame"),
        ("app-update-wall", "update"),
        ("whole-replay-cpu-", "cpu-"),
        ("main-static", "main0"),
        ("main-moving", "main1"),
        ("side-static", "side0"),
        ("side-moving", "side1"),
        ("observed-", ""),
        ("camera-", "cam-"),
    ):
        label = label.replace(before, after)
    if not full.startswith("observed-"):
        label = label.replace("median", "med")
    label = re.sub(r"[^a-z0-9-]+", "-", label.lower()).strip("-")
    version = "2" if "real-scene-v2/" in full else "1"
    scope = ("obs" if full.startswith("observed-") else "rs") + version
    prefix = f"{scope}-{dataset}-{parts[3][0]}-"
    room = 37 - len(prefix)
    if len(label) > room:
        label = label[: room - 9] + "-" + label[-8:]
    return prefix + label + "-" + hashlib.sha256(full.encode()).hexdigest()[:10]


def validate_report(report, names, check):
    # Byte/count series are histories; regression thresholds apply to latency.
    expected = (
        names if isinstance(names, dict) else {name: {"latency": {}} for name in names}
    )
    if report.get("alerts") or len(report.get("results", [])) != 1:
        raise ValueError("Bencher reported alerts or an incomplete iteration")
    results = report["results"][0]
    aliases = {}
    for name in expected:
        # Bencher collapses repeated dashes in the slug, but preserves them in
        # the initial display name. Keep that stable alias after a human rename.
        slug = (
            re.sub(r"-+", "-", name).strip("-")
            if re.fullmatch(r"[a-z0-9-]+", name)
            else name
        )
        for alias in (name, slug):
            aliases.setdefault(alias, set()).add(name)
    actual = []
    for result in results:
        benchmark = result["benchmark"]
        matches = set().union(
            *(
                aliases.get(benchmark.get(key), set())
                for key in ("name", "slug", "uuid")
            )
        )
        if len(matches) != 1:
            raise ValueError("Incomplete or unexpected Bencher metrics")
        actual.append(matches.pop())
    if set(actual) != set(expected) or len(actual) != len(expected):
        raise ValueError("Incomplete or unexpected Bencher metrics")
    latency_results = []
    for name, result in zip(actual, results):
        expected_measure = next(iter(expected[name]))
        measures = result["measures"]
        if len(measures) != 1 or measures[0]["measure"]["slug"] != expected_measure:
            raise ValueError("Bencher returned a different measure")
        item = measures[0]
        value = (
            item["metric"]["value"]
            if "metric" in item
            else next(m["value"] for m in item["metrics"] if m["name"] == "value")
        )
        if "value" in expected[name][expected_measure] and not math.isclose(
            value,
            expected[name][expected_measure]["value"],
            rel_tol=1e-12,
            abs_tol=1e-9,
        ):
            raise ValueError("Bencher returned a different measurement value")
        if expected_measure == "latency":
            latency_results.append(
                {**result, "benchmark": {**result["benchmark"], "name": name}}
            )
        else:
            item = measures[0]
            value = (
                item["metric"]["value"]
                if "metric" in item
                else next(m["value"] for m in item["metrics"] if m["name"] == "value")
            )
            validate_bmf({name: {expected_measure: {"value": value}}})
    if latency_results:
        check_bencher_report(
            {"results": [latency_results], "alerts": []},
            require_baseline=check,
            names={result["benchmark"]["name"] for result in latency_results},
        )


def diagnostic_publication(summary, metrics, check):
    if summary["kind"] != "browser" or check:
        raise ValueError(
            "Diagnostic browser history cannot act as a hardware regression gate"
        )
    target = (
        ("observed-" + summary["testbed"])
        if summary.get("schema") == "dimensify-real-scene/2"
        else summary["testbed"].replace("real-browser-v1-", "observed-browser-v1-", 1)
    )
    if len(target) > 64:
        raise ValueError("Diagnostic testbed name exceeds 64 characters")
    profiling = summary.get("identity", {}).get("protocol", {}).get("workerProfiling")
    branch = (
        "worker-profiles"
        if profiling and profiling != "disabled"
        else "browser-diagnostics"
    )
    return {
        "branch": branch,
        "testbed": target,
        "metrics": {
            name.replace("real-scene-v", "observed-real-scene-v", 1): value
            for name, value in metrics.items()
        },
    }


def validate_diagnostic(report):
    metadata = report["metadata"]
    if metadata["sourceState"] != "clean" or metadata["profile"] != "release":
        raise ValueError("Modified browser builds cannot enter diagnostic history")
    for adapter in [metadata["adapter"], *(run["adapter"] for run in report["runs"])]:
        if (
            adapter.get("backend") != "BrowserWebGpu"
            or re.search(
                r"swiftshader|llvmpipe|lavapipe|software|microsoft basic render",
                canonical(adapter),
                re.I,
            )
            or str(adapter.get("device_type")).lower() == "cpu"
        ):
            raise ValueError(
                "Browser diagnostic history excludes known software adapters"
            )


def service_error(stderr):
    for key, value in os.environ.items():
        if (
            any(part in key for part in ("KEY", "TOKEN", "PASSWORD", "SECRET"))
            and len(value) > 5
        ):
            stderr = stderr.replace(value, "[REDACTED]")
    stderr = re.sub(r"bencher_(?:user|run)_[A-Za-z0-9_-]+", "[REDACTED]", stderr)
    stderr = re.sub(r"(?i)Bearer\s+\S+", "Bearer [REDACTED]", stderr)
    return stderr[-2000:]


def ensure_measures(project, names):
    if not names:
        return
    existing = subprocess.run(
        ["bencher", "measure", "list", project, "--per-page", "100"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if existing.returncode:
        raise ValueError("Cannot inspect Bencher custom measures")
    found = {measure["slug"]: measure for measure in json.loads(existing.stdout)}
    units = {**CUSTOM_MEASURES, "count": "count"}
    for name in sorted(names):
        if name in found:
            if found[name]["units"] != units[name]:
                raise ValueError(f"Unexpected units for Bencher measure {name}")
            continue
        result = subprocess.run(
            [
                "bencher",
                "measure",
                "create",
                project,
                "--name",
                name,
                "--slug",
                name,
                "--units",
                units[name],
                "--attempts",
                "3",
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        if result.returncode:
            raise ValueError(
                f"Cannot create Bencher measure {name}; configure it with units {units[name]}"
            )
