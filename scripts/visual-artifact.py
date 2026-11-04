#!/usr/bin/env python3
"""Download a source-run visual artifact without executing its contents."""

import argparse
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import zipfile

CASES = {"anisotropic", "overlap-front", "overlap-reverse", "sh-front", "sh-reverse"}
MAX_ARCHIVE = 32 * 1024 * 1024


def validate_run(run, repository, run_id, commit):
    if (
        run.get("id") != run_id
        or run.get("repository", {}).get("full_name") != repository
        or run.get("path") != ".github/workflows/visual-correctness.yml"
        or run.get("head_sha") != commit
        or not re.fullmatch(r"[0-9a-f]{40}", commit)
        or run.get("event")
        not in {"pull_request", "push", "workflow_dispatch", "schedule"}
        or run.get("status") != "completed"
    ):
        raise ValueError(
            "Source workflow/repository/run/commit identity does not match"
        )

def plan_reports(run, listing, repository, run_id):
    """Select actual lanes; a failed source never invents downloadable captures."""
    validate_run(run, repository, run_id, run.get("head_sha", ""))
    if run.get("conclusion") not in {
        "success", "failure", "cancelled", "timed_out", "action_required",
        "neutral", "skipped", "stale", "startup_failure",
    }:
        raise ValueError("Invalid completed source conclusion")
    entries = listing.get("artifacts")
    if (
        not isinstance(entries, list)
        or listing.get("total_count") != len(entries)
        or len(entries) > 100
    ):
        raise ValueError("Unexpected artifact inventory size")
    lanes = [
        "visual-native-linux", "visual-browser-linux",
        "visual-native-metal", "visual-native-dx12",
    ]
    available = []
    for lane in lanes:
        matches = [entry for entry in entries if entry.get("name") == lane]
        if len(matches) > 1:
            raise ValueError("Source capture artifact is duplicated")
        if not matches:
            continue
        entry = matches[0]
        if (
            entry.get("expired")
            or entry.get("size_in_bytes", MAX_ARCHIVE + 1) > MAX_ARCHIVE
        ):
            raise ValueError("Source capture artifact expired or oversized")
        available.append(lane)
    missing = [lane for lane in lanes[:2] if lane not in available]
    if run.get("conclusion") == "success" and missing:
        raise ValueError("Successful source is missing required capture artifacts")
    return {
        "artifacts": available, "missing": missing,
        "sourceConclusion": run.get("conclusion"), "run": run_id,
        "commit": run["head_sha"],
    }


def extract(data, output):
    if len(data) > MAX_ARCHIVE:
        raise ValueError("Visual artifact exceeds archive limit")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        if len(entries) > 100 or sum(e.file_size for e in entries) > MAX_ARCHIVE:
            raise ValueError("Visual artifact exceeds entry/uncompressed limit")
        names = [entry.filename for entry in entries]
        if len(names) != len(set(names)):
            raise ValueError("Duplicate archive paths")
        for entry in entries:
            name = entry.filename
            if (
                "\\" in name
                or PurePosixPath(name).is_absolute()
                or ".." in PurePosixPath(name).parts
                or ":" in name
                or stat.S_ISLNK(entry.external_attr >> 16)
                or entry.file_size > 4 * 1024 * 1024
            ):
                raise ValueError("Unsafe or oversized archive entry")
        if (
            "captures.json" not in names
            or archive.getinfo("captures.json").file_size > 65536
        ):
            raise ValueError("Missing or oversized capture manifest")
        manifest_bytes = archive.read("captures.json")
        manifest = json.loads(manifest_bytes)
        captures = manifest.get("captures")
        if (
            manifest.get("schema") != "dimensify-visual-captures/1"
            or not isinstance(captures, list)
            or len(captures) != len(CASES)
        ):
            raise ValueError("Invalid capture manifest")
        if {c.get("name") for c in captures} != CASES:
            raise ValueError("Unexpected capture inventory")
        images = {}
        for capture in captures:
            name = capture["name"]
            filename = name + ".png"
            if (
                capture.get("file") != filename
                or capture.get("width") != 256
                or capture.get("height") != 256
                or filename not in names
            ):
                raise ValueError(
                    "Missing image, unsafe filename or unexpected dimensions"
                )
            content = archive.read(filename)
            if not content.startswith(b"\x89PNG\r\n\x1a\n"):
                raise ValueError("Capture is not PNG")
            images[filename] = content
    output.mkdir(parents=True, exist_ok=False)
    (output / "captures.json").write_bytes(manifest_bytes)
    for filename, content in images.items():
        (output / filename).write_bytes(content)


def github_api(endpoint):
    return subprocess.check_output(["gh", "api", endpoint], timeout=90)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", required=True, type=int)
    parser.add_argument("--commit")
    parser.add_argument("--plan", action="store_true")
    parser.add_argument(
        "--artifact",
        choices=[
            "visual-native-linux",
            "visual-browser-linux",
            "visual-native-metal",
            "visual-native-dx12",
        ],
    )
    parser.add_argument("--out", type=Path)
    parser.add_argument("--source-out", type=Path)
    args = parser.parse_args()
    repository = os.environ["GITHUB_REPOSITORY"]
    if (
        not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)
        or args.run <= 0
    ):
        raise ValueError("Invalid repository or run ID")
    base = f"repos/{repository}/actions/runs/{args.run}"
    run = json.loads(github_api(base))
    if args.plan:
        listing = json.loads(github_api(base + "/artifacts?per_page=100"))
        plan = plan_reports(run, listing, repository, args.run)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write("artifacts=" + json.dumps(plan["artifacts"]) + "\n")
                output.write("has_captures=" + str(bool(plan["artifacts"])).lower() + "\n")
                output.write("source_conclusion=" + plan["sourceConclusion"] + "\n")
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as summary:
                summary.write(f"Source Visual correctness run {args.run}: {plan['sourceConclusion']}.\n\n")
                summary.write(f"Captured lanes: {', '.join(plan['artifacts']) or 'none'}.\n\n")
                if plan["missing"]:
                    summary.write("Missing captures: " + ", ".join(plan["missing"]) + ".\n\n")
                if not plan["artifacts"]:
                    summary.write("Pixel Eagle comparison unavailable: no captures were produced. This is not a passed comparison.\n")
        print(json.dumps(plan))
        return
    if not all((args.commit, args.artifact, args.out, args.source_out)):
        parser.error("capture download requires --commit, --artifact, --out and --source-out")
    validate_run(run, repository, args.run, args.commit)
    listing = json.loads(github_api(base + "/artifacts?per_page=100"))
    if listing.get("total_count", 101) > 100:
        raise ValueError("Unexpected artifact inventory size")
    matches = [a for a in listing["artifacts"] if a["name"] == args.artifact]
    if (
        len(matches) != 1
        or matches[0].get("expired")
        or matches[0].get("size_in_bytes", MAX_ARCHIVE + 1) > MAX_ARCHIVE
    ):
        raise ValueError("Source artifact missing, duplicated, expired or oversized")
    artifact_id = matches[0]["id"]
    if not isinstance(artifact_id, int) or artifact_id <= 0:
        raise ValueError("Invalid artifact ID")
    data = github_api(f"repos/{repository}/actions/artifacts/{artifact_id}/zip")
    extract(data, args.out)
    source = {
        "repository": repository,
        "runId": args.run,
        "commit": args.commit,
        "artifact": args.artifact,
        "conclusion": run.get("conclusion"),
    }
    with args.source_out.open("x") as output:
        json.dump(source, output, indent=2)
        output.write("\n")
    print(
        json.dumps(
            {
                "sourceRun": args.run,
                "sourceCommit": args.commit,
                "artifact": args.artifact,
                "captures": len(CASES),
            }
        )
    )


if __name__ == "__main__":
    main()
