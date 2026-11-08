import json
from pathlib import Path
import tempfile
import unittest

import perf_cache_stability as cache


class CacheStabilityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for i, name in enumerate(cache.NAMES):
            directory = self.root / str(i) / "new"
            directory.mkdir(parents=True)
            (directory / "benchmark.json").write_text(json.dumps({"full_id": name}))
            (directory / "sample.json").write_text(
                json.dumps({"iters": [1] * 100, "times": [1000] * 100})
            )
            self.estimate(directory, 1000)

    def estimate(self, directory, point):
        (directory / "estimates.json").write_text(
            json.dumps({"mean": {"point_estimate": point, "confidence_interval": {
                "lower_bound": point * 0.99, "upper_bound": point * 1.01,
                "confidence_level": 0.95}}})
        )

    def test_complete_raw_samples_are_required(self):
        rows = cache.load_repeat(self.root)
        self.assertEqual(set(rows), set(cache.NAMES))
        self.assertEqual(rows[cache.ADMISSION]["samples"], 100)
        (self.root / "0/new/sample.json").unlink()
        with self.assertRaises((ValueError, FileNotFoundError)):
            cache.load_repeat(self.root)

    def test_unexpected_workloads_are_rejected(self):
        (self.root / "0/new/benchmark.json").write_text('{"full_id":"unknown"}')
        with self.assertRaises(ValueError):
            cache.load_repeat(self.root)

    def test_missing_workloads_are_rejected(self):
        (self.root / "0/new/benchmark.json").unlink()
        with self.assertRaises(ValueError):
            cache.load_repeat(self.root)

    def test_insufficient_or_invalid_samples_are_rejected(self):
        for samples in [[], [1] * 99, [float("nan")] * 100, [0] * 100]:
            (self.root / "0/new/sample.json").write_text(
                json.dumps({"iters": [1] * len(samples), "times": samples})
            )
            with self.assertRaises(ValueError):
                cache.load_repeat(self.root)

    def test_negative_estimate_is_rejected(self):
        self.estimate(self.root / "0/new", -1)
        with self.assertRaises(ValueError):
            cache.load_repeat(self.root)

    def test_invalid_confidence_level_is_rejected(self):
        path = self.root / "0/new/estimates.json"
        estimates = json.loads(path.read_text())
        estimates["mean"]["confidence_interval"]["confidence_level"] = 0.99
        path.write_text(json.dumps(estimates))
        with self.assertRaises(ValueError):
            cache.load_repeat(self.root)

    def test_reversed_positive_confidence_bounds_are_rejected(self):
        path = self.root / "0/new/estimates.json"
        estimates = json.loads(path.read_text())
        estimates["mean"]["confidence_interval"].update(lower_bound=1100, upper_bound=900)
        path.write_text(json.dumps(estimates))
        with self.assertRaises(ValueError):
            cache.load_repeat(self.root)

    def test_linear_sampling_uses_slope_estimate(self):
        path = self.root / "0/new/estimates.json"
        estimates = json.loads(path.read_text())
        estimates["slope"] = {"point_estimate": 2000, "confidence_interval": {
            "lower_bound": 1990, "upper_bound": 2010, "confidence_level": 0.95}}
        path.write_text(json.dumps(estimates))
        row = cache.load_repeat(self.root)[cache.NAMES[0]]
        self.assertEqual(row["point_ns"], 2000)
        self.assertEqual(row["estimator"], "slope")

    def test_duplicate_workloads_are_rejected(self):
        (self.root / "1/new/benchmark.json").write_text(
            json.dumps({"full_id": cache.NAMES[0]})
        )
        with self.assertRaises(ValueError):
            cache.load_repeat(self.root)

    def test_fewer_than_three_processes_cannot_claim_repeatability(self):
        row = cache.load_repeat(self.root)
        with self.assertRaises(ValueError):
            cache.summarize([row, row])

    def test_legacy_variation_does_not_hide_stable_admission(self):
        rows = []
        for value in [1000, 3000, 1000]:
            self.estimate(self.root / "0/new", value)
            rows.append(cache.load_repeat(self.root))
        summary = cache.summarize(rows)
        self.assertFalse(summary[cache.NAMES[0]]["repeatable"])
        self.assertTrue(summary[cache.ADMISSION]["repeatable"])

    def test_wide_interval_prevents_repeatability_claim(self):
        rows = cache.load_repeat(self.root)
        rows[cache.ADMISSION]["upper_ns"] = 2000
        self.assertFalse(cache.summarize([rows] * 3)[cache.ADMISSION]["repeatable"])

    def test_ci_summary_separates_repeatability_from_regression(self):
        rows = cache.load_repeat(self.root)
        summary = cache.markdown(cache.summarize([rows] * 3))
        self.assertIn("Diagnostic only", summary)
        self.assertIn(cache.ADMISSION, summary)
        self.assertIn("v1 regression gate remains unchanged", summary)


if __name__ == "__main__":
    unittest.main()
