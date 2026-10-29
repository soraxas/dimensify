"""Rebuild diagnostic worker sampled-profile metrics from raw V8 profiles.

Sample deltas describe elapsed profile attribution, including idle and cancelled
work. They are never an OS CPU clock, job-success measurement or frame latency.
"""
import math
import statistics


def _number(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
        raise ValueError(f"Invalid {label}")
    try:
        finite = math.isfinite(value)
    except OverflowError as error:
        raise ValueError(f"Invalid {label}") from error
    if not finite:
        raise ValueError(f"Invalid {label}")
    return value


def _count(value, label):
    if type(value) is not int or value < 0:
        raise ValueError(f"Invalid {label}")
    return value


def _boolean(value, label):
    if type(value) is not bool:
        raise ValueError(f"Invalid {label}")
    return value


def _raw_profile(profile):
    if not isinstance(profile, dict):
        raise ValueError("Missing raw worker profile")
    start = _number(profile.get("startTime"), "profile start")
    end = _number(profile.get("endTime"), "profile end")
    if end < start:
        raise ValueError("Reversed profile window")
    nodes, samples, deltas = (profile.get(key) for key in ("nodes", "samples", "timeDeltas"))
    if not isinstance(nodes, list) or not nodes or not isinstance(samples, list) or not isinstance(deltas, list) or len(samples) != len(deltas):
        raise ValueError("Missing/inconsistent profile nodes, samples or deltas")
    identities = set()
    for node in nodes:
        if not isinstance(node, dict) or type(node.get("id")) is not int or node["id"] <= 0 or node["id"] in identities:
            raise ValueError("Invalid/duplicate profile node identity")
        if not isinstance(node.get("callFrame"), dict) or not isinstance(node["callFrame"].get("functionName"), str):
            raise ValueError("Missing profile node call frame")
        identities.add(node["id"])
    for node in nodes:
        if "children" in node and (not isinstance(node["children"], list) or any(type(child) is not int or child not in identities for child in node["children"])):
            raise ValueError("Unknown profile child node")
    if any(type(sample) is not int or sample not in identities for sample in samples):
        raise ValueError("Profile sample references missing node")
    values = [_number(delta, "sample delta") for delta in deltas]
    try:
        elapsed = math.fsum(values)
        # The first sampling interval can straddle profiler setup. Tolerate that
        # interval extending outside the window, but never arbitrary later time.
        after_first = math.fsum(values[1:])
    except OverflowError as error:
        raise ValueError("Profile elapsed attribution overflow") from error
    if not math.isfinite(elapsed) or after_first > end - start + 1e-6:
        raise ValueError("Sample deltas exceed profile window beyond first interval")
    return len(samples), elapsed


def _round(profile):
    if (not isinstance(profile, dict) or profile.get("schema") != "dimensify-worker-sampled-profiles/1"
            or profile.get("cohort") != "worker-profile-diagnostic" or profile.get("stopped") is not True
            or "exactWorkerThreadCpuMs" not in profile or profile["exactWorkerThreadCpuMs"] is not None):
        raise ValueError("Missing/incomplete diagnostic worker profile or unsupported exact CPU claim")
    _number(profile.get("samplingIntervalUs"), "sampling interval")
    if profile["samplingIntervalUs"] <= 0:
        raise ValueError("Sampling interval must be positive")
    workers, errors = profile.get("workers"), profile.get("errors")
    if not isinstance(workers, list) or not isinstance(errors, list) or any(not isinstance(error, str) for error in errors):
        raise ValueError("Missing worker/error coverage")
    counts = {"observed-workers": len(workers), "profiled-workers": 0, "segments": 0, "samples": 0,
              "errors": len(errors), "unflushed-tails": 0,
              "parent-termination-requests": _count(profile.get("parentTerminationRequests"), "parent termination requests"),
              "parent-termination-flush-timeouts": _count(profile.get("parentTerminationFlushTimeouts"), "termination flush timeouts"),
              "retention-limit-reached": int(_boolean(profile.get("sampleLimitReached"), "retention limit"))}
    if counts["parent-termination-flush-timeouts"] > counts["parent-termination-requests"]:
        raise ValueError("Termination flush timeouts exceed requests")
    ids, elapsed = set(), 0
    for worker in workers:
        if not isinstance(worker, dict) or not isinstance(worker.get("targetId"), str) or not worker["targetId"] or worker["targetId"] in ids:
            raise ValueError("Missing/duplicate worker target identity")
        ids.add(worker["targetId"])
        counts["unflushed-tails"] += int(_boolean(worker.get("unflushedTail"), "unflushed tail"))
        _boolean(worker.get("detached"), "worker detached status")
        if "error" in worker:
            if not isinstance(worker["error"], str) or not worker["error"]:
                raise ValueError("Invalid worker profile error")
            counts["errors"] += 1
        segments = worker.get("segments")
        if not isinstance(segments, list):
            raise ValueError("Missing worker profile segments")
        worker_samples = 0
        windows = []
        for segment in segments:
            if not isinstance(segment, dict):
                raise ValueError("Invalid profile segment")
            raw = segment.get("profile")
            samples, duration = _raw_profile(raw)
            windows.append((raw["startTime"], raw["endTime"]))
            counts["segments"] += 1
            counts["samples"] += samples
            worker_samples += samples
            elapsed += duration
        ordered = sorted(windows)
        if any(right[0] < left[1] for left, right in zip(ordered, ordered[1:])):
            raise ValueError("Overlapping worker profile segments")
        counts["profiled-workers"] += int(worker_samples > 0)
    if not math.isfinite(elapsed) or not math.isfinite(elapsed * 1000):
        raise ValueError("Profile sampled elapsed overflow")
    return counts, elapsed * 1000


def metrics(report):
    """Return median-per-round BMF; missing sample coverage never invents time."""
    profiles = report.get("workerProfiles") if isinstance(report, dict) else None
    if not isinstance(profiles, list) or len(profiles) < 3:
        raise ValueError("Require at least three independent worker profile rounds")
    rounds, values, elapsed = set(), [], []
    for profile in profiles:
        if not isinstance(profile, dict) or type(profile.get("round")) is not int or profile["round"] <= 0 or profile["round"] in rounds:
            raise ValueError("Missing/duplicate worker profile round")
        rounds.add(profile["round"])
        counts, duration = _round(profile)
        values.append(counts)
        elapsed.append(duration if counts["samples"] else None)
    result = {f"browser-worker-profile-{name}": {"count": {"value": statistics.median(row[name] for row in values)}}
              for name in values[0]}
    if all(value is not None for value in elapsed):
        result["browser-worker-profile-sampled-elapsed"] = {"sampled-profile-time": {"value": statistics.median(elapsed)}}
    return result
