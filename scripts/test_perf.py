"""Fail closed when performance measurements or regression baselines are missing."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("perf", Path(__file__).with_name("perf.py"))
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)

NAMES = ["sog-decode-v1/65536", "depth-sort-v1/100000", "depth-sort-v1/1000000",
         "lod-targets-v1/1000", "lod-targets-v1/10000"]


def output():
    return "\n".join(f"{name}\n    time: [1.0 ms 1.1 ms 1.2 ms]" for name in NAMES)


def report(value=1_100_000.0, boundary=True):
    return {"alerts": [], "results": [[{
        "benchmark": {"name": name},
        "measures": [{"measure": {"slug": "latency"},
                      "metric": {"value": value},
                      "boundary": {"baseline": 1_000_000.0, "upper_limit": 1_200_000.0}
                      if boundary else None}],
    } for name in NAMES]]}


class CriterionTests(unittest.TestCase):
    def test_reads_all_workloads_and_converts_units_to_nanoseconds(self):
        values = perf.criterion_results(output())
        self.assertEqual(set(values), set(NAMES))
        self.assertEqual(values[NAMES[0]], 1_100_000.0)
        self.assertEqual(perf.criterion_results(output().replace("ms", "ns"))[NAMES[0]], 1.1)
        self.assertEqual(perf.criterion_results(output().replace("ms", "us"))[NAMES[0]], 1100.0)

    def test_missing_duplicate_and_nonfinite_results_are_errors(self):
        for text in ["", output().replace(NAMES[0], "unknown"),
                     output() + "\n" + output(), output().replace("1.1", "NaN"),
                     output().replace("1.0 ms", "1.3 ms")]:
            with self.subTest(text=text[:70]), self.assertRaises(ValueError):
                perf.criterion_results(text)


class GateTests(unittest.TestCase):
    def test_normal_report_passes(self):
        perf.validate_report(report())

    def test_synthetic_25_percent_slowdown_fails_even_without_alert_array(self):
        with self.assertRaisesRegex(ValueError, "regression"):
            perf.validate_report(report(value=1_250_000.0))

    def test_missing_baseline_never_passes_the_gate(self):
        for boundary in [None, {}, {"baseline": None, "upper_limit": None}]:
            data = report()
            data["results"][0][0]["measures"][0]["boundary"] = boundary
            with self.subTest(boundary=boundary), self.assertRaisesRegex(ValueError, "baseline"):
                perf.validate_report(data)

    def test_bootstrap_accepts_missing_baseline_but_still_requires_all_results(self):
        perf.validate_report(report(boundary=False), require_baseline=False)
        data = report(boundary=False)
        data["results"][0].pop()
        with self.assertRaises(ValueError):
            perf.validate_report(data, require_baseline=False)

    def test_alerts_missing_measure_and_nonfinite_values_fail(self):
        data = report()
        data["alerts"] = [{"status": "active"}]
        missing = report()
        missing["results"][0][0]["measures"] = []
        invalid = report()
        invalid["results"][0][0]["measures"][0]["metric"]["value"] = float("nan")
        for item in [data, missing, invalid, {}, {"results": []}]:
            with self.subTest(item=item), self.assertRaises(ValueError):
                perf.validate_report(item)

    def test_modern_report_metric_boundaries_are_supported(self):
        data = report()
        for result in data["results"][0]:
            measure = result["measures"][0]
            measure["metrics"] = [{"name": "value", "value": measure.pop("metric")["value"],
                                   "boundaries": [{"boundary": measure.pop("boundary")}]}]
        perf.validate_report(data)
        data["results"][0][0]["measures"][0]["metrics"][0]["value"] = 1_250_000
        with self.assertRaisesRegex(ValueError, "regression"):
            perf.validate_report(data)


class TestbedTests(unittest.TestCase):
    def test_different_hardware_or_toolchain_never_shares_baselines(self):
        args = ["ubuntu-24.04", "x86_64", "Intel Xeon 8370C", "rustc 1.98.1"]
        baseline = perf.testbed_name(*args)
        self.assertRegex(baseline, r"^[a-z0-9-]{1,64}$")
        self.assertEqual(baseline, perf.testbed_name(*args))
        for i in range(len(args)):
            changed = copy.copy(args)
            changed[i] += "other"
            self.assertNotEqual(baseline, perf.testbed_name(*changed))


class UploadTests(unittest.TestCase):
    def invoke(self, env):
        return subprocess.run([sys.executable, str(Path(__file__).with_name("perf.py")), "upload"],
                              env=env, capture_output=True, text=True)

    def test_missing_configuration_fails_without_publishing(self):
        env = {k: v for k, v in os.environ.items() if not k.startswith("BENCHER_")}
        result = self.invoke(env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing BENCHER_PROJECT", result.stderr)
        env["BENCHER_PROJECT"] = "test-project"
        result = self.invoke(env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Missing BENCHER_API_KEY", result.stderr)

    def test_fork_is_refused_even_if_credentials_are_available(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            event.write_text(json.dumps({"pull_request": {"head": {"repo": {"full_name": "fork/dimensify"}}}}))
            env = {**os.environ, "BENCHER_PROJECT": "test-project", "BENCHER_API_KEY": "test-only-key",
                   "GITHUB_EVENT_PATH": str(event), "GITHUB_REPOSITORY": "soraxas/dimensify"}
            result = self.invoke(env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Fork PRs are artifact-only", result.stderr)
            self.assertNotIn("test-only-key", result.stderr)

    def test_manual_feature_branch_cannot_pollute_main_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            event.write_text("{}")
            env = {**os.environ, "BENCHER_PROJECT": "test-project", "BENCHER_API_KEY": "test-only-key",
                   "GITHUB_EVENT_PATH": str(event), "GITHUB_REF": "refs/heads/feature"}
            result = self.invoke(env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Non-PR publication is only allowed on main", result.stderr)


if __name__ == "__main__":
    unittest.main()
