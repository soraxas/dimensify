"""Rebuild and check browser replay metrics from saved raw samples."""

import json
import statistics
import subprocess

from perf_resources import browser_metrics as browser_resources
from perf_scenes_common import ROOT, clean_env, positive, validate_bmf


def worker_profiling(report):
    flag = report["metadata"].get("workerProfiling", False)
    marker = report.get("protocol", {}).get("workerProfiling", "disabled")
    profiles = report.get("workerProfiles", [])
    if (
        type(flag) is not bool
        or not isinstance(marker, str)
        or not marker.strip()
        or not isinstance(profiles, list)
        or (marker != "disabled") != flag
        or bool(profiles) != flag
    ):
        raise ValueError("Contradictory or incomplete worker profiling cohort signals")
    return flag


def browser_results(report, bmf):
    """Check each physical round; privacy-redacted adapters remain diagnostic."""
    metadata, runs = report["metadata"], report["runs"]
    if report.get("errors") or len(runs) < 6:
        raise ValueError("Require three successful cold/warm browser rounds")
    rounds = sorted({run["round"] for run in runs})
    if any(
        sorted(run["mode"] for run in runs if run["round"] == number)
        != ["cold", "warm"]
        for number in rounds
    ):
        raise ValueError("Missing cold/warm pair")
    observed = [
        (
            run["adapter"],
            run["browser"],
            run["browserEnvironment"],
            run["final"]["settings"],
        )
        for run in runs
    ]
    if any(value != observed[0] for value in observed):
        raise ValueError("Browser environment or settings changed between rounds")
    profiling = worker_profiling(report)
    if profiling:
        from perf_profiles import metrics as profile_metrics

        if {item["round"] for item in report.get("workerProfiles", [])} != set(rounds):
            raise ValueError("Worker profiles must cover exactly the replay rounds")
        metrics = profile_metrics(report)
        validate_bmf(metrics)
        return {
            "identity": browser_identity(report, observed[0]),
            "commit": metadata["commit"],
            "metrics": metrics,
            "publication_eligible": False,
        }
    bmf = dict(bmf)
    # Lifetime CPU totals and sampled cache peaks cover the entire cold+warm
    # replay. They are neither isolated worker CPU times nor process/VRAM peaks.
    warm = [run for run in runs if run["mode"] == "warm"]
    for stage in (
        "decode",
        "snapshot_compose",
        "selection_plan",
        "relocation_plan",
        "source_upload_prepare",
        "selection_copy_prepare",
    ):
        totals = [
            (run["final"].get("cpu", {}).get("cpu_stages") or {}).get(stage, {})
            for run in warm
        ]
        if all(
            item.get("attempts", 0) > 0 and item.get("total_ms", 0) > 0
            for item in totals
        ):
            for field, label in (("total_ms", "total-wall"), ("max_ms", "max-wall")):
                bmf[f"browser-whole-replay-cpu-{stage.replace('_', '-')}-{label}"] = {
                    "latency": {
                        "value": statistics.median(
                            positive(item[field]) for item in totals
                        )
                        * 1e6
                    }
                }
            bmf[f"browser-whole-replay-cpu-{stage.replace('_', '-')}-attempts"] = {
                "count": {
                    "value": statistics.median(item["attempts"] for item in totals)
                }
            }
    for field, name, measure in (
        ("cacheReadyBytes", "cache-ready-peak", "cache-payload-bytes"),
        ("cacheReservedBytes", "cache-reserved-peak", "cache-payload-bytes"),
        ("renderReadySplats", "ready-splats-peak", "count"),
        ("pendingDownloads", "pending-downloads-peak", "count"),
        ("pendingGenerations", "pending-generations-peak", "count"),
    ):
        peaks = []
        for number in rounds:
            observations = [
                obs[field]
                for run in runs
                if run["round"] == number
                for obs in run.get("observations", [])
                if field in obs
            ]
            if observations:
                peaks.append(max(observations))
        if len(peaks) == len(rounds):
            bmf[f"browser-observed-{name}"] = {
                measure: {"value": statistics.median(peaks)}
            }
    validate_bmf(bmf)
    bmf.update(browser_resources(runs))
    validate_bmf(bmf)
    return {
        "identity": browser_identity(report, observed[0]),
        "commit": metadata["commit"],
        "metrics": bmf,
        "publication_eligible": report.get("performancePublicationEligible") is True
        and metadata["sourceState"] == "clean",
    }


def browser_identity(report, observed):
    metadata = report["metadata"]
    identity = {
        "kind": "browser",
        "protocol": report["protocol"],
        "adapter": observed[0],
        "browser": observed[1],
        "browser_environment": observed[2],
        "settings": observed[3],
        "hardware": metadata["hardware"],
        "platform": metadata["platform"],
        "arch": metadata["arch"],
    }
    if metadata.get("physicalDevice"):
        identity["physical_device"] = metadata["physicalDevice"]
        identity["runner"] = {
            "platform": metadata["platform"],
            "arch": metadata["arch"],
        }
        identity["platform"] = metadata["physicalDevice"]["os"]
        identity["arch"] = "unobserved"
    identity["protocol"] = {
        key: value
        for key, value in identity["protocol"].items()
        if key != "telemetryPublishMs"
    }
    return identity


def browser_metrics(report):
    """Use the harness's actual statistics and eligibility contracts on raw data."""
    worker_profiling(report)
    program = r"""
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
"""
    result = subprocess.run(
        ["node", "-e", program, str(ROOT / "scripts/perf-browser-results.cjs")],
        input=json.dumps(report, allow_nan=False),
        text=True,
        env=clean_env(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode:
        raise ValueError("Saved browser raw samples failed replay validation")
    return json.loads(result.stdout)
