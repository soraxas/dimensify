"""Sparse observed Gaussian GPU intervals and scoped capacities, in typed BMF.

Use the final warm collector once per physical round: it already retains cold
and warm samples. Distinct completed ordinals are same-scope pass segments; sum
only segments with the same originating render frame/view. A failed segment
invalidates that entire observed frame/view. Different scopes are never added,
so these measurements are neither total-frame GPU time nor presentation time.
Unobserved frames and segments remain unobserved; count metrics expose coverage.

Buffer capacities are deduplicated by actual BufferId upstream and checked here.
Category totals overlap and must never be added. Packed source payload is a
separate logical cache payload measurement, not an actual texture allocation or
resident VRAM measurement. Peaks span cold and warm within each physical round;
publication takes the median of at least three independent round peaks/quantiles.
"""

import math
import re
import statistics


_ID = re.compile(r"^[0-9]+$")
_SCOPE = re.compile(r"^[a-z][a-z0-9_\-]*$")
_SAFE_JS_INTEGER = (1 << 53) - 1
_CAPABILITIES = {
    "initializing",
    "unsupported_timestamp_query",
    "invalid_timestamp_period",
    "timestamp_query",
    "device_failed",
}


def _number(value, label):
    try:
        finite = math.isfinite(value)
    except (TypeError, OverflowError):
        finite = False
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not finite
        or value < 0
    ):
        raise ValueError(f"Invalid finite nonnegative {label}")
    return value


def _count(value, label):
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise ValueError(f"Invalid nonnegative integer {label}")
    return value


def _identity(value):
    if isinstance(value, bool) or not isinstance(value, (str, int)):
        raise ValueError("Invalid GPU identity")
    if isinstance(value, int) and value > _SAFE_JS_INTEGER:
        raise ValueError("Unsafe numeric GPU identity; full-width IDs must be strings")
    if not _ID.fullmatch(str(value)) or int(value) > (1 << 64) - 1:
        raise ValueError("Invalid GPU identity")
    return str(int(value))


def _label(value):
    if not isinstance(value, str) or not _SCOPE.fullmatch(value):
        raise ValueError("Invalid GPU scope/category")
    return value.replace("_", "-")


def _add(result, name, values, measure):
    if name in result:
        raise ValueError("Colliding GPU metric labels")
    value = statistics.median(values)
    _number(value, "BMF measurement")
    result[name] = {measure: {"value": value}}


def _timing_round(run):
    collector = run.get("gpuSamples")
    if collector is None:
        return {}, {}, None
    if not isinstance(collector, dict):
        raise ValueError("Invalid GPU collector")
    schema = collector.get("schema")
    if schema is not None and schema != "dimensify-browser-gpu-samples/1":
        raise ValueError("Unknown browser GPU sample schema")
    reports = {}
    for report in collector.get("reports", []):
        scope = report["scope"]
        _label(scope)
        if scope in reports:
            raise ValueError("Duplicate GPU scope report")
        if report.get("capability") not in _CAPABILITIES:
            raise ValueError("Unknown GPU capability")
        for field in (
            "completed",
            "invalid",
            "dropped_busy",
            "evicted_history",
            "pending",
        ):
            if field in report:
                _count(report[field], field)
        if "completed" not in report:
            raise ValueError("GPU completed counter missing")
        if (
            report.get("invalid", 0) > report["completed"]
            or report.get("evicted_history", 0) > report["completed"]
        ):
            raise ValueError("GPU history counters exceed completed samples")
        reports[scope] = report
    grouped = {scope: {} for scope in reports}
    ordinals = set()
    counts = {scope: {"valid-segments": 0, "invalid-segments": 0} for scope in reports}
    for sample in collector.get("samples", []):
        scope = sample["scope"]
        if scope not in reports or reports[scope]["capability"] != "timestamp_query":
            raise ValueError("GPU sample has no supported capability report")
        ordinal = _count(sample["ordinal"], "GPU ordinal")
        if ordinal >= reports[scope]["completed"]:
            raise ValueError("GPU ordinal exceeds completed history")
        if (scope, ordinal) in ordinals:
            raise ValueError("Duplicate GPU sample ordinal")
        ordinals.add((scope, ordinal))
        identity = sample["identity"]
        frame, view, scene, selection = (
            _identity(identity[field])
            for field in ("frame", "view", "scene", "selection")
        )
        measurement = sample["measurement"]
        duration = measurement.get("duration_ms")
        if duration is not None:
            _number(duration, "GPU duration")
        valid = measurement.get("failure") is None and duration is not None
        counts[scope]["valid-segments" if valid else "invalid-segments"] += 1
        # Scene IDs intentionally do not partition a frame: multiple observed
        # Gaussian scenes can contribute distinct segments of the same pass.
        group = grouped[scope].setdefault(
            (frame, view), {"selection": selection, "valid": True, "duration": 0}
        )
        if group["selection"] != selection:
            raise ValueError("Inconsistent originating selection in GPU frame/view")
        group["valid"] &= valid
        if valid:
            group["duration"] += duration
            _number(group["duration"], "summed GPU duration")
    rounds = {}
    for scope, report in reports.items():
        if report["capability"] != "timestamp_query":
            continue
        frames = list(grouped[scope].values())
        values = [
            _number(frame["duration"] * 1e6, "GPU duration in nanoseconds")
            for frame in frames
            if frame["valid"]
        ]
        observed = counts[scope]["valid-segments"] + counts[scope]["invalid-segments"]
        if (
            report.get("invalid", counts[scope]["invalid-segments"])
            < counts[scope]["invalid-segments"]
        ):
            raise ValueError(
                "GPU invalid counter smaller than observed invalid segments"
            )
        rounds[scope] = {
            **counts[scope],
            "valid-frames": len(values),
            "invalid-frames": len(frames) - len(values),
            "unretained-completed-segments": report["completed"] - observed,
        }
        for field in (
            "completed",
            "invalid",
            "dropped_busy",
            "evicted_history",
            "pending",
        ):
            if field in report:
                rounds[scope][field.replace("_", "-")] = report[field]
        if values:
            values.sort()
            rounds[scope]["quantiles"] = {
                "median": statistics.median(values),
                "p95": values[math.ceil(len(values) * 0.95) - 1],
                "p99": values[math.ceil(len(values) * 0.99) - 1],
            }
    capabilities = {scope: report["capability"] for scope, report in reports.items()}
    return (
        capabilities,
        rounds,
        _count(collector.get("historyGaps", 0), "GPU history gaps"),
    )


def _buffer_point(point):
    if not isinstance(point, dict):
        raise ValueError("Invalid GPU buffer observation")
    for field in (
        "live_bytes",
        "live_buffers",
        "peak_live_bytes",
        "observed_bytes_total",
        "observed_buffers_total",
        "entries_truncated",
    ):
        _count(point[field], "GPU buffer " + field)
    if (
        point["peak_live_bytes"] < point["live_bytes"]
        or point["observed_bytes_total"] < point["live_bytes"]
        or point["observed_buffers_total"] < point["live_buffers"]
    ):
        raise ValueError("GPU buffer lifetime counters smaller than live capacities")
    ids, sizes, category_bytes, category_counts = set(), 0, {}, {}
    for entry in point["entries"]:
        identity = entry["id"]
        if not isinstance(identity, str) or not identity or identity in ids:
            raise ValueError("Duplicate or invalid GPU buffer identity")
        ids.add(identity)
        size = _count(entry["bytes"], "GPU buffer entry bytes")
        sizes += size
        categories = entry["categories"]
        if not isinstance(categories, list) or len(set(categories)) != len(categories):
            raise ValueError("Duplicate GPU buffer category")
        for category in categories:
            _label(category)
            category_bytes[category] = category_bytes.get(category, 0) + size
            category_counts[category] = category_counts.get(category, 0) + 1
    if len(ids) + point["entries_truncated"] != point["live_buffers"]:
        raise ValueError("GPU buffer entry count does not match live count")
    if sizes > point["live_bytes"] or (
        not point["entries_truncated"] and sizes != point["live_bytes"]
    ):
        raise ValueError("GPU buffer entry sizes do not match live capacity")
    categories = point["categories"]
    for name, row in categories.items():
        _label(name)
        size = _count(row["bytes"], "GPU category bytes")
        count = _count(row["buffers"], "GPU category buffers")
        if size > point["live_bytes"] or count > point["live_buffers"]:
            raise ValueError("GPU category exceeds live inventory")
        if size < category_bytes.get(name, 0) or count < category_counts.get(name, 0):
            raise ValueError("GPU category smaller than retained entries")
        if not point["entries_truncated"] and (
            size != category_bytes.get(name, 0) or count != category_counts.get(name, 0)
        ):
            raise ValueError("GPU category does not match deduplicated entries")
    if not point["entries_truncated"] and not set(category_bytes).issubset(categories):
        raise ValueError("GPU entry category missing from inventory")
    payload = point.get("packed_source_payload_bytes")
    if payload is not None:
        _count(payload, "packed source payload")
    resources = point.get("resources")
    if (
        not isinstance(resources, dict)
        or not resources
        or any(not isinstance(value, bool) for value in resources.values())
    ):
        raise ValueError("GPU buffer resource capability map missing or invalid")
    for name in resources:
        _label(name)
    if not any(resources.values()):
        raise ValueError("GPU buffer observation has no available owner capability")
    return point


def _buffers(result, runs, round_numbers):
    observations = []
    for number in round_numbers:
        points = [
            _buffer_point(observation["gpuBuffers"])
            for run in runs
            if run["round"] == number
            for observation in run.get("observations", [])
            if observation.get("gpuBuffers") is not None
        ]
        observations.append(points)
    if not any(observations):
        return
    if not all(observations):
        raise ValueError("GPU buffer capability changed between rounds")
    resources = observations[0][0]["resources"]
    if any(
        point["resources"] != resources for points in observations for point in points
    ):
        raise ValueError("GPU buffer resource capability changed between rounds")
    payloads = [
        [
            point["packed_source_payload_bytes"]
            for point in points
            if point.get("packed_source_payload_bytes") is not None
        ]
        for points in observations
    ]
    if any(payloads) and not all(payloads):
        raise ValueError("Packed source payload capability changed between rounds")
    fields = (
        ("live_bytes", "live-peak", "gpu-buffer-bytes"),
        ("live_buffers", "count-peak", "count"),
        ("peak_live_bytes", "lifetime-live-peak", "gpu-buffer-bytes"),
        ("observed_bytes_total", "observed-allocated-peak", "gpu-buffer-bytes"),
        ("observed_buffers_total", "observed-allocations-peak", "count"),
        ("entries_truncated", "inventory-truncated-peak", "count"),
    )
    for field, label, measure in fields:
        _add(
            result,
            "browser-observed-gpu-buffer-" + label,
            [max(point[field] for point in points) for points in observations],
            measure,
        )
    category_names = {
        name
        for points in observations
        for point in points
        for name in point["categories"]
    }
    labels = [_label(name) for name in category_names]
    if len(set(labels)) != len(labels):
        raise ValueError("Colliding GPU category metric labels")
    for name in sorted(category_names):
        prefix = "browser-observed-gpu-buffer-" + _label(name)
        for field, suffix, measure in (
            ("bytes", "-peak", "gpu-buffer-bytes"),
            ("buffers", "-count-peak", "count"),
        ):
            _add(
                result,
                prefix + suffix,
                [
                    max(
                        point["categories"].get(name, {}).get(field, 0)
                        for point in points
                    )
                    for points in observations
                ],
                measure,
            )
    if all(payloads):
        _add(
            result,
            "browser-observed-packed-source-payload-peak",
            [max(values) for values in payloads],
            "cache-payload-bytes",
        )


def metrics(runs):
    """Return typed BMF metrics for at least three rounds; omit unavailable measurements."""
    if not runs:
        return {}
    for run in runs:
        if _count(run["round"], "physical round") == 0 or run["mode"] not in (
            "cold",
            "warm",
        ):
            raise ValueError("Invalid physical round or replay mode")
    round_numbers = sorted({run["round"] for run in runs})
    if len(round_numbers) < 3:
        raise ValueError("GPU metrics require at least three distinct physical rounds")
    warm = []
    for number in round_numbers:
        selected = [
            run for run in runs if run["round"] == number and run["mode"] == "warm"
        ]
        if len(selected) != 1:
            raise ValueError("Expected one final warm GPU collector per round")
        warm.append(selected[0])
    parsed = [_timing_round(run) for run in warm]
    if any(capabilities != parsed[0][0] for capabilities, _, _ in parsed):
        raise ValueError("GPU timing capability changed between rounds")
    result = {}
    scopes = parsed[0][1]
    labels = [_label(scope) for scope in scopes]
    if len(set(labels)) != len(labels):
        raise ValueError("Colliding GPU scope metric labels")
    for scope in sorted(scopes):
        prefix = "browser-observed-gpu-" + _label(scope)
        rounds = [measurements[scope] for _, measurements, _ in parsed]
        fields = set().union(*(item.keys() for item in rounds)) - {"quantiles"}
        for field in sorted(fields):
            if any(field not in item for item in rounds):
                raise ValueError("GPU counter capability changed between rounds")
            _add(
                result, prefix + "-" + field, [item[field] for item in rounds], "count"
            )
        if all("quantiles" in item for item in rounds):
            for label in ("median", "p95", "p99"):
                values = [item["quantiles"][label] for item in rounds]
                # Quantized zero intervals stay valid raw samples and coverage
                # counts; they never turn into fabricated positive latency.
                if statistics.median(values) > 0:
                    _add(result, prefix + "-" + label, values, "latency")
    if scopes:
        _add(
            result,
            "browser-observed-gpu-history-gaps",
            [gaps for _, _, gaps in parsed],
            "count",
        )
    _buffers(result, runs, round_numbers)
    return result
