"""Run CPU benchmarks without credentials; publish saved results separately."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

from perf_testbeds import cpu_name, ensure_name

ROOT = Path(__file__).resolve().parents[1]
NAMES = {
    "sog-decode-v1/65536",
    "depth-sort-v1/100000",
    "depth-sort-v1/1000000",
    "lod-targets-v1/1000",
    "lod-targets-v1/10000",
}
PIPELINE_NAMES = {
    "pipeline-ply-v1/10000",
    "pipeline-ply-v1/100000",
    "pipeline-sog-bundle-v1/65536",
    "pipeline-sog-packed-v1/65536",
    "pipeline-compose-v1/65536",
    "pipeline-metadata-v1/1000",
    "pipeline-metadata-v1/10000",
    "pipeline-lod-selection-v1/1000",
    "pipeline-lod-selection-v1/10000",
    "pipeline-relocation-v1/pressure",
    "pipeline-cache-admission-v1/1024",
    "pipeline-scheduler-v1/resident",
}
UNITS = {"ns": 1, "us": 1000, "µs": 1000, "ms": 1_000_000, "s": 1_000_000_000}


def suite_names(suite):
    if suite not in ("cpu", "pipeline"):
        raise ValueError(f"Unknown benchmark suite: {suite}")
    return NAMES if suite == "cpu" else PIPELINE_NAMES


def positive(value):
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
        or value <= 0
    ):
        raise ValueError(f"Invalid performance measurement: {value!r}")
    return value


def criterion_results(output, names=NAMES):
    """Require exactly the complete suite, including finite confidence bounds."""
    results = {}
    for match in re.finditer(r"(?m)^([\w/-]+)\s+time:\s*\[([^\]]+)\]", output):
        name, interval = match.groups()
        tokens = interval.split()
        if name in results or len(tokens) != 6:
            raise ValueError(f"Duplicate or malformed result: {name}")
        try:
            lower, value, upper = [
                positive(float(tokens[i])) * UNITS[tokens[i + 1]] for i in (0, 2, 4)
            ]
        except (KeyError, ValueError) as error:
            raise ValueError(f"Invalid Criterion interval: {name}") from error
        if not lower <= value <= upper:
            raise ValueError(f"Invalid Criterion interval: {name}")
        results[name] = value
    if set(results) != names:
        raise ValueError(
            f"Incomplete benchmark suite: expected {sorted(names)}, got {sorted(results)}"
        )
    return results


def validate_report(report, require_baseline=True, names=NAMES):
    """Bencher can return success without enough history; that is not a pass."""
    if report.get("alerts"):
        raise ValueError("Bencher reported a performance regression")
    iterations = report.get("results") or []
    if len(iterations) != 1:
        raise ValueError("Expected one complete Bencher report iteration")
    seen = set()
    for result in iterations[0]:
        name = result["benchmark"]["name"]
        if name in seen:
            raise ValueError(f"Duplicate benchmark: {name}")
        seen.add(name)
        measures = [m for m in result["measures"] if m["measure"]["slug"] == "latency"]
        if len(measures) != 1:
            raise ValueError(f"Missing latency: {name}")
        measure = measures[0]
        if "metrics" in measure:
            metrics = [m for m in measure["metrics"] if m["name"] == "value"]
            if len(metrics) != 1:
                raise ValueError(f"Missing latency value: {name}")
            value = positive(metrics[0]["value"])
            boundaries = [b["boundary"] for b in metrics[0]["boundaries"]]
        else:
            value = positive(measure["metric"]["value"])
            boundaries = [measure.get("boundary")]
        if not require_baseline:
            continue
        boundaries = [
            b for b in boundaries if b and b.get("baseline") and b.get("upper_limit")
        ]
        if not boundaries:
            raise ValueError(
                f"Missing baseline for {name}; bootstrap three main-branch reports on this testbed"
            )
        for boundary in boundaries:
            baseline = positive(boundary["baseline"])
            limit = positive(boundary["upper_limit"])
            if limit > baseline * 1.200001:
                raise ValueError(
                    f"Unexpected baseline threshold for {name}: exceeds 20%"
                )
            if value > limit:
                raise ValueError(
                    f"Performance regression: {name} = {value:g} ns, limit {limit:g} ns"
                )
    if seen != names:
        raise ValueError(f"Incomplete Bencher report: {sorted(seen)}")


def testbed_name(system, arch, cpu, rust, suite="cpu"):
    suite_names(suite)
    fingerprint = hashlib.sha256(
        json.dumps([system, arch, cpu, rust]).encode()
    ).hexdigest()[:16]
    prefix = "cpu" if suite == "cpu" else "pipeline-cpu"
    return f"{prefix}-v1-{fingerprint}"


def command_output(*args, checkout=ROOT):
    return subprocess.check_output(args, cwd=checkout, text=True).strip()


def run(output, runs, suite="cpu", checkout=ROOT):
    checkout = checkout.resolve()
    names = suite_names(suite)
    output.mkdir(parents=True, exist_ok=False)
    cpu = (
        command_output("sysctl", "-n", "machdep.cpu.brand_string")
        if platform.system() == "Darwin"
        else next(
            line.split(":", 1)[1].strip()
            for line in Path("/proc/cpuinfo").read_text().splitlines()
            if line.startswith("model name")
        )
    )
    rust = command_output("rustc", "--version", checkout=checkout)
    system = (
        platform.freedesktop_os_release().get("PRETTY_NAME", platform.system())
        if platform.system() == "Linux"
        else platform.system()
    )
    metadata = dict(
        commit=command_output("git", "rev-parse", "HEAD", checkout=checkout),
        dirty=bool(command_output("git", "status", "--porcelain", checkout=checkout)),
        measurement_job={
            "run_id": os.environ.get("GITHUB_RUN_ID"),
            "attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
            "runner": os.environ.get("RUNNER_NAME"),
        },
        cpu=cpu,
        arch=platform.machine(),
        system=system,
        rust=rust,
        runner_image=os.environ.get("ImageVersion", "local"),
        profile="bench (opt-level=3, thin LTO, codegen-units=1)",
        workload_version=1,
        suite=suite,
        testbed=testbed_name(system, platform.machine(), cpu, rust, suite=suite),
        runs=runs,
        timestamp=datetime.now(timezone.utc).isoformat(),
    )
    (output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")
    # Cargo, build scripts and workloads must never inherit reporting credentials.
    env = {
        k: v
        for k, v in os.environ.items()
        if not k.startswith(("BENCHER_", "GH_", "GITHUB_TOKEN", "ACTIONS_", "FNOX_"))
    }
    command = [
        "cargo",
        "bench",
        "--locked",
        "-p",
        "dimensify_perf",
        "--bench",
        suite,
        "--color",
        "never",
    ]
    if suite == "pipeline":
        command += ["--features", "pipeline"]
    subprocess.run([*command, "--no-run"], cwd=checkout, env=env, check=True)
    for i in range(runs):
        with (output / f"criterion-{i + 1}.txt").open("w") as log:
            process = subprocess.Popen(
                [*command, "--", "--noplot"],
                cwd=checkout,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )
            for line in process.stdout:
                print(line, end="", flush=True)
                log.write(line)
            if process.wait():
                raise ValueError(f"Benchmark run {i + 1} failed; see {log.name}")
        criterion_results(Path(log.name).read_text(), names)

    metadata.update(
        commit_after=command_output("git", "rev-parse", "HEAD", checkout=checkout),
        dirty_after=bool(
            command_output("git", "status", "--porcelain", checkout=checkout)
        ),
    )
    (output / "environment.json").write_text(json.dumps(metadata, indent=2) + "\n")


def validate_pair(metadata, base, pull):
    """Accept fresh base-commit measurements, never candidate-as-baseline data."""
    if not pull or base.get("commit") != pull["base"]["sha"] or base.get("runs") != 3:
        raise ValueError("Paired baseline needs three measured PR-base runs")
    for row in (metadata, base):
        if (
            row.get("dirty") is not False
            or row.get("dirty_after") is not False
            or row.get("commit_after") != row["commit"]
        ):
            raise ValueError(
                "Paired measurements require unchanged clean source checkouts"
            )
    for key in (
        "cpu",
        "arch",
        "system",
        "rust",
        "runner_image",
        "profile",
        "workload_version",
        "suite",
        "testbed",
        "measurement_job",
    ):
        if key not in metadata or metadata[key] != base.get(key):
            raise ValueError(f"Paired baseline environment mismatch: {key}")
    job = metadata.get("measurement_job")
    if (
        not isinstance(job, dict)
        or job.get("run_id") != os.environ.get("GITHUB_RUN_ID")
        or not job.get("runner")
        or not re.fullmatch(r"[1-9][0-9]*", str(job.get("attempt")))
    ):
        raise ValueError("Paired measurements must come from the same workflow runner")
    for key in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"):
        if not re.fullmatch(r"[1-9][0-9]*", os.environ.get(key, "")):
            raise ValueError(f"Missing or invalid {key} for paired publication")


def publish_saved(args, output, runs, require_baseline, names):
    for i in range(runs):
        file = output / f"criterion-{i + 1}.txt"
        criterion_results(file.read_text(), names)
        completed = subprocess.run(
            [*args, "--file", str(file)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        (output / f"bencher-{i + 1}.json").write_text(completed.stdout)
        if completed.returncode:
            # Bencher stderr can include HTTP diagnostics; keep credentials out of logs.
            raise ValueError(
                "Bencher upload or regression check failed; inspect the Bencher dashboard"
            )
        validate_report(
            json.loads(completed.stdout), require_baseline=require_baseline, names=names
        )


def upload(output, bootstrap, suite="cpu", paired_base=None):
    names = suite_names(suite)
    for name in ("BENCHER_PROJECT", "BENCHER_API_KEY"):
        if not os.environ.get(name):
            raise ValueError(f"Missing {name}; see docs/performance.md")
    event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    pull = event.get("pull_request")
    if pull and pull["head"]["repo"]["full_name"] != os.environ["GITHUB_REPOSITORY"]:
        raise ValueError("Fork PRs are artifact-only; upload from a maintainer branch")
    if not pull and os.environ.get("GITHUB_REF") != "refs/heads/main":
        raise ValueError("Non-PR publication is only allowed on main")
    if bootstrap and (pull or os.environ.get("GITHUB_REF") != "refs/heads/main"):
        raise ValueError("Bootstrap is only allowed on main")
    metadata = json.loads((output / "environment.json").read_text())
    if metadata.get("suite", "cpu") != suite:
        raise ValueError("Saved measurements belong to a different benchmark suite")
    expected_hash = pull["head"]["sha"] if pull else os.environ["GITHUB_SHA"]
    if metadata["commit"] != expected_hash or metadata["dirty"]:
        raise ValueError("Benchmark commit does not match the clean workflow checkout")
    runs = 3 if bootstrap else 1
    if metadata["runs"] != runs:
        raise ValueError(f"Expected {runs} measured run(s)")
    base = None
    if paired_base is not None:
        if bootstrap:
            raise ValueError("Paired PR comparison is separate from main bootstrap")
        base = json.loads((paired_base / "environment.json").read_text())
        validate_pair(metadata, base, pull)
        # Validate every raw log before publishing any base or candidate result.
        for directory, count in ((paired_base, 3), (output, 1)):
            for i in range(count):
                criterion_results(
                    (directory / f"criterion-{i + 1}.txt").read_text(), names
                )
    ensure_name(
        os.environ["BENCHER_PROJECT"],
        metadata["testbed"],
        cpu_name(
            metadata["system"],
            metadata["arch"],
            metadata["cpu"],
            metadata["rust"],
            suite,
        ),
    )
    # Each suite resets its own PR start point. Sharing the PR branch would let
    # concurrent publishers replace each other's Bencher heads and thresholds.
    prefix = "" if suite == "cpu" else "pipeline-"
    branch = f"{prefix}pr-{pull['number']}" if pull else "main"
    args = [
        "bencher",
        "run",
        "--project",
        os.environ["BENCHER_PROJECT"],
        "--branch",
        branch,
        "--hash",
        expected_hash,
        "--testbed",
        metadata["testbed"],
        "--adapter",
        "rust_criterion",
        "--threshold-measure",
        "latency",
        "--threshold-test",
        "percentage",
        "--threshold-min-sample-size",
        "3",
        "--threshold-max-sample-size",
        "10",
        "--threshold-upper-boundary",
        "0.20",
        "--error-on-alert",
        "--format",
        "json",
        "--quiet",
        "--attempts",
        "3",
    ]
    start_point = "main"
    if base is not None:
        start_point = f"{prefix}base-pr-{pull['number']}-run-{os.environ['GITHUB_RUN_ID']}-{os.environ['GITHUB_RUN_ATTEMPT']}"
        base_args = args.copy()
        base_args[base_args.index("--branch") + 1] = start_point
        base_args[base_args.index("--hash") + 1] = base["commit"]
        print(
            f"Paired base {base['commit']} on {metadata['testbed']}: three measured reports -> {start_point}",
            flush=True,
        )
        publish_saved(base_args, paired_base, 3, False, names)
    if pull:
        args += [
            "--start-point",
            start_point,
            "--start-point-hash",
            pull["base"]["sha"],
            "--start-point-reset",
            "--start-point-clone-thresholds",
        ]
    if os.environ.get("GH_TOKEN"):
        ci_id = "CPU performance" if suite == "cpu" else "Pipeline CPU performance"
        args += ["--github-actions", os.environ["GH_TOKEN"], "--ci-id", ci_id]
    publish_saved(args, output, runs, not bootstrap, names)
    print(
        "Baseline seeded: run the normal workflow to verify the gate."
        if bootstrap
        else "CPU performance gate passed: all workloads have baselines and no regressions."
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["run", "upload"])
    parser.add_argument("--output", type=Path, default=ROOT / "dist/performance")
    parser.add_argument("--bootstrap", action="store_true")
    parser.add_argument("--suite", choices=("cpu", "pipeline"), default="cpu")
    parser.add_argument(
        "--checkout",
        type=Path,
        default=ROOT,
        help="Source checkout to measure (run only)",
    )
    parser.add_argument(
        "--paired-base",
        type=Path,
        help="Three saved same-runner PR-base measurements (upload only)",
    )
    args = parser.parse_args()
    if args.action == "run":
        if args.paired_base is not None:
            parser.error("--paired-base is only valid for upload")
        run(args.output, 3 if args.bootstrap else 1, args.suite, args.checkout)
    else:
        if args.checkout != ROOT:
            parser.error("--checkout is only valid for run")
        upload(args.output, args.bootstrap, args.suite, args.paired_base)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Performance check failed: {error}", file=sys.stderr)
        sys.exit(1)
