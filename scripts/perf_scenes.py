#!/usr/bin/env python3
"""Repeat real-scene benchmarks; publish saved hardware results separately."""

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys

from perf_scenes_bencher import (
    bencher_name,
    benchmark_prefix,
    diagnostic_publication,
    ensure_measures,
    service_error,
    testbed,
    validate_diagnostic,
    validate_report,
)
from perf_scenes_browser import (
    browser_identity,
    browser_metrics,
    browser_results,
    worker_profiling,
)
from perf_scenes_common import (
    CASES,
    CUSTOM_MEASURES,
    ROOT,
    canonical,
    clean_env,
    digest,
    distribution,
    inside,
    positive,
    validate_bmf,
    write,
)
from perf_scenes_fixtures import bind_fixture, fixture, hierarchy_dependencies
from perf_scenes_native import (
    aggregate,
    native_identity,
    native_lod_results,
    native_results,
)
from perf_scenes_web import build_web, serve, verify_site
from perf_testbeds import ensure_name

# Entry point and run/upload orchestration; the names below remain importable
# from here for callers and tests that predate the split.
__all__ = [
    "CASES",
    "CUSTOM_MEASURES",
    "ROOT",
    "aggregate",
    "bencher_name",
    "benchmark_prefix",
    "bind_fixture",
    "browser_identity",
    "browser_metrics",
    "browser_results",
    "build_web",
    "canonical",
    "clean_env",
    "diagnostic_publication",
    "digest",
    "distribution",
    "ensure_measures",
    "execute",
    "fixture",
    "hierarchy_dependencies",
    "inside",
    "main",
    "native_identity",
    "native_lod_results",
    "native_results",
    "positive",
    "run",
    "serve",
    "service_error",
    "testbed",
    "upload",
    "validate_bmf",
    "validate_diagnostic",
    "validate_report",
    "verify_saved",
    "verify_site",
    "worker_profiling",
    "write",
]


def execute(command, log, *, cwd=ROOT):
    with Path(log).open("w") as stream:
        result = subprocess.run(
            command, cwd=cwd, env=clean_env(), stdout=stream, stderr=subprocess.STDOUT
        )
    if result.returncode:
        raise ValueError(f"Measurement failed; see {log}")


def run(args):
    native = args.kind in ("native", "native-lod")
    if (
        getattr(args, "transparency", "sorted") != "sorted"
        and args.kind != "native-lod"
    ):
        raise ValueError("The transparency experiment currently requires native-lod")
    if args.kind == "native" and args.preset == "constrained":
        raise ValueError(
            "The constrained preset applies to streaming runners, not flat native"
        )
    if args.kind == "browser" and getattr(args, "encoded_compositing", False):
        raise ValueError("Encoded compositing option applies to the native runner")
    if args.kind == "browser" and args.camera:
        raise ValueError("Explicit camera JSON applies only to the native runner")
    if native and getattr(args, "worker_profile", False):
        raise ValueError("Worker profiling applies only to browser diagnostic runs")
    if args.kind == "native-lod" and getattr(args, "encoded_compositing", False):
        raise ValueError(
            "Native LOD always uses encoded compositing; omit the flat-runner option"
        )
    data = fixture(args.fixture)
    if not 3 <= args.rounds <= 20:
        raise ValueError("Use 3..20 independent rounds")
    if args.kind == "native" and (
        data.get("format") != "sog-v2-bundle" or not args.runner
    ):
        raise ValueError("Native runner requires a bundled SOG fixture and --runner")
    if args.kind == "native-lod" and (
        data.get("format") != "sog-v2-hierarchy" or not args.runner
    ):
        raise ValueError("Native LOD runner requires a hierarchy fixture and --runner")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    write(output / "fixture.json", data)
    try:
        if native:
            reports = []
            report_paths = []
            for number in range(1, args.rounds + 1):
                print(
                    f"Native {data['dataset_id']} round {number}/{args.rounds}",
                    flush=True,
                )
                target = output / f"round-{number}"
                source_args = (
                    ["--manifest", data["entrypoint"], "--preset", args.preset]
                    if args.kind == "native-lod"
                    else [data["entrypoint"]]
                )
                if (
                    args.kind == "native-lod"
                    and getattr(args, "transparency", "sorted") != "sorted"
                ):
                    source_args += ["--transparency", args.transparency]
                command = [
                    str(args.runner.resolve()),
                    *source_args,
                    "--output",
                    str(target),
                ]
                if args.gpu_timestamps:
                    command += ["--gpu-timestamps"]
                if getattr(args, "encoded_compositing", False):
                    command += ["--encoded-compositing"]
                if args.camera:
                    command += ["--camera", str(args.camera.resolve())]
                usage_path = output / f"round-{number}-process-usage.json"
                execute(
                    [
                        sys.executable,
                        str(ROOT / "scripts/perf_resources.py"),
                        "--output",
                        str(usage_path),
                        "--",
                        *command,
                    ],
                    output / f"round-{number}.log",
                )
                report = json.loads((target / "raw.json").read_text())
                report["process_usage"] = json.loads(usage_path.read_text())
                write(target / "raw.json", report)
                reports.append(report)
                report_paths.append(target / "raw.json")
            result = (
                native_lod_results(reports, report_paths)
                if args.kind == "native-lod"
                else native_results(reports, report_paths)
            )
            source_hash = next(
                row["sha256"]
                for row in data["files"]
                if row["path"] == data["entrypoint_relative"]
            )
            if any(report["source"]["sha256"] != source_hash for report in reports):
                raise ValueError("Runner used a different fixture")
            result["publication_eligible"] = True
        else:
            if not args.site:
                raise ValueError("Browser run requires --site from web-build")
            site = args.site.resolve()
            build = verify_site(site)
            target = output / "browser"
            with serve(site, Path(data["root"]).resolve()) as url:
                command = [
                    "node",
                    str(ROOT / "scripts/perf-browser.cjs"),
                    "--url",
                    url + "/index.html",
                    "--out",
                    str(target),
                    "--commit",
                    build["commit"],
                    "--source-state",
                    build["sourceState"],
                    "--build-id",
                    "sha256:" + build["wasm_sha256"],
                    "--hardware",
                    platform.node() + "/" + platform.platform(),
                    "--rounds",
                    str(args.rounds),
                ]
                if data.get("format") == "sog-v2-bundle":
                    command += ["--file", data["entrypoint"]]
                else:
                    command += [
                        "--asset-url",
                        url + "/fixtures/" + data["entrypoint_relative"],
                        "--asset-id",
                        data["asset_identity"],
                    ]
                if args.preset == "constrained":
                    command += [
                        "--splat-budget",
                        "150000",
                        "--cache-mib",
                        "32",
                        "--source-mib",
                        "32",
                        "--concurrency",
                        "1",
                    ]
                if getattr(args, "worker_profile", False):
                    command += ["--worker-profile"]
                execute(command, output / "browser.log")
            report = json.loads((target / "raw.json").read_text())
            rebuilt = browser_metrics(report)
            report["performancePublicationEligible"] = rebuilt["publication_eligible"]
            result = browser_results(report, rebuilt["metrics"])
        # Verify no source changed during the measurements.
        fixture(output / "fixture.json")
        bind_fixture(
            {
                "dataset_id": data["dataset_id"],
                "asset_identity": data["asset_identity"],
                "kind": args.kind,
            },
            data,
            reports if native else [report],
        )
        prefix = benchmark_prefix(
            {
                "schema": "dimensify-real-scene/2",
                "dataset_id": data["dataset_id"],
                "asset_identity": data["asset_identity"],
                "preset": args.preset,
                "identity": result["identity"],
            }
        )
        result["metrics"] = {
            f"{prefix}/{name}": value for name, value in result["metrics"].items()
        }
        validate_bmf(result["metrics"])
        metrics = result.pop("metrics")
        result.update(
            schema="dimensify-real-scene/2",
            kind=args.kind,
            preset=args.preset,
            rounds=args.rounds,
            asset_identity=data["asset_identity"],
            dataset_id=data["dataset_id"],
            testbed=testbed(result["identity"]),
            measured_at=datetime.now(timezone.utc).isoformat(),
            scope="native serialized GPU completion and scoped process/buffer measurements; browser cadence/readiness and sampled heap/capacity; no display FPS or total VRAM",
        )
        write(output / "summary.json", result)
        write(output / "bmf.json", metrics if result["publication_eligible"] else {})
        write(output / "diagnostic-metrics.json", metrics)
        print(
            json.dumps(
                {
                    "output": str(output),
                    "metrics": len(metrics),
                    "publication_eligible": result["publication_eligible"],
                    "testbed": result["testbed"],
                }
            )
        )
    except Exception as error:
        write(output / "failure.json", {"error": str(error)})
        raise


def upload(args):
    if not os.environ.get("BENCHER_API_KEY"):
        raise ValueError(
            "Expose BENCHER_API_KEY through your secret manager; never put it in arguments"
        )
    output = args.output.resolve()
    summary = json.loads((output / "summary.json").read_text())
    diagnostic = args.diagnostic_history
    if (
        summary.get("schema")
        not in ("dimensify-real-scene/1", "dimensify-real-scene/2")
        or (not diagnostic and summary.get("publication_eligible") is not True)
        or summary.get("rounds", 0) < 3
    ):
        raise ValueError("Diagnostic/incomplete measurements cannot be published")
    data = json.loads(
        (output / ("diagnostic-metrics.json" if diagnostic else "bmf.json")).read_text()
    )
    validate_bmf(data)
    verify_saved(output, summary, data, diagnostic=diagnostic)
    saved_fixture = fixture(output / "fixture.json")
    reports = (
        [
            json.loads((output / f"round-{number}/raw.json").read_text())
            for number in range(1, summary["rounds"] + 1)
        ]
        if summary["kind"] in ("native", "native-lod")
        else [json.loads((output / "browser/raw.json").read_text())]
    )
    bind_fixture(summary, saved_fixture, reports)
    expected_prefix = benchmark_prefix(summary) + "/"
    if any(not name.startswith(expected_prefix) for name in data) or summary[
        "testbed"
    ] != testbed(
        summary["identity"], legacy=summary["schema"] == "dimensify-real-scene/1"
    ):
        raise ValueError("Results do not match their saved fixture/testbed")
    if not re.fullmatch(r"[a-f0-9]{40}", summary["commit"]):
        raise ValueError("Invalid measured commit")
    branch, testbed_name, bmf_file = (
        args.branch,
        summary["testbed"],
        output / "bmf.json",
    )
    if diagnostic:
        target = diagnostic_publication(summary, data, check=args.check)
        branch, testbed_name, data = (
            target["branch"],
            target["testbed"],
            target["metrics"],
        )
        bmf_file = output / "diagnostic-bmf.json"
    mapping = {bencher_name(name): name for name in data}
    if len(mapping) != len(data):
        raise ValueError("Compact benchmark name collision")
    data = {short: data[full] for short, full in mapping.items()}
    bmf_file = output / (
        "diagnostic-bencher-bmf.json" if diagnostic else "bencher-bmf.json"
    )
    write(bmf_file, data)
    write(output / "benchmark-map.json", mapping)
    args_list = [
        "bencher",
        "run",
        "--project",
        args.project,
        "--branch",
        branch,
        "--hash",
        summary["commit"],
        "--testbed",
        testbed_name,
        "--adapter",
        "json",
        "--file",
        str(bmf_file),
        "--format",
        "json",
        "--quiet",
        "--attempts",
        "3",
        "--error-on-alert",
    ]
    # Tracking is intentionally separate from the existing required CPU gate.
    # --check requires existing boundaries; initial history never claims a passed gate.
    if args.check:
        args_list += [
            "--threshold-measure",
            "latency",
            "--threshold-test",
            "percentage",
            "--threshold-upper-boundary",
            "0.20",
            "--threshold-min-sample-size",
            "3",
            "--threshold-max-sample-size",
            "10",
        ]
    ensure_measures(
        args.project, {next(iter(measures)) for measures in data.values()} - {"latency"}
    )
    # Existing v1 histories retain their curated readable labels on re-upload.
    if summary["schema"] == "dimensify-real-scene/2":
        ensure_name(args.project, testbed_name, testbed_name)
    result = subprocess.run(
        args_list, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    if result.returncode:
        raise ValueError("Bencher upload failed: " + service_error(result.stderr))
    report = json.loads(result.stdout)
    write(
        output
        / ("bencher-diagnostic-report.json" if diagnostic else "bencher-report.json"),
        report,
    )
    validate_report(report, data, check=args.check)
    write(
        output / "publication.json",
        {
            "kind": "diagnostic-history" if diagnostic else "hardware-history",
            "branch": branch,
            "testbed": testbed_name,
            "report": report.get("uuid"),
            "regression_check": args.check,
        },
    )
    print(
        "Bencher report saved; "
        + (
            "regression comparison passed"
            if args.check
            else "history recorded, no regression-gate claim"
        )
    )


def verify_saved(output, summary, metrics, diagnostic=False):
    """Recompute from raw samples so stale/edited BMF cannot slip into history."""
    if summary["kind"] in ("native", "native-lod"):
        paths = [
            output / f"round-{number}/raw.json"
            for number in range(1, summary["rounds"] + 1)
        ]
        reports = [json.loads(path.read_text()) for path in paths]
        result = (
            native_lod_results(reports, paths)
            if summary["kind"] == "native-lod"
            else native_results(reports, paths)
        )
    else:
        report = json.loads((output / "browser/raw.json").read_text())
        rebuilt = browser_metrics(report)
        report["performancePublicationEligible"] = rebuilt["publication_eligible"]
        result = browser_results(report, rebuilt["metrics"])
        if diagnostic:
            validate_diagnostic(report)
        elif not result["publication_eligible"]:
            raise ValueError("Saved browser evidence is diagnostic")
    prefix = benchmark_prefix(summary)
    rebuilt = {f"{prefix}/{name}": metric for name, metric in result["metrics"].items()}
    if (
        rebuilt != metrics
        or result["identity"] != summary["identity"]
        or result["commit"] != summary["commit"]
    ):
        raise ValueError(
            "Saved metrics do not match raw samples or measurement identity"
        )
    if any(
        result.get(key) != summary.get(key)
        for key in ("repeat_values", "repeat_measures")
    ):
        raise ValueError("Saved repeat evidence does not match raw samples or units")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    build = actions.add_parser("web-build")
    build.add_argument("--checkout", type=Path, default=ROOT)
    build.add_argument("--output", type=Path, required=True)
    measure = actions.add_parser("run")
    measure.add_argument(
        "--kind", choices=("native", "native-lod", "browser"), required=True
    )
    measure.add_argument("--fixture", type=Path, required=True)
    measure.add_argument("--output", type=Path, required=True)
    measure.add_argument("--runner", type=Path)
    measure.add_argument("--site", type=Path)
    measure.add_argument("--rounds", type=int, default=3)
    measure.add_argument(
        "--transparency",
        choices=("sorted", "stochastic"),
        default="sorted",
        help="Native LOD transparency; stochastic has a separate protocol/history",
    )
    measure.add_argument(
        "--preset", choices=("default", "constrained"), default="default"
    )
    measure.add_argument("--gpu-timestamps", action="store_true")
    measure.add_argument(
        "--worker-profile",
        action="store_true",
        help="Browser sampled worker profiling; separate diagnostic cohort",
    )
    measure.add_argument(
        "--encoded-compositing",
        action="store_true",
        help="Native HDR encoded Gaussian compositing; distinct workload",
    )
    measure.add_argument(
        "--camera",
        type=Path,
        help="Native source-coordinate camera JSON; versioned in metric identity",
    )
    publish = actions.add_parser("upload")
    publish.add_argument("--output", type=Path, required=True)
    publish.add_argument(
        "--project", default=os.environ.get("BENCHER_PROJECT", "dimensify")
    )
    publish.add_argument("--branch", default="main")
    publish.add_argument("--check", action="store_true")
    publish.add_argument(
        "--diagnostic-history",
        action="store_true",
        help="Separate browser observation history; never a hardware regression gate",
    )
    args = parser.parse_args()
    if args.action == "web-build":
        build_web(args.checkout, args.output.resolve())
    elif args.action == "run":
        run(args)
    else:
        upload(args)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Real-scene benchmark failed: {error}", file=sys.stderr)
        sys.exit(1)
