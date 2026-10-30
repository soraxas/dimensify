"""Shared real-scene constants, file identity helpers and BMF value contract."""

import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import statistics

from perf_resources import BYTE_MEASURES

ROOT = Path(__file__).resolve().parents[1]
CUSTOM_MEASURES = {**BYTE_MEASURES, "sampled-profile-time": "nanoseconds"}
CASES = {"main-static", "main-moving", "side-static", "side-moving"}


def digest(path):
    with Path(path).open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def inside(root, relative):
    part = PurePosixPath(relative)
    if (
        not relative
        or part.is_absolute()
        or any(x in (".", "..") for x in part.parts)
        or "\\" in relative
        or ":" in relative
    ):
        raise ValueError("Unsafe fixture path")
    path = (root / relative).resolve()
    if not path.is_relative_to(root):
        raise ValueError("Fixture path escapes root")
    return path


def positive(value):
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
        or value <= 0
    ):
        raise ValueError("Expected finite positive timing")
    return value


def distribution(values):
    values = sorted(positive(value) for value in values)
    if not values:
        raise ValueError("Missing frame samples")
    return {
        "median": statistics.median(values),
        "p95": values[math.ceil(len(values) * 0.95) - 1],
        "p99": values[math.ceil(len(values) * 0.99) - 1],
    }


def validate_bmf(data):
    if not data:
        raise ValueError("Empty BMF cannot be published")
    for name, measures in data.items():
        if (
            not name
            or len(measures) != 1
            or not set(measures) <= {"latency", "count", *CUSTOM_MEASURES}
        ):
            raise ValueError("Unsupported measure")
        measure, metric = next(iter(measures.items()))
        if set(metric) != {"value"}:
            raise ValueError(
                "Expected a measured value, not percentile confidence bounds"
            )
        value = metric["value"]
        if measure == "latency":
            positive(value)
        elif (
            isinstance(value, bool)
            or not isinstance(value, (int, float))
            or not math.isfinite(value)
            or value < 0
        ):
            raise ValueError("Expected nonnegative scoped measurement")


def clean_env():
    return {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("BENCHER_", "GH_", "GITHUB_TOKEN", "ACTIONS_", "FNOX_"))
    }
