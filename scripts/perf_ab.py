"""Interleaved baseline/candidate browser comparisons over real-scene fixtures.

`site` builds an instrumented viewer from a git ref (in a temporary worktree)
or from the current checkout. `run` executes a JSON plan of arms x conditions
through `perf_scenes.py run --kind browser`, alternating arm order per repeat.
`summarize` reports cold-load progression medians from the retained raw output.
Results are local diagnostics; nothing is published.
"""

import argparse
import json
import math
from pathlib import Path
import shlex
import statistics
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
# Logical ready-splat counts whose first observation marks visible progress.
DEFAULT_THRESHOLDS = (31_000, 125_000, 250_000, 500_000)


def build_site(ref, output):
    """Build a benchmark site from `ref`, or the working tree when ref is None."""
    command = [sys.executable, str(ROOT / "scripts/perf_scenes.py"), "web-build"]
    if ref is None:
        subprocess.run(command + ["--output", str(output)], cwd=ROOT, check=True)
        return
    with tempfile.TemporaryDirectory(prefix="perf-ab-") as parent:
        checkout = Path(parent) / "checkout"
        subprocess.run(
            ["git", "worktree", "add", "--detach", str(checkout), ref],
            cwd=ROOT,
            check=True,
        )
        try:
            subprocess.run(
                command + ["--checkout", str(checkout), "--output", str(output.resolve())],
                cwd=ROOT,
                check=True,
            )
        finally:
            subprocess.run(
                ["git", "worktree", "remove", "--force", str(checkout)],
                cwd=ROOT,
                check=False,
            )


def load_plan(path):
    plan = json.loads(Path(path).read_text())
    arms, conditions = plan.get("arms"), plan.get("conditions", {"default": []})
    if not isinstance(arms, dict) or len(arms) < 2:
        raise ValueError("plan needs at least two arms")
    for name, arm in arms.items():
        if not isinstance(arm, dict) or not isinstance(arm.get("site"), str):
            raise ValueError(f"arm {name} needs a site path")
        if not all(isinstance(a, str) for a in arm.get("args", [])):
            raise ValueError(f"arm {name} args must be strings")
    for name, args in conditions.items():
        if not isinstance(args, list) or not all(isinstance(a, str) for a in args):
            raise ValueError(f"condition {name} must be a list of strings")
    repeats = plan.get("repeats", 2)
    if not isinstance(repeats, int) or repeats < 1:
        raise ValueError("repeats must be a positive integer")
    if not isinstance(plan.get("fixture"), str):
        raise ValueError("plan needs a fixture path")
    return {**plan, "conditions": conditions, "repeats": repeats}


def schedule(plan):
    """Ordered (condition, repeat, arm) runs; odd repeats reverse arm order."""
    arms = list(plan["arms"])
    runs = []
    for condition in plan["conditions"]:
        for repeat in range(plan["repeats"]):
            order = arms if repeat % 2 == 0 else arms[::-1]
            runs += [(condition, repeat + 1, arm) for arm in order]
    return runs


def run_plan(plan_path, output):
    plan = load_plan(plan_path)
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError(f"output {output} must be new or empty")
    (output / "plan.json").write_text(json.dumps(plan, indent=2) + "\n")
    rounds = str(plan.get("rounds", 3))
    failures = 0
    for condition, repeat, arm in schedule(plan):
        target = output / condition / f"{arm}-{repeat}"
        target.parent.mkdir(parents=True, exist_ok=True)
        command = [
            sys.executable, str(ROOT / "scripts/perf_scenes.py"), "run",
            "--kind", "browser", "--fixture", plan["fixture"],
            "--site", plan["arms"][arm]["site"], "--rounds", rounds,
            "--output", str(target),
            *plan["conditions"][condition], *plan["arms"][arm].get("args", []),
        ]
        print(f"== {condition} {arm} #{repeat}: {shlex.join(command[2:])}", flush=True)
        with (target.parent / f"{arm}-{repeat}.log").open("w") as log:
            code = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT).returncode
        failures += code != 0
        print(f"   exit={code}", flush=True)
    return failures


def _median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def cold_summary(raw, thresholds=DEFAULT_THRESHOLDS):
    """Per-directory cold-load medians from one perf-browser raw.json."""
    runs = [r for r in raw["runs"] if r["mode"] == "cold"]

    def reach(run, count):
        start = run["inputStartMs"]
        for observation in run["observations"]:
            if observation["renderReadySplats"] >= count:
                return observation["observedAtMs"] - start
        return None

    def peak(run, key, scale=1):
        values = [o.get(key) or 0 for o in run["observations"]]
        return max(values) / scale if values else None

    worker = lambda run, key: (run["final"].get("worker") or {}).get(key)
    row = {
        "rounds": len(runs),
        "load_ready_ms": _median(r["loadReadyMs"] for r in runs),
        "encoded_ms": _median(r["rendererEncodedMs"] for r in runs),
        "settle_ms": _median(r["settleMs"] for r in runs),
        "ready_splats": _median(peak(r, "renderReadySplats") for r in runs),
        "workers": _median(worker(r, "started") for r in runs),
        "dispatch_to_handler_ms": _median(worker(r, "dispatchToHandlerMs") for r in runs),
        "worker_roundtrip_ms": _median(worker(r, "roundTripMs") for r in runs),
        "main_wasm_mib": _median(peak(r, "mainWasmLinearMemoryBytes", 2**20) for r in runs),
        "cache_ready_mib": _median(peak(r, "cacheReadyBytes", 2**20) for r in runs),
    }
    for count in thresholds:
        row[f"reach_{count}_ms"] = _median(reach(r, count) for r in runs)
    return row | motion_summary(raw)


def _nearest_rank(values, quantile):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * quantile) - 1)] if ordered else None


def motion_summary(raw):
    """Warm replay app-frame intervals over camera-motion cases."""
    p50, p95, share = [], [], []
    for run in (r for r in raw["runs"] if r["mode"] == "warm"):
        cases = [c for c in run.get("cases", []) if c["name"].startswith("moving")]
        intervals = [i for c in cases for i in c["appIntervals"]]
        if not intervals:
            continue
        p50.append(_nearest_rank(intervals, 0.5))
        p95.append(_nearest_rank(intervals, 0.95))
        share.append(100 * sum(c.get("stochasticFrames", 0) for c in cases) / len(intervals))
    return {
        "moving_p50_ms": _median(p50),
        "moving_p95_ms": _median(p95),
        "moving_stochastic_pct": _median(share),
    }


def summarize(output, thresholds=DEFAULT_THRESHOLDS):
    """Median of per-repeat medians per condition and arm."""
    plan = load_plan(output / "plan.json")
    table = {}
    for condition in plan["conditions"]:
        for arm in plan["arms"]:
            rows = []
            for repeat in range(1, plan["repeats"] + 1):
                raw = output / condition / f"{arm}-{repeat}" / "browser" / "raw.json"
                if raw.exists():
                    rows.append(cold_summary(json.loads(raw.read_text()), thresholds))
            if rows:
                table[(condition, arm)] = {
                    key: _median(row[key] for row in rows) for key in rows[0]
                } | {"repeats": len(rows)}
    return table


def markdown(table):
    if not table:
        return "No completed runs."
    columns = list(next(iter(table.values())))
    lines = ["| condition | arm | " + " | ".join(columns) + " |",
             "|---" * (len(columns) + 2) + "|"]
    for (condition, arm), row in table.items():
        cells = ["-" if row[c] is None else f"{row[c]:.0f}" for c in columns]
        lines.append(f"| {condition} | {arm} | " + " | ".join(cells) + " |")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    site = actions.add_parser("site", help="Build an instrumented site")
    site.add_argument("--ref", help="git ref to build; omit for the working tree")
    site.add_argument("--output", type=Path, required=True)
    run = actions.add_parser("run", help="Execute an interleaved plan")
    run.add_argument("--plan", type=Path, required=True)
    run.add_argument("--output", type=Path, required=True)
    summary = actions.add_parser("summarize", help="Report cold-load medians")
    summary.add_argument("output", type=Path)
    summary.add_argument("--json", action="store_true")
    args = parser.parse_args()
    if args.action == "site":
        build_site(args.ref, args.output)
    elif args.action == "run":
        failures = run_plan(args.plan, args.output)
        print(markdown(summarize(args.output)))
        sys.exit(1 if failures else 0)
    else:
        table = summarize(args.output)
        if args.json:
            print(json.dumps([{"condition": c, "arm": a, **row} for (c, a), row in table.items()], indent=2))
        else:
            print(markdown(table))


if __name__ == "__main__":
    main()
