import json
from pathlib import Path
import tempfile
import unittest

import perf_ab


def raw(settle, steps):
    """A minimal perf-browser raw.json with one cold run."""
    return {"runs": [{
        "mode": "cold", "inputStartMs": 1000.0, "loadReadyMs": 200.0,
        "rendererEncodedMs": 250.0, "settleMs": settle,
        "observations": [
            {"observedAtMs": 1000.0 + at, "renderReadySplats": count,
             "mainWasmLinearMemoryBytes": 2**20 * 64, "cacheReadyBytes": 2**20 * count // 1000}
            for at, count in steps
        ],
        "final": {"worker": {"started": 2, "dispatchToHandlerMs": 10.0, "roundTripMs": 90.0}},
    }, {"mode": "warm", "settleMs": 99999.0}]}


class PerfAbTests(unittest.TestCase):
    def plan(self, **overrides):
        return {"fixture": "f.json", "arms": {"base": {"site": "a"}, "cand": {"site": "b", "args": ["--lod-underfill", "0"]}},
                "conditions": {"local": [], "slow": ["--throttle-mbps", "50"]}, "repeats": 2, **overrides}

    def test_schedule_alternates_arm_order_within_each_condition(self):
        self.assertEqual(perf_ab.schedule(self.plan()), [
            ("local", 1, "base"), ("local", 1, "cand"), ("local", 2, "cand"), ("local", 2, "base"),
            ("slow", 1, "base"), ("slow", 1, "cand"), ("slow", 2, "cand"), ("slow", 2, "base"),
        ])

    def test_plan_validation_rejects_ambiguous_inputs(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "plan.json"
            for bad in ({"arms": {"only": {"site": "a"}}}, {"arms": {"a": {}, "b": {"site": "b"}}},
                        {"repeats": 0}, {"conditions": {"x": "--preset constrained"}}):
                path.write_text(json.dumps(self.plan(**bad)))
                with self.assertRaises(ValueError):
                    perf_ab.load_plan(path)

    def test_cold_summary_reports_first_threshold_crossings_and_peaks(self):
        row = perf_ab.cold_summary(raw(1500.0, [(100, 0), (300, 31000), (900, 125000), (1400, 500000)]))
        self.assertEqual(row["rounds"], 1)
        self.assertEqual(row["settle_ms"], 1500.0)
        self.assertEqual(row["reach_31000_ms"], 300.0)
        self.assertEqual(row["reach_250000_ms"], 1400.0)
        self.assertEqual(row["ready_splats"], 500000)
        self.assertEqual(row["main_wasm_mib"], 64)
        self.assertEqual(row["workers"], 2)

    def test_unreached_thresholds_remain_unavailable(self):
        row = perf_ab.cold_summary(raw(800.0, [(100, 31000)]))
        self.assertIsNone(row["reach_125000_ms"])

    def test_summarize_takes_median_of_repeats_and_skips_missing_runs(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            (output / "plan.json").write_text(json.dumps(self.plan(conditions={"local": []}, repeats=3)))
            for repeat, settle in ((1, 1000.0), (2, 3000.0), (3, 2000.0)):
                target = output / "local" / f"base-{repeat}" / "browser"
                target.mkdir(parents=True)
                (target / "raw.json").write_text(json.dumps(raw(settle, [(100, 31000)])))
            table = perf_ab.summarize(output)
            self.assertEqual(table[("local", "base")]["settle_ms"], 2000.0)
            self.assertEqual(table[("local", "base")]["repeats"], 3)
            self.assertNotIn(("local", "cand"), table)
            self.assertIn("| local | base |", perf_ab.markdown(table))


if __name__ == "__main__":
    unittest.main()
