"""Aggregate repeated native runner reports into comparable metrics."""

import json
import re
import statistics

from perf_capture import validate_capture
from perf_resources import gpu_pass_metrics, native_metrics as native_resources
from perf_scenes_common import CASES, canonical, distribution, positive, validate_bmf


def native_identity(report):
    env = report["environment"]
    identity = {
        "kind": "native",
        "protocol": report["protocol"],
        "adapter": env["adapter"],
        "os": env.get("os"),
        "cpu": env.get("cpu"),
        "architecture": env.get("architecture"),
        "rustc": env.get("build_rustc"),
        "render": json.loads(canonical(report["render"])),
        "gpu_timestamps": report["gpu_timestamps_requested"],
    }
    # Newly explicit default metadata must not split established sorted history.
    if identity["render"].get("transparency", "sorted") == "sorted":
        identity["render"].pop("transparency", None)
        identity["render"].pop("stochastic_seed", None)
        identity["render"].pop("stochastic_sample", None)
    if "executable_sha256" in env:
        identity["executable"] = env["executable_sha256"]
    if report.get("process_usage"):
        identity["resource_instrumentation"] = {
            "schema": report["process_usage"]["schema"],
            "method": report["process_usage"].get("method"),
            "platform": report["process_usage"].get("platform"),
        }
    return identity


def aggregate(values):
    keys = set(values[0])
    if any(set(row) != keys for row in values):
        raise ValueError("Incomplete metrics across repeated runs")
    normalized = [
        {
            name: value if isinstance(value, dict) else {"latency": {"value": value}}
            for name, value in row.items()
        }
        for row in values
    ]
    for row in normalized:
        validate_bmf(row)
    measures = {name: next(iter(normalized[0][name])) for name in keys}
    if any(
        next(iter(row[name])) != measures[name] for row in normalized for name in keys
    ):
        raise ValueError("Measurement unit changed between runs")
    repeats = {
        name: [row[name][measures[name]]["value"] for row in normalized]
        for name in sorted(keys)
    }
    result = {
        "metrics": {
            name: {measures[name]: {"value": statistics.median(samples)}}
            for name, samples in repeats.items()
        },
        "repeat_values": repeats,
    }
    if any(measure != "latency" for measure in measures.values()):
        result["repeat_measures"] = measures
    return result


def native_results(reports, report_paths=None):
    """Aggregate frames; production run/upload paths also verify saved captures."""
    if len(reports) < 3:
        raise ValueError("Require at least three independent native processes")
    if report_paths is not None and len(report_paths) != len(reports):
        raise ValueError("Native capture paths must match report count")
    first = reports[0]
    expected = native_identity(first)
    values = []
    for number, report in enumerate(reports):
        env = report["environment"]
        commit = env["git_commit"]
        adapter = env["adapter"]
        if (
            report.get("classification") != "hardware_performance"
            or report.get("publication_eligible") is not True
            or env.get("git_dirty") is not False
            or env.get("git_dirty_after") is not False
            or env.get("build_dirty") != "false"
            or env.get("debug_assertions") is not False
            or env.get("build_opt_level") != "3"
            or env.get("build_commit") != commit
            or env.get("git_commit_after") != commit
            or not re.fullmatch(r"[a-f0-9]{40}", commit)
            or adapter.get("device_type") not in ("IntegratedGpu", "DiscreteGpu")
            or re.search(
                r"swiftshader|llvmpipe|lavapipe|software", canonical(adapter), re.I
            )
        ):
            raise ValueError("Native report is not clean hardware performance evidence")
        if (
            native_identity(report) != expected
            or report["source"]["sha256"] != first["source"]["sha256"]
            or commit != first["environment"]["git_commit"]
        ):
            raise ValueError(
                "Incomparable native runs: source, build or environment changed"
            )
        cases = report["cases"]
        if len(cases) != 4 or {case["name"] for case in cases} != CASES:
            raise ValueError("Missing native camera case")
        initial = next(case for case in cases if case["name"] == "main-static")
        row = {
            "loading/read-input": positive(report["source"]["source_prepare_ns"]),
            "loading/decode-expanded": positive(report["source"]["decode_ns"]),
            "loading/renderer-setup": positive(env["renderer_setup_ns"]),
            "loading/first-completed-draw-after-spawn": positive(
                initial["first_acknowledged_completed_draw_ns"]
            ),
        }
        for case in cases:
            if report_paths is not None:
                validate_capture(report, case, report_paths[number])
            frames = case["rows"]
            if (
                len(frames) < 240
                or any(
                    frame["acknowledged_sort_and_draw"] is not True for frame in frames
                )
                or case["capture"]["non_background_pixels"] <= 0
            ):
                raise ValueError("Missing real native frames or nonblank capture")
            if (
                sum(positive(frame["completed_frame_ns"]) for frame in frames)
                < 2_000_000_000
            ):
                raise ValueError(
                    "Require two seconds of completed work per native case"
                )
            ids = [frame.get("sample_id") for frame in frames]
            if any(type(value) is not int or value <= 0 for value in ids) or len(
                set(ids)
            ) != len(ids):
                raise ValueError("Repeated or invalid native originating frame IDs")
            for frame in frames:
                if positive(frame["app_update_wall_ns"]) > frame["completed_frame_ns"]:
                    raise ValueError(
                        "Native frame app update exceeds completion interval"
                    )
                # The flat protocol creates exactly one cloud and one view.
                if any(
                    type(frame.get(field)) is not int or frame[field] != 1
                    for field in ("sort_encoded", "draw_encoded")
                ):
                    raise ValueError(
                        "Native frame requires exactly one encoded sort and draw"
                    )
            for field, label in (
                ("completed_frame_ns", "completed-frame"),
                ("app_update_wall_ns", "app-update-wall"),
            ):
                for quantile, value in distribution(
                    [frame[field] for frame in frames]
                ).items():
                    row[f"{case['name']}/{label}/{quantile}"] = value
            radix = []
            for frame in frames:
                sample = frame.get("gpu_radix_sample")
                if not sample:
                    continue
                if str(sample.get("identity", {}).get("selection")) != str(
                    frame["sample_id"]
                ):
                    raise ValueError("Native radix sample belongs to another frame")
                measurement = sample.get("measurement", {})
                duration = measurement.get("duration_ms")
                if duration is not None:
                    if measurement.get("failure") is not None:
                        raise ValueError(
                            "Failed native radix timestamp contains a duration"
                        )
                    radix.append(positive(duration) * 1e6)
            if report["gpu_timestamps_requested"] and len(radix) == len(frames):
                for quantile, value in distribution(radix).items():
                    row[f"{case['name']}/gpu-radix-and-compaction/{quantile}"] = value
            row.update(gpu_pass_metrics(case))
        row.update(native_resources(report))
        values.append(row)
    return {
        **aggregate(values),
        "identity": expected,
        "commit": first["environment"]["git_commit"],
    }


def native_lod_results(reports, report_paths=None):
    from perf_lod import results

    return results(
        reports,
        aggregate=aggregate,
        identity=native_identity,
        report_paths=report_paths,
    )
