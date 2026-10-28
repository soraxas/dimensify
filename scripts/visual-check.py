#!/usr/bin/env python3
"""Always assert analytic image regions, then compare a configured reviewed lane."""
import argparse
import json
import os
from pathlib import Path
import subprocess

BASELINES = Path("tests/visual/baselines/index.json")
IDENTITY = ("lane", "target", "backend", "adapter", "driver", "browser", "viewport", "sceneHash", "settingsHash")


def select_baseline(metadata, registry):
    candidates = [entry for entry in registry["lanes"] if entry["lane"] == metadata["lane"]]
    matches = []
    for entry in candidates:
        expected = json.loads((Path(entry["directory"]) / "manifest.json").read_text())["metadata"]
        if all(key in metadata and key in expected and expected[key] == metadata[key] for key in IDENTITY):
            matches.append(entry)
    if len(matches) > 1:
        raise ValueError("Ambiguous baseline lane")
    if candidates and not matches:
        raise ValueError("Reviewed references exist for this lane but none match the complete rendering identity")
    return matches[0]["directory"] if matches else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--actual", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--require-golden", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    subprocess.run(["node", "scripts/visual-compare.cjs", "--semantic-only", "--expectations", "tests/visual/expectations.json", "--actual", str(args.actual), "--out", str(args.out / "semantic")], check=True)
    metadata = json.loads((args.actual / "captures.json").read_text())["metadata"]
    registry = json.loads(BASELINES.read_text())
    if registry.get("schema") != "dimensify-visual-baselines/1" or not isinstance(registry.get("lanes"), list):
        raise ValueError("Invalid reviewed baseline registry")
    baseline = select_baseline(metadata, registry)
    if baseline:
        subprocess.run(["node", "scripts/visual-compare.cjs", "--actual", str(args.actual), "--baseline", baseline, "--out", str(args.out / "golden")], check=True)
        verdict = {"semanticPassed": True, "goldenCompared": True, "baseline": baseline}
    else:
        verdict = {"semanticPassed": True, "goldenCompared": False, "reason": "No reviewed reference configured for this lane; semantic assertions passed. Golden comparison is pending explicit baseline review."}
        if args.require_golden:
            (args.out / "result.json").write_text(json.dumps(verdict, indent=2) + "\n")
            raise ValueError("A reviewed golden baseline is required for this lane")
    (args.out / "result.json").write_text(json.dumps(verdict, indent=2) + "\n")
    summary = f"Gaussian visual assertions: semantic PASS; golden {'PASS' if verdict['goldenCompared'] else 'NOT CONFIGURED (not compared)'}"
    print(summary)
    if os.getenv("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as output:
            output.write(summary + "\n")


if __name__ == "__main__":
    main()
