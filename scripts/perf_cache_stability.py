#!/usr/bin/env python3
"""Repeat cache timing experiments without changing the v1 regression gate."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE_FILES = (
    "Cargo.toml", "Cargo.lock", "tools/perf/Cargo.toml",
    "tools/perf/benches/cache-admission-diagnostic.rs",
    "crates/splat/src/streaming/runtime/test_support/benchmarks.rs",
    "crates/splat/src/streaming/runtime/cache.rs",
    "scripts/perf_cache_stability.py",
)
ADMISSION = "cache-admission-v2/1024"
TEARDOWN = "cache-teardown-v2/1024"
NAMES = (
    "diagnostic-cache-v1/legacy-small-input",
    "diagnostic-cache-v1/admit-only-small-input",
    "diagnostic-cache-v1/admit-only-per-iteration",
    ADMISSION,
    TEARDOWN,
)


def positive(value):
    return type(value) in (float, int) and math.isfinite(value) and value > 0


def load_repeat(directory):
    rows = {}
    for path in directory.glob("**/new/benchmark.json"):
        name = json.loads(path.read_text())["full_id"]
        if name not in NAMES or name in rows:
            raise ValueError("Unexpected or duplicate cache workload")
        samples = json.loads(path.with_name("sample.json").read_text())
        iters, times = samples["iters"], samples["times"]
        if (
            len(iters) < 100
            or len(iters) != len(times)
            or not all(positive(n) for n in iters + times)
        ):
            raise ValueError("Cache workload needs at least 100 valid raw samples")
        estimates = json.loads(path.with_name("estimates.json").read_text())
        # Match Criterion's typical estimate: slope for linear sampling, mean
        # for flat sampling. Keep the raw data and estimator identity as well.
        estimator = "slope" if estimates.get("slope") is not None else "mean"
        estimate = estimates[estimator]
        point = estimate["point_estimate"]
        interval = estimate["confidence_interval"]
        lower, upper = interval["lower_bound"], interval["upper_bound"]
        if (
            not all(positive(n) for n in (point, lower, upper))
            or not lower <= point <= upper
            or interval["confidence_level"] != 0.95
        ):
            raise ValueError("Invalid cache estimate or 95% confidence interval")
        rows[name] = dict(
            point_ns=point, lower_ns=lower, upper_ns=upper,
            estimator=estimator, samples=len(iters),
        )
    if set(rows) != set(NAMES):
        raise ValueError("Incomplete cache workload inventory")
    return rows


def summarize(repeats):
    if len(repeats) < 3 or any(set(row) != set(NAMES) for row in repeats):
        raise ValueError("Need at least three complete cache processes")
    summary = {}
    for name in NAMES:
        points = [row[name]["point_ns"] for row in repeats]
        cv = statistics.stdev(points) / statistics.mean(points)
        ratio = max(points) / min(points)
        widths = [
            (row[name]["upper_ns"] - row[name]["lower_ns"]) / row[name]["point_ns"]
            for row in repeats
        ]
        summary[name] = dict(
            process_estimates_ns=points, process_cv=cv, max_min_ratio=ratio,
            relative_interval_widths=widths,
            repeatable=cv <= 0.05 and ratio <= 1.15 and max(widths) <= 0.10,
        )
    return summary


def markdown(summary):
    lines = [
        "### Cache timing repeatability", "",
        "Diagnostic only: the v1 regression gate remains unchanged.", "",
        "| Workload | Process estimates (us) | Process CV | Max/min | Repeatable |",
        "| --- | --- | ---: | ---: | --- |",
    ]
    for name in NAMES:
        row = summary[name]
        points = ", ".join(f"{value / 1000:.3f}" for value in row["process_estimates_ns"])
        lines.append(
            f"| `{name}` | {points} | {row['process_cv']:.1%} | "
            f"{row['max_min_ratio']:.3f} | {'yes' if row['repeatable'] else 'no'} |"
        )
    return "\n".join(lines) + "\n"


def run(output, runs):
    if not 3 <= runs <= 10:
        raise ValueError("Use 3 to 10 independent benchmark processes")
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source_hashes = {
        name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
        for name in SOURCE_FILES
    }
    env = {
        k: v for k, v in os.environ.items()
        if not k.startswith(("BENCHER_", "GH_", "GITHUB_TOKEN", "ACTIONS_", "FNOX_"))
    }
    command = [
        "cargo", "bench", "--locked", "-p", "dimensify_perf", "--features", "pipeline",
        "--bench", "cache-admission-diagnostic", "--no-run", "--message-format=json",
    ]
    with (output / "build.log").open("w") as log:
        build = subprocess.run(command, cwd=ROOT, env=env, text=True,
                               stdout=subprocess.PIPE, stderr=log)
    (output / "build.jsonl").write_text(build.stdout)
    if build.returncode:
        raise ValueError("Cache harness build failed; see build.log")
    executables = [
        row["executable"] for line in build.stdout.splitlines()
        if (row := json.loads(line)).get("reason") == "compiler-artifact"
        and row.get("target", {}).get("name") == "cache-admission-diagnostic"
        and row.get("executable")
    ]
    if len(executables) != 1:
        raise ValueError("Expected exactly one built cache harness")
    executable = Path(executables[0])
    digest = hashlib.sha256(executable.read_bytes()).hexdigest()
    arguments = ["--bench", "--noplot", "--sample-size", "100",
                 "--warm-up-time", "3", "--measurement-time", "10"]
    environment = dict(
        commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
        rust=subprocess.check_output(["rustc", "--version"], cwd=ROOT, text=True).strip(),
        system=platform.platform(), arch=platform.machine(),
        cpu=(subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip()
             if platform.system() == "Darwin" else next(
                 (line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                  if line.startswith("model name")), "unknown")),
        runner_image=os.environ.get("ImageVersion", "local"),
        run_id=os.environ.get("GITHUB_RUN_ID"), attempt=os.environ.get("GITHUB_RUN_ATTEMPT"),
        binary_sha256=digest, arguments=arguments, processes=runs,
        profile="bench (opt-level=3, thin LTO, codegen-units=1)",
        source_sha256=source_hashes,
        logical_cpus=os.cpu_count(),
        load_average_before=list(os.getloadavg()) if hasattr(os, "getloadavg") else None,
        timestamp=datetime.now(timezone.utc).isoformat(),
    )
    (output / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
    repeats = []
    for i in range(runs):
        directory = output / f"process-{i + 1}"
        directory.mkdir()
        env["CRITERION_HOME"] = str(directory / "criterion")
        print(f"Cache stability process {i + 1}/{runs}", flush=True)
        with (directory / "criterion.log").open("w") as log:
            subprocess.run([str(executable), *arguments], cwd=ROOT, env=env,
                           stdout=log, stderr=subprocess.STDOUT, check=True)
        repeats.append(load_repeat(directory / "criterion"))
    if hashlib.sha256(executable.read_bytes()).hexdigest() != digest:
        raise ValueError("Cache harness changed during measurement")
    if any(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() != digest
           for name, digest in source_hashes.items()):
        raise ValueError("Cache benchmark sources changed during measurement")
    environment["load_average_after"] = (
        list(os.getloadavg()) if hasattr(os, "getloadavg") else None
    )
    (output / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")
    report = dict(
        schema="dimensify-cache-stability/1", status="diagnostic-only",
        criteria="Process CV <= 5%, max/min <= 1.15, each 95% CI width <= 10%; not a regression threshold",
        repeats=repeats, summary=summarize(repeats),
    )
    (output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    (output / "summary.md").write_text(markdown(report["summary"]))
    print(json.dumps(report["summary"], indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/cache-stability")
    parser.add_argument("--runs", type=int, default=3)
    options = parser.parse_args()
    run(options.output, options.runs)
