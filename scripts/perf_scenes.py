#!/usr/bin/env python3
"""Repeat real-scene benchmarks; publish saved hardware results separately."""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import statistics
import subprocess
import sys
import threading
from urllib.parse import unquote, urlsplit

from perf import validate_report as check_bencher_report

ROOT = Path(__file__).resolve().parents[1]
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
    if not relative or part.is_absolute() or any(x in (".", "..") for x in part.parts) or "\\" in relative or ":" in relative:
        raise ValueError("Unsafe fixture path")
    path = (root / relative).resolve()
    if not path.is_relative_to(root):
        raise ValueError("Fixture path escapes root")
    return path


def fixture(path):
    """Recheck the complete prepared asset set, not just the entrypoint."""
    data = json.loads(Path(path).read_text())
    if data["schema"] != "inlier-gaussian-fixture/1" or not re.fullmatch(r"[a-z0-9-]+", data["dataset_id"]):
        raise ValueError("Unsupported fixture manifest")
    root = Path(data["root"]).resolve()
    rows = sorted(({key: row[key] for key in ("path", "bytes", "sha256")} for row in data["files"]), key=lambda row: row["path"])
    if not rows or len({row["path"] for row in rows}) != len(rows):
        raise ValueError("Empty or duplicate fixture files")
    for row in rows:
        actual = inside(root, row["path"])
        if actual.stat().st_size != row["bytes"] or digest(actual) != row["sha256"]:
            raise ValueError(f"Fixture size/hash mismatch: {row['path']}")
    identity = "sha256:" + hashlib.sha256(canonical(rows).encode()).hexdigest()
    if data["asset_identity"] != identity or data["entrypoint_relative"] not in {row["path"] for row in rows}:
        raise ValueError("Fixture set identity or entrypoint mismatch")
    data["entrypoint"] = str(inside(root, data["entrypoint_relative"]))
    return data


def positive(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
        raise ValueError("Expected finite positive timing")
    return value


def distribution(values):
    values = sorted(positive(value) for value in values)
    if not values:
        raise ValueError("Missing frame samples")
    return {"median": statistics.median(values), "p95": values[math.ceil(len(values) * .95) - 1],
            "p99": values[math.ceil(len(values) * .99) - 1]}


def validate_bmf(data):
    if not data:
        raise ValueError("Empty BMF cannot be published")
    for name, measures in data.items():
        if not name or len(measures) != 1 or not set(measures) <= {"latency", "cache-payload-bytes", "count"}:
            raise ValueError("Unsupported measure")
        measure, metric = next(iter(measures.items()))
        if set(metric) != {"value"}:
            raise ValueError("Expected a measured value, not percentile confidence bounds")
        value = metric["value"]
        if measure == "latency":
            positive(value)
        elif isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError("Expected nonnegative byte/count measurement")


def native_identity(report):
    env = report["environment"]
    return {"kind": "native", "protocol": report["protocol"], "adapter": env["adapter"],
            "os": env.get("os"), "cpu": env.get("cpu"), "architecture": env.get("architecture"),
            "rustc": env.get("build_rustc"), "render": report["render"],
            "gpu_timestamps": report["gpu_timestamps_requested"]}


def testbed(identity):
    return f"real-{identity['kind']}-v1-" + hashlib.sha256(canonical(identity).encode()).hexdigest()[:20]


def benchmark_prefix(summary):
    prefix = f"real-scene-v1/{summary['dataset_id']}/{summary['asset_identity'][7:19]}/{summary['preset']}"
    camera = summary["identity"].get("render", {}).get("camera_input")
    if camera:
        prefix += "/camera-" + camera["sha256"][:12]
    return prefix


def bencher_name(full):
    """Avoid the service's lossy long-name slug generation and collisions."""
    parts = full.split("/")
    dataset = parts[1].split("-")[0][:9]
    label = "-".join(parts[4:]).removesuffix("-v1").removeprefix("browser-")
    for before, after in (("renderer-encoded-interval", "encoded"), ("renderer-encoded-ready", "encoded-ready"),
                          ("browser-raf-interval", "raf"), ("app-interval", "app"),
                          ("static-near-side", "near0"), ("moving-near-side", "near1"),
                          ("static-main", "main0"), ("moving-main", "main1"),
                          ("gpu-radix-and-compaction", "gpu-sort"), ("completed-frame", "frame"),
                          ("app-update-wall", "update"), ("whole-replay-cpu-", "cpu-"),
                          ("observed-", ""), ("camera-", "cam-")):
        label = label.replace(before, after)
    scope = "obs1" if full.startswith("observed-") else "rs1"
    prefix = f"{scope}-{dataset}-{parts[3][0]}-"
    room = 37 - len(prefix)
    if len(label) > room:
        label = label[:room - 9] + "-" + label[-8:]
    return prefix + label + "-" + hashlib.sha256(full.encode()).hexdigest()[:10]


def aggregate(values):
    keys = set(values[0])
    if any(set(row) != keys for row in values):
        raise ValueError("Incomplete metrics across repeated runs")
    repeats = {name: [positive(row[name]) for row in values] for name in sorted(keys)}
    return {"metrics": {name: {"latency": {"value": statistics.median(samples)}} for name, samples in repeats.items()},
            "repeat_values": repeats}


def native_results(reports):
    if len(reports) < 3:
        raise ValueError("Require at least three independent native processes")
    first = reports[0]
    expected = native_identity(first)
    values = []
    for report in reports:
        env = report["environment"]
        commit = env["git_commit"]
        adapter = env["adapter"]
        if (report.get("classification") != "hardware_performance" or report.get("publication_eligible") is not True
                or env.get("git_dirty") is not False or env.get("git_dirty_after") is not False
                or env.get("build_dirty") != "false" or env.get("debug_assertions") is not False
                or env.get("build_opt_level") != "3" or env.get("build_commit") != commit
                or env.get("git_commit_after") != commit or not re.fullmatch(r"[a-f0-9]{40}", commit)
                or adapter.get("device_type") not in ("IntegratedGpu", "DiscreteGpu")
                or re.search(r"swiftshader|llvmpipe|lavapipe|software", canonical(adapter), re.I)):
            raise ValueError("Native report is not clean hardware performance evidence")
        if (native_identity(report) != expected or report["source"]["sha256"] != first["source"]["sha256"]
                or commit != first["environment"]["git_commit"]):
            raise ValueError("Incomparable native runs: source, build or environment changed")
        cases = report["cases"]
        if len(cases) != 4 or {case["name"] for case in cases} != CASES:
            raise ValueError("Missing native camera case")
        row = {"loading/read-input": positive(report["source"]["source_prepare_ns"]),
               "loading/decode-expanded": positive(report["source"]["decode_ns"]),
               "loading/renderer-setup": positive(env["renderer_setup_ns"]),
               "loading/first-completed-draw-after-spawn": positive(cases[0]["first_acknowledged_completed_draw_ns"])}
        for case in cases:
            frames = case["rows"]
            if len(frames) < 240 or any(frame["acknowledged_sort_and_draw"] is not True for frame in frames) or case["capture"]["non_background_pixels"] <= 0:
                raise ValueError("Missing real native frames or nonblank capture")
            if sum(positive(frame["completed_frame_ns"]) for frame in frames) < 2_000_000_000:
                raise ValueError("Require two seconds of completed work per native case")
            for field, label in (("completed_frame_ns", "completed-frame"), ("app_update_wall_ns", "app-update-wall")):
                for quantile, value in distribution([frame[field] for frame in frames]).items():
                    row[f"{case['name']}/{label}/{quantile}"] = value
            radix = [frame["gpu_radix_sample"]["measurement"]["duration_ms"] * 1e6 for frame in frames
                     if frame.get("gpu_radix_sample") and frame["gpu_radix_sample"].get("measurement", {}).get("duration_ms") is not None]
            if report["gpu_timestamps_requested"] and len(radix) == len(frames):
                for quantile, value in distribution(radix).items():
                    row[f"{case['name']}/gpu-radix-and-compaction/{quantile}"] = value
        values.append(row)
    return {**aggregate(values), "identity": expected, "commit": first["environment"]["git_commit"]}


def browser_results(report, bmf):
    """Check each physical round; privacy-redacted adapters remain diagnostic."""
    metadata, runs = report["metadata"], report["runs"]
    if report.get("errors") or len(runs) < 6:
        raise ValueError("Require three successful cold/warm browser rounds")
    rounds = sorted({run["round"] for run in runs})
    if any(sorted(run["mode"] for run in runs if run["round"] == number) != ["cold", "warm"] for number in rounds):
        raise ValueError("Missing cold/warm pair")
    observed = [(run["adapter"], run["browser"], run["browserEnvironment"], run["final"]["settings"]) for run in runs]
    if any(value != observed[0] for value in observed):
        raise ValueError("Browser environment or settings changed between rounds")
    bmf = dict(bmf)
    # Lifetime CPU totals and sampled cache peaks cover the entire cold+warm
    # replay. They are neither isolated worker CPU times nor process/VRAM peaks.
    warm = [run for run in runs if run["mode"] == "warm"]
    for stage in ("decode", "snapshot_compose", "selection_plan", "relocation_plan", "source_upload_prepare", "selection_copy_prepare"):
        totals = [(run["final"].get("cpu", {}).get("cpu_stages") or {}).get(stage, {}) for run in warm]
        if all(item.get("attempts", 0) > 0 and item.get("total_ms", 0) > 0 for item in totals):
            for field, label in (("total_ms", "total-wall"), ("max_ms", "max-wall")):
                bmf[f"browser-whole-replay-cpu-{stage.replace('_', '-')}-{label}"] = {"latency": {"value": statistics.median(positive(item[field]) for item in totals) * 1e6}}
            bmf[f"browser-whole-replay-cpu-{stage.replace('_', '-')}-attempts"] = {"count": {"value": statistics.median(item["attempts"] for item in totals)}}
    for field, name, measure in (("cacheReadyBytes", "cache-ready-peak", "cache-payload-bytes"),
                                 ("cacheReservedBytes", "cache-reserved-peak", "cache-payload-bytes"),
                                 ("renderReadySplats", "ready-splats-peak", "count"),
                                 ("pendingDownloads", "pending-downloads-peak", "count"),
                                 ("pendingGenerations", "pending-generations-peak", "count")):
        peaks = []
        for number in rounds:
            observations = [obs[field] for run in runs if run["round"] == number for obs in run.get("observations", []) if field in obs]
            if observations:
                peaks.append(max(observations))
        if len(peaks) == len(rounds):
            bmf[f"browser-observed-{name}"] = {measure: {"value": statistics.median(peaks)}}
    validate_bmf(bmf)
    identity = {"kind": "browser", "protocol": report["protocol"], "adapter": observed[0][0],
                "browser": observed[0][1], "browser_environment": observed[0][2], "settings": observed[0][3],
                "hardware": metadata["hardware"], "platform": metadata["platform"], "arch": metadata["arch"]}
    # The viewer URL and scenario timings are deliberately excluded from testbed identity.
    identity["protocol"] = {key: value for key, value in identity["protocol"].items() if key != "telemetryPublishMs"}
    return {"identity": identity, "commit": metadata["commit"], "metrics": bmf,
            "publication_eligible": report.get("performancePublicationEligible") is True and metadata["sourceState"] == "clean"}


def browser_metrics(report):
    """Use the harness's actual statistics and eligibility contracts on raw data."""
    program = r'''
const fs = require("node:fs");
const {metrics, validateMetadata, validateRun, performancePublicationEligible} = require(process.argv[1]);
const report = JSON.parse(fs.readFileSync(0, "utf8"));
validateMetadata(report.metadata);
let eligible = performancePublicationEligible(report.metadata);
const expected = (report.protocol.steps || []).map(step => step.name).sort();
for (const run of report.runs) {
  validateRun(run);
  eligible = eligible && performancePublicationEligible({...report.metadata, adapter:run.adapter, browser:run.browser});
  const names = (run.cases || []).map(item => item.name).sort();
  if (JSON.stringify(names) !== JSON.stringify(expected)) throw Error("Missing browser camera case");
}
console.log(JSON.stringify({metrics:metrics(report.runs), publication_eligible:eligible}));
'''
    result = subprocess.run(["node", "-e", program, str(ROOT / "scripts/perf-browser-results.cjs")],
                            input=json.dumps(report, allow_nan=False), text=True, env=clean_env(),
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise ValueError("Saved browser raw samples failed replay validation")
    return json.loads(result.stdout)


def validate_report(report, names, check):
    # Byte/count series are histories; regression thresholds apply to latency.
    expected = names if isinstance(names, dict) else {name: {"latency": {}} for name in names}
    if report.get("alerts") or len(report.get("results", [])) != 1:
        raise ValueError("Bencher reported alerts or an incomplete iteration")
    results = report["results"][0]
    actual = [result["benchmark"]["name"] for result in results]
    if set(actual) != set(expected) or len(actual) != len(expected):
        raise ValueError("Incomplete or unexpected Bencher metrics")
    latency_results = []
    for result in results:
        name = result["benchmark"]["name"]
        expected_measure = next(iter(expected[name]))
        measures = result["measures"]
        if len(measures) != 1 or measures[0]["measure"]["slug"] != expected_measure:
            raise ValueError("Bencher returned a different measure")
        item = measures[0]
        value = item["metric"]["value"] if "metric" in item else next(m["value"] for m in item["metrics"] if m["name"] == "value")
        if "value" in expected[name][expected_measure] and not math.isclose(value, expected[name][expected_measure]["value"], rel_tol=1e-12, abs_tol=1e-9):
            raise ValueError("Bencher returned a different measurement value")
        if expected_measure == "latency":
            latency_results.append(result)
        else:
            item = measures[0]
            value = item["metric"]["value"] if "metric" in item else next(m["value"] for m in item["metrics"] if m["name"] == "value")
            validate_bmf({name: {expected_measure: {"value": value}}})
    if latency_results:
        check_bencher_report({"results": [latency_results], "alerts": []}, require_baseline=check,
                             names={result["benchmark"]["name"] for result in latency_results})


def clean_env():
    return {key: value for key, value in os.environ.items()
            if not key.startswith(("BENCHER_", "GH_", "GITHUB_TOKEN", "ACTIONS_", "FNOX_"))}


def execute(command, log, *, cwd=ROOT):
    with Path(log).open("w") as stream:
        result = subprocess.run(command, cwd=cwd, env=clean_env(), stdout=stream, stderr=subprocess.STDOUT)
    if result.returncode:
        raise ValueError(f"Measurement failed; see {log}")


@contextmanager
def serve(site, assets):
    class Handler(SimpleHTTPRequestHandler):
        def translate_path(self, path):
            path = unquote(urlsplit(path).path)
            if path.startswith("/fixtures/"):
                return str(inside(assets, path.removeprefix("/fixtures/")))
            return super().translate_path(path)

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(Handler, directory=str(site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def build_web(checkout, output):
    checkout = checkout.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=checkout, text=True).strip())
    subprocess.run(["cargo", "build", "--locked", "--release", "-p", "dimensify", "--features", "splat-viewer-perf", "--example", "splat_viewer", "--target", "wasm32-unknown-unknown"], cwd=checkout, env=clean_env(), check=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=checkout, env=clean_env()))
    wasm = Path(metadata["target_directory"]) / "wasm32-unknown-unknown/release/examples/splat_viewer.wasm"
    subprocess.run(["wasm-bindgen", "--target", "web", "--out-dir", str(output / "pkg"), str(wasm)], env=clean_env(), check=True)
    subprocess.run(["cargo", "build", "--locked", "-p", "sog_codec", "--features", "worker", "--release", "--target", "wasm32-unknown-unknown"], cwd=checkout, env=clean_env(), check=True)
    codec = Path(metadata["target_directory"]) / "wasm32-unknown-unknown/release/sog_codec.wasm"
    subprocess.run(["wasm-bindgen", "--target", "web", "--out-dir", str(output / "sog-codec"), str(codec)], env=clean_env(), check=True)
    shutil.copy2(checkout / "crates/splat/src/streaming/runtime/sog-decode-worker.mjs", output / "sog-decode-worker.mjs")
    shutil.copy2(checkout / "splat-web/index.html", output / "index.html")
    after = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip()
    dirty |= bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=checkout, text=True).strip()) or before != after
    write(output / "build.json", {"commit": before, "sourceState": "modified" if dirty else "clean", "profile": "release",
                                 "feature": "splat-viewer-perf", "wasm_sha256": digest(output / "pkg/splat_viewer_bg.wasm"),
                                 "files": {str(path.relative_to(output)): digest(path) for path in output.rglob("*") if path.is_file()}})
    verify_site(output)


def verify_site(site):
    build = json.loads((site / "build.json").read_text())
    required = {"index.html", "pkg/splat_viewer.js", "pkg/splat_viewer_bg.wasm", "sog-decode-worker.mjs",
                "sog-codec/sog_codec.js", "sog-codec/sog_codec_bg.wasm"}
    if build.get("feature") != "splat-viewer-perf" or not required <= set(build["files"]):
        raise ValueError("Browser package incomplete: require viewer and SOG worker/codec artifacts")
    if any(digest(inside(site, name)) != sha for name, sha in build["files"].items()):
        raise ValueError("Served build bytes differ from recorded build")
    if digest(site / "pkg/splat_viewer_bg.wasm") != build["wasm_sha256"]:
        raise ValueError("Served WASM identity mismatch")
    return build


def run(args):
    if args.kind == "native" and args.preset == "constrained":
        raise ValueError("The constrained preset applies only to browser streaming")
    if args.kind == "browser" and args.gpu_timestamps:
        raise ValueError("Browser replay does not measure GPU timestamps")
    if args.kind == "browser" and args.camera:
        raise ValueError("Explicit camera JSON applies only to the native runner")
    data = fixture(args.fixture)
    if not 3 <= args.rounds <= 20:
        raise ValueError("Use 3..20 independent rounds")
    if args.kind == "native" and (data.get("format") != "sog-v2-bundle" or not args.runner):
        raise ValueError("Native runner requires a bundled SOG fixture and --runner")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    write(output / "fixture.json", data)
    try:
        if args.kind == "native":
            reports = []
            for number in range(1, args.rounds + 1):
                print(f"Native {data['dataset_id']} round {number}/{args.rounds}", flush=True)
                target = output / f"round-{number}"
                command = [str(args.runner.resolve()), data["entrypoint"], "--output", str(target)]
                if args.gpu_timestamps:
                    command += ["--gpu-timestamps"]
                if args.camera:
                    command += ["--camera", str(args.camera.resolve())]
                execute(command, output / f"round-{number}.log")
                reports.append(json.loads((target / "raw.json").read_text()))
            result = native_results(reports)
            if any(report["source"]["sha256"] != data["files"][0]["sha256"] for report in reports):
                raise ValueError("Runner used a different fixture")
            result["publication_eligible"] = True
        else:
            if not args.site:
                raise ValueError("Browser run requires --site from web-build")
            site = args.site.resolve()
            build = verify_site(site)
            target = output / "browser"
            with serve(site, Path(data["root"]).resolve()) as url:
                command = ["node", str(ROOT / "scripts/perf-browser.cjs"), "--url", url + "/index.html", "--out", str(target),
                           "--commit", build["commit"], "--source-state", build["sourceState"], "--build-id", "sha256:" + build["wasm_sha256"],
                           "--hardware", platform.node() + "/" + platform.platform(), "--rounds", str(args.rounds)]
                if data.get("format") == "sog-v2-bundle":
                    command += ["--file", data["entrypoint"]]
                else:
                    command += ["--asset-url", url + "/fixtures/" + data["entrypoint_relative"], "--asset-id", data["asset_identity"]]
                if args.preset == "constrained":
                    command += ["--splat-budget", "150000", "--cache-mib", "32", "--source-mib", "32", "--concurrency", "1"]
                execute(command, output / "browser.log")
            report = json.loads((target / "raw.json").read_text())
            rebuilt = browser_metrics(report)
            report["performancePublicationEligible"] = rebuilt["publication_eligible"]
            result = browser_results(report, rebuilt["metrics"])
        # Verify no source changed during the measurements.
        fixture(output / "fixture.json")
        bind_fixture({"dataset_id": data["dataset_id"], "asset_identity": data["asset_identity"], "kind": args.kind},
                     data, reports if args.kind == "native" else [report])
        prefix = benchmark_prefix({"dataset_id": data["dataset_id"], "asset_identity": data["asset_identity"],
                                   "preset": args.preset, "identity": result["identity"]})
        result["metrics"] = {f"{prefix}/{name}": value for name, value in result["metrics"].items()}
        validate_bmf(result["metrics"])
        metrics = result.pop("metrics")
        result.update(schema="dimensify-real-scene/1", kind=args.kind, preset=args.preset, rounds=args.rounds,
                      asset_identity=data["asset_identity"], dataset_id=data["dataset_id"], testbed=testbed(result["identity"]),
                      measured_at=datetime.now(timezone.utc).isoformat(),
                      scope="native serialized GPU completion; browser cadence/readiness only; no display FPS, process RSS or total VRAM")
        write(output / "summary.json", result)
        write(output / "bmf.json", metrics if result["publication_eligible"] else {})
        write(output / "diagnostic-metrics.json", metrics)
        print(json.dumps({"output": str(output), "metrics": len(metrics), "publication_eligible": result["publication_eligible"], "testbed": result["testbed"]}))
    except Exception as error:
        write(output / "failure.json", {"error": str(error)})
        raise


def upload(args):
    if not os.environ.get("BENCHER_API_KEY"):
        raise ValueError("Expose BENCHER_API_KEY through your secret manager; never put it in arguments")
    output = args.output.resolve()
    summary = json.loads((output / "summary.json").read_text())
    diagnostic = args.diagnostic_history
    if summary.get("schema") != "dimensify-real-scene/1" or (not diagnostic and summary.get("publication_eligible") is not True) or summary.get("rounds", 0) < 3:
        raise ValueError("Diagnostic/incomplete measurements cannot be published")
    data = json.loads((output / ("diagnostic-metrics.json" if diagnostic else "bmf.json")).read_text())
    validate_bmf(data)
    verify_saved(output, summary, data, diagnostic=diagnostic)
    saved_fixture = fixture(output / "fixture.json")
    reports = ([json.loads((output / f"round-{number}/raw.json").read_text()) for number in range(1, summary["rounds"] + 1)]
               if summary["kind"] == "native" else [json.loads((output / "browser/raw.json").read_text())])
    bind_fixture(summary, saved_fixture, reports)
    expected_prefix = benchmark_prefix(summary) + "/"
    if any(not name.startswith(expected_prefix) for name in data) or summary["testbed"] != testbed(summary["identity"]):
        raise ValueError("Results do not match their saved fixture/testbed")
    if not re.fullmatch(r"[a-f0-9]{40}", summary["commit"]):
        raise ValueError("Invalid measured commit")
    branch, testbed_name, bmf_file = args.branch, summary["testbed"], output / "bmf.json"
    if diagnostic:
        target = diagnostic_publication(summary, data, check=args.check)
        branch, testbed_name, data = target["branch"], target["testbed"], target["metrics"]
        bmf_file = output / "diagnostic-bmf.json"
    mapping = {bencher_name(name): name for name in data}
    if len(mapping) != len(data):
        raise ValueError("Compact benchmark name collision")
    data = {short: data[full] for short, full in mapping.items()}
    bmf_file = output / ("diagnostic-bencher-bmf.json" if diagnostic else "bencher-bmf.json")
    write(bmf_file, data)
    write(output / "benchmark-map.json", mapping)
    args_list = ["bencher", "run", "--project", args.project, "--branch", branch,
                 "--hash", summary["commit"], "--testbed", testbed_name, "--adapter", "json",
                 "--file", str(bmf_file), "--format", "json", "--quiet", "--attempts", "3", "--error-on-alert"]
    # Tracking is intentionally separate from the existing required CPU gate.
    # --check requires existing boundaries; initial history never claims a passed gate.
    if args.check:
        args_list += ["--threshold-measure", "latency", "--threshold-test", "percentage", "--threshold-upper-boundary", "0.20",
                      "--threshold-min-sample-size", "3", "--threshold-max-sample-size", "10"]
    ensure_measures(args.project, {next(iter(measures)) for measures in data.values()} - {"latency"})
    result = subprocess.run(args_list, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise ValueError("Bencher upload failed: " + service_error(result.stderr))
    report = json.loads(result.stdout)
    write(output / ("bencher-diagnostic-report.json" if diagnostic else "bencher-report.json"), report)
    validate_report(report, data, check=args.check)
    write(output / "publication.json", {"kind": "diagnostic-history" if diagnostic else "hardware-history",
                                       "branch": branch, "testbed": testbed_name, "report": report.get("uuid"),
                                       "regression_check": args.check})
    print("Bencher report saved; " + ("regression comparison passed" if args.check else "history recorded, no regression-gate claim"))


def verify_saved(output, summary, metrics, diagnostic=False):
    """Recompute from raw samples so stale/edited BMF cannot slip into history."""
    if summary["kind"] == "native":
        result = native_results([json.loads((output / f"round-{number}/raw.json").read_text()) for number in range(1, summary["rounds"] + 1)])
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
    if (rebuilt != metrics or result["identity"] != summary["identity"] or result["commit"] != summary["commit"]):
        raise ValueError("Saved metrics do not match raw samples or measurement identity")


def diagnostic_publication(summary, metrics, check):
    if summary["kind"] != "browser" or check:
        raise ValueError("Diagnostic browser history cannot act as a hardware regression gate")
    return {"branch": "browser-diagnostics", "testbed": summary["testbed"].replace("real-browser-v1-", "observed-browser-v1-", 1),
            "metrics": {name.replace("real-scene-v1/", "observed-real-scene-v1/", 1): value for name, value in metrics.items()}}


def validate_diagnostic(report):
    metadata = report["metadata"]
    if metadata["sourceState"] != "clean" or metadata["profile"] != "release":
        raise ValueError("Modified browser builds cannot enter diagnostic history")
    for adapter in [metadata["adapter"], *(run["adapter"] for run in report["runs"])]:
        if (adapter.get("backend") != "BrowserWebGpu"
                or re.search(r"swiftshader|llvmpipe|lavapipe|software|microsoft basic render", canonical(adapter), re.I)
                or str(adapter.get("device_type")).lower() == "cpu"):
            raise ValueError("Browser diagnostic history excludes known software adapters")


def service_error(stderr):
    for key, value in os.environ.items():
        if any(part in key for part in ("KEY", "TOKEN", "PASSWORD", "SECRET")) and len(value) > 5:
            stderr = stderr.replace(value, "[REDACTED]")
    stderr = re.sub(r"bencher_(?:user|run)_[A-Za-z0-9_-]+", "[REDACTED]", stderr)
    stderr = re.sub(r"(?i)Bearer\s+\S+", "Bearer [REDACTED]", stderr)
    return stderr[-2000:]


def bind_fixture(summary, data, reports):
    if summary["dataset_id"] != data["dataset_id"] or summary["asset_identity"] != data["asset_identity"]:
        raise ValueError("Saved fixture does not match measurement identity")
    if summary["kind"] == "native":
        if len(data["files"]) != 1 or any(report["source"]["sha256"] != data["files"][0]["sha256"] for report in reports):
            raise ValueError("Native measurements do not match the verified fixture")
    else:
        identity = "sha256:" + data["files"][0]["sha256"] if data.get("format") == "sog-v2-bundle" else data["asset_identity"]
        if any(report["metadata"]["asset"]["identity"] != identity for report in reports):
            raise ValueError("Browser measurements do not match the verified fixture")


def ensure_measures(project, names):
    if not names:
        return
    existing = subprocess.run(["bencher", "measure", "list", project, "--per-page", "100"],
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if existing.returncode:
        raise ValueError("Cannot inspect Bencher custom measures")
    found = {measure["slug"]: measure for measure in json.loads(existing.stdout)}
    units = {"cache-payload-bytes": "bytes", "count": "count"}
    for name in sorted(names):
        if name in found:
            if found[name]["units"] != units[name]:
                raise ValueError(f"Unexpected units for Bencher measure {name}")
            continue
        result = subprocess.run(["bencher", "measure", "create", project, "--name", name, "--slug", name,
                                 "--units", units[name], "--attempts", "3"],
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if result.returncode:
            raise ValueError(f"Cannot create Bencher measure {name}; configure it with units {units[name]}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    build = actions.add_parser("web-build")
    build.add_argument("--checkout", type=Path, default=ROOT)
    build.add_argument("--output", type=Path, required=True)
    measure = actions.add_parser("run")
    measure.add_argument("--kind", choices=("native", "browser"), required=True)
    measure.add_argument("--fixture", type=Path, required=True)
    measure.add_argument("--output", type=Path, required=True)
    measure.add_argument("--runner", type=Path)
    measure.add_argument("--site", type=Path)
    measure.add_argument("--rounds", type=int, default=3)
    measure.add_argument("--preset", choices=("default", "constrained"), default="default")
    measure.add_argument("--gpu-timestamps", action="store_true")
    measure.add_argument("--camera", type=Path, help="Native source-coordinate camera JSON; versioned in metric identity")
    publish = actions.add_parser("upload")
    publish.add_argument("--output", type=Path, required=True)
    publish.add_argument("--project", default=os.environ.get("BENCHER_PROJECT", "dimensify"))
    publish.add_argument("--branch", default="main")
    publish.add_argument("--check", action="store_true")
    publish.add_argument("--diagnostic-history", action="store_true", help="Separate browser observation history; never a hardware regression gate")
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
