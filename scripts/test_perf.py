"""Fail closed when performance measurements or regression baselines are missing."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "perf", Path(__file__).with_name("perf.py")
)
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)

NAMES = [
    "sog-decode-v1/65536",
    "depth-sort-v1/100000",
    "depth-sort-v1/1000000",
    "lod-targets-v1/1000",
    "lod-targets-v1/10000",
]


def output():
    return "\n".join(f"{name}\n    time: [1.0 ms 1.1 ms 1.2 ms]" for name in NAMES)


def report(value=1_100_000.0, boundary=True):
    return {
        "alerts": [],
        "results": [
            [
                {
                    "benchmark": {"name": name},
                    "measures": [
                        {
                            "measure": {"slug": "latency"},
                            "metric": {"value": value},
                            "boundary": {
                                "baseline": 1_000_000.0,
                                "upper_limit": 1_200_000.0,
                            }
                            if boundary
                            else None,
                        }
                    ],
                }
                for name in NAMES
            ]
        ],
    }


class CriterionTests(unittest.TestCase):
    def test_reads_all_workloads_and_converts_units_to_nanoseconds(self):
        values = perf.criterion_results(output())
        self.assertEqual(set(values), set(NAMES))
        self.assertEqual(values[NAMES[0]], 1_100_000.0)
        self.assertEqual(
            perf.criterion_results(output().replace("ms", "ns"))[NAMES[0]], 1.1
        )
        self.assertEqual(
            perf.criterion_results(output().replace("ms", "us"))[NAMES[0]], 1100.0
        )

    def test_missing_duplicate_and_nonfinite_results_are_errors(self):
        for text in [
            "",
            output().replace(NAMES[0], "unknown"),
            output() + "\n" + output(),
            output().replace("1.1", "NaN"),
            output().replace("1.0 ms", "1.3 ms"),
        ]:
            with self.subTest(text=text[:70]), self.assertRaises(ValueError):
                perf.criterion_results(text)


class GateTests(unittest.TestCase):
    def test_malformed_alert_fields_fail_without_tracebacks_or_raw_values(self):
        for field in ["benchmark", "boundary", "project"]:
            for malformed in ["secret-token", ["secret-token"], 42]:
                data = report()
                alert = {
                    "benchmark": {"name": NAMES[0]},
                    "value": 1_250_000,
                    "boundary": {"baseline": 1_000_000, "upper_limit": 1_200_000},
                }
                data["alerts"] = [alert]
                (data if field == "project" else alert)[field] = malformed
                with (
                    self.subTest(field=field, value=malformed),
                    self.assertRaises(ValueError) as error,
                ):
                    perf.validate_report(data)
                self.assertNotIn("secret-token", str(error.exception))

    def test_report_errors_never_echo_unknown_names_or_raw_numeric_fields(self):
        for modern in [False, True]:
            for field in ["name", "value", "baseline", "upper_limit"]:
                data = report()
                row = data["results"][0][0]
                measure = row["measures"][0]
                if field == "name":
                    row["benchmark"]["name"] = "secret-token"
                elif field == "value":
                    measure["metric"]["value"] = "secret-token"
                else:
                    measure["boundary"][field] = "secret-token"
                if modern:
                    measure["metrics"] = [
                        {
                            "name": "value",
                            "value": measure.pop("metric")["value"],
                            "boundaries": [{"boundary": measure.pop("boundary")}],
                        }
                    ]
                with (
                    self.subTest(modern=modern, field=field),
                    self.assertRaises(ValueError) as error,
                ):
                    perf.validate_report(data)
                self.assertNotIn("secret-token", str(error.exception))

    def test_non_string_benchmark_names_fail_closed(self):
        for name in [None, 123, [], {}]:
            data = report()
            data["results"][0][0]["benchmark"]["name"] = name
            with self.subTest(name=name), self.assertRaises(ValueError):
                perf.validate_report(data)

    def test_normal_report_passes(self):
        perf.validate_report(report())

    def test_synthetic_25_percent_slowdown_fails_even_without_alert_array(self):
        with self.assertRaisesRegex(ValueError, "regression"):
            perf.validate_report(report(value=1_250_000.0))

    def test_missing_baseline_never_passes_the_gate(self):
        for boundary in [None, {}, {"baseline": None, "upper_limit": None}]:
            data = report()
            data["results"][0][0]["measures"][0]["boundary"] = boundary
            with (
                self.subTest(boundary=boundary),
                self.assertRaisesRegex(ValueError, "baseline"),
            ):
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
            measure["metrics"] = [
                {
                    "name": "value",
                    "value": measure.pop("metric")["value"],
                    "boundaries": [{"boundary": measure.pop("boundary")}],
                }
            ]
        perf.validate_report(data)
        data["results"][0][0]["measures"][0]["metrics"][0]["value"] = 1_250_000
        with self.assertRaisesRegex(ValueError, "regression"):
            perf.validate_report(data)


class TestbedTests(unittest.TestCase):
    def test_pipeline_and_small_cpu_suite_never_share_a_testbed(self):
        args = ["ubuntu-24.04", "x86_64", "CPU", "rustc 1.98.1"]
        self.assertNotEqual(
            perf.testbed_name(*args), perf.testbed_name(*args, suite="pipeline")
        )

    def test_different_hardware_or_toolchain_never_shares_baselines(self):
        args = ["ubuntu-24.04", "x86_64", "Intel Xeon 8370C", "rustc 1.98.1"]
        baseline = perf.testbed_name(*args)
        self.assertRegex(baseline, r"^[a-z0-9-]{1,64}$")
        self.assertEqual(baseline, perf.testbed_name(*args))
        for i in range(len(args)):
            changed = copy.copy(args)
            changed[i] += "other"
            self.assertNotEqual(baseline, perf.testbed_name(*changed))


class PipelineTests(unittest.TestCase):
    def test_full_pipeline_suite_is_required_independently_of_original_cpu_suite(self):
        names = perf.suite_names("pipeline")
        self.assertEqual(len(names), 12)
        self.assertIn("pipeline-sog-packed-v1/65536", names)
        self.assertIn("pipeline-relocation-v1/pressure", names)
        self.assertIn("pipeline-scheduler-v1/resident", names)
        text = "\n".join(
            f"{name}\n    time: [1 ms 1.1 ms 1.2 ms]" for name in sorted(names)
        )
        self.assertEqual(set(perf.criterion_results(text, names)), names)
        with self.assertRaises(ValueError):
            perf.criterion_results(text)
        with self.assertRaises(ValueError):
            perf.criterion_results(text + "\n" + output(), names)
        with self.assertRaises(ValueError):
            perf.suite_names("unknown")

    def test_every_new_pipeline_metric_needs_a_valid_baseline(self):
        names = perf.suite_names("pipeline")
        data = report()
        template = data["results"][0][0]
        data["results"][0] = []
        for name in sorted(names):
            row = copy.deepcopy(template)
            row["benchmark"]["name"] = name
            data["results"][0].append(row)
        perf.validate_report(data, names=names)
        data["results"][0][-1]["measures"][0]["boundary"] = None
        with self.assertRaisesRegex(ValueError, "baseline"):
            perf.validate_report(data, names=names)


class UploadTests(unittest.TestCase):
    def publish_result(self, data, returncode=1, require_baseline=True):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            (target / "criterion-1.txt").write_text(output())
            response = SimpleNamespace(
                stdout=json.dumps(data) if isinstance(data, dict) else data,
                stderr="secret-token must never be printed",
                returncode=returncode,
            )
            with patch.object(perf.subprocess, "run", return_value=response):
                perf.publish_saved([], target, 1, require_baseline, set(NAMES))

    def test_nonzero_report_explains_regression_without_service_stderr(self):
        data = report()
        data["uuid"] = "01a18f55-9ef2-70d5-9871-7078820d7dd2"
        data["project"] = {"slug": "dimensify"}
        data["alerts"] = [
            {
                "benchmark": {"name": NAMES[0]},
                "value": 1_250_000,
                "boundary": {"baseline": 1_000_000, "upper_limit": 1_200_000},
            }
        ]
        with self.assertRaises(ValueError) as error:
            self.publish_result(data)
        message = str(error.exception)
        for expected in [
            NAMES[0],
            "1.25e+06 ns",
            "baseline 1e+06 ns",
            "limit 1.2e+06 ns",
            "https://bencher.dev/console/projects/dimensify/reports/" + data["uuid"],
        ]:
            self.assertIn(expected, message)
        self.assertNotIn("secret-token", message)

    def test_nonzero_missing_baseline_is_distinct_from_transport_error(self):
        with self.assertRaisesRegex(ValueError, "Missing baseline"):
            self.publish_result(report(boundary=False))

    def test_non_json_failure_withholds_stderr(self):
        with self.assertRaisesRegex(ValueError, "no valid JSON report") as error:
            self.publish_result("invalid response secret-token")
        self.assertNotIn("secret-token", str(error.exception))

    def test_nonzero_valid_report_still_fails(self):
        with self.assertRaisesRegex(ValueError, "command failed"):
            self.publish_result(report())

    def test_alert_output_does_not_echo_unknown_benchmarks_or_metadata(self):
        data = report()
        data["alerts"] = [{"benchmark": {"name": "secret-token"}}]
        data["project"] = {"slug": "secret-token?credential=secret-token"}
        data["uuid"] = "secret-token"
        with self.assertRaisesRegex(ValueError, "regression") as error:
            self.publish_result(data)
        self.assertNotIn("secret-token", str(error.exception))

    def test_pipeline_report_uses_its_own_pr_branch_and_thresholds(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            event = output / "event.json"
            commit = "a" * 40
            event.write_text(
                json.dumps(
                    {
                        "pull_request": {
                            "number": 42,
                            "head": {
                                "sha": commit,
                                "repo": {"full_name": "owner/dimensify"},
                            },
                            "base": {"sha": "b" * 40},
                        }
                    }
                )
            )
            (output / "environment.json").write_text(
                json.dumps(
                    {
                        "commit": commit,
                        "dirty": False,
                        "runs": 1,
                        "suite": "pipeline",
                        "testbed": "pipeline-cpu-v1-test",
                        "system": "Ubuntu 24.04.5 LTS",
                        "arch": "x86_64",
                        "cpu": "AMD EPYC 9V74 80-Core Processor",
                        "rust": "rustc 1.98.1",
                    }
                )
            )
            (output / "criterion-1.txt").write_text(
                "\n".join(
                    f"{name}\n    time: [1 ms 1.1 ms 1.2 ms]"
                    for name in sorted(perf.PIPELINE_NAMES)
                )
            )
            data = report()
            template = data["results"][0][0]
            data["results"][0] = []
            for name in sorted(perf.PIPELINE_NAMES):
                row = copy.deepcopy(template)
                row["benchmark"]["name"] = name
                data["results"][0].append(row)
            env = {
                "BENCHER_PROJECT": "test",
                "BENCHER_API_KEY": "test-only-key",
                "GITHUB_EVENT_PATH": str(event),
                "GITHUB_REPOSITORY": "owner/dimensify",
            }
            result = subprocess.CompletedProcess([], 0, json.dumps(data), "")
            with (
                patch.dict(os.environ, env, clear=True),
                patch.object(perf, "ensure_name") as naming,
                patch.object(perf.subprocess, "run", return_value=result) as call,
            ):
                perf.upload(output, False, "pipeline")
            naming.assert_called_once_with(
                "test",
                "pipeline-cpu-v1-test",
                "pipeline-cpu-epyc-9v74-ubuntu-24-04-5-x64-rust1-98-1",
            )
            args = call.call_args.args[0]
            self.assertEqual(args[args.index("--branch") + 1], "pipeline-pr-42")
            self.assertEqual(args[args.index("--start-point") + 1], "main")
            self.assertIn("--error-on-alert", args)

    def invoke(self, env):
        return subprocess.run(
            [sys.executable, str(Path(__file__).with_name("perf.py")), "upload"],
            env=env,
            capture_output=True,
            text=True,
        )

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
            event.write_text(
                json.dumps(
                    {
                        "pull_request": {
                            "head": {"repo": {"full_name": "fork/dimensify"}}
                        }
                    }
                )
            )
            env = {
                **os.environ,
                "BENCHER_PROJECT": "test-project",
                "BENCHER_API_KEY": "test-only-key",
                "GITHUB_EVENT_PATH": str(event),
                "GITHUB_REPOSITORY": "soraxas/dimensify",
            }
            result = self.invoke(env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Fork PRs are artifact-only", result.stderr)
            self.assertNotIn("test-only-key", result.stderr)

    def test_manual_feature_branch_cannot_pollute_main_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            event.write_text("{}")
            env = {
                **os.environ,
                "BENCHER_PROJECT": "test-project",
                "BENCHER_API_KEY": "test-only-key",
                "GITHUB_EVENT_PATH": str(event),
                "GITHUB_REF": "refs/heads/feature",
            }
            result = self.invoke(env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Non-PR publication is only allowed on main", result.stderr)


class MeasurementCheckoutTests(unittest.TestCase):
    def test_git_provenance_uses_the_requested_checkout(self):
        with tempfile.TemporaryDirectory() as directory:
            checkout = Path(directory).resolve()
            subprocess.run(["git", "init", "--quiet", str(checkout)], check=True)
            self.assertEqual(
                perf.command_output(
                    "git", "rev-parse", "--show-toplevel", checkout=checkout
                ),
                str(checkout),
            )

    def test_base_measurement_runs_three_processes_in_the_base_checkout_without_keys(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            checkout = root / "source"
            checkout.mkdir()
            destination = root / "measurements"
            commit = "b" * 40

            def command(*args, checkout=perf.ROOT):
                if args[:2] == ("git", "rev-parse"):
                    self.assertEqual(checkout, (root / "source").resolve())
                    return commit
                if args[:2] == ("git", "status"):
                    self.assertEqual(checkout, (root / "source").resolve())
                    return ""
                return "test hardware or compiler"

            def process(*args, **kwargs):
                return SimpleNamespace(
                    stdout=iter(output().splitlines(keepends=True)), wait=lambda: 0
                )

            with (
                patch.dict(
                    os.environ,
                    {
                        "BENCHER_API_KEY": "test-only",
                        "GH_TOKEN": "test-only",
                        "GITHUB_RUN_ID": "123",
                        "GITHUB_RUN_ATTEMPT": "1",
                        "RUNNER_NAME": "same-runner",
                    },
                ),
                patch.object(perf, "command_output", side_effect=command),
                patch.object(perf.subprocess, "run") as build,
                patch.object(perf.subprocess, "Popen", side_effect=process) as runs,
                patch("builtins.print"),
            ):
                perf.run(destination, 3, checkout=checkout)
            self.assertEqual(runs.call_count, 3)
            for call in [build.call_args, *runs.call_args_list]:
                self.assertEqual(call.kwargs["cwd"], checkout.resolve())
                self.assertNotIn("BENCHER_API_KEY", call.kwargs["env"])
                self.assertNotIn("GH_TOKEN", call.kwargs["env"])
            metadata = json.loads((destination / "environment.json").read_text())
            self.assertEqual(metadata["commit"], commit)
            self.assertEqual(metadata["commit_after"], commit)
            self.assertFalse(metadata["dirty_after"])
            self.assertEqual(
                metadata["measurement_job"],
                {"run_id": "123", "attempt": "1", "runner": "same-runner"},
            )
            for number in range(1, 4):
                self.assertEqual(
                    set(
                        perf.criterion_results(
                            (destination / f"criterion-{number}.txt").read_text()
                        )
                    ),
                    set(NAMES),
                )


class PairedBaselineTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.head = self.root / "head"
        self.base = self.root / "base"
        self.head.mkdir()
        self.base.mkdir()
        self.event = {
            "pull_request": {
                "number": 16,
                "head": {"sha": "a" * 40, "repo": {"full_name": "owner/dimensify"}},
                "base": {"sha": "b" * 40},
            }
        }
        event_path = self.root / "event.json"
        event_path.write_text(json.dumps(self.event))
        self.env = {
            "BENCHER_PROJECT": "test",
            "BENCHER_API_KEY": "test-only-key",
            "GITHUB_EVENT_PATH": str(event_path),
            "GITHUB_REPOSITORY": "owner/dimensify",
            "GITHUB_RUN_ID": "12345",
            "GITHUB_RUN_ATTEMPT": "2",
            "GH_TOKEN": "test-only-gh",
        }
        common = dict(
            dirty=False,
            cpu="AMD EPYC 7763",
            arch="x86_64",
            system="Ubuntu 24.04.5 LTS",
            rust="rustc 1.98.1",
            runner_image="image",
            profile="bench",
            workload_version=1,
            suite="cpu",
            testbed="cpu-v1-test",
            measurement_job={"run_id": "12345", "attempt": "2", "runner": "runner-one"},
        )
        for directory, commit, runs in [
            (self.head, "a" * 40, 1),
            (self.base, "b" * 40, 3),
        ]:
            metadata = dict(
                common, commit=commit, commit_after=commit, dirty_after=False, runs=runs
            )
            (directory / "environment.json").write_text(json.dumps(metadata))
            for number in range(1, runs + 1):
                (directory / f"criterion-{number}.txt").write_text(output())

    def upload(self, candidate_report=None):
        responses = [
            subprocess.CompletedProcess([], 0, json.dumps(report(boundary=False)), "")
            for _ in range(3)
        ]
        responses.append(
            subprocess.CompletedProcess(
                [], 0, json.dumps(candidate_report or report()), ""
            )
        )
        with (
            patch.dict(os.environ, self.env, clear=True),
            patch.object(perf, "ensure_name"),
            patch.object(perf.subprocess, "run", side_effect=responses) as calls,
        ):
            perf.upload(self.head, False, paired_base=self.base)
        return [call.args[0] for call in calls.call_args_list]

    def change_base(self, **changes):
        path = self.base / "environment.json"
        data = json.loads(path.read_text())
        data.update(changes)
        path.write_text(json.dumps(data))

    def test_three_real_base_reports_precede_candidate_on_isolated_start_point(self):
        calls = self.upload()
        self.assertEqual(len(calls), 4)
        for args in calls[:3]:
            self.assertEqual(args[args.index("--branch") + 1], "base-pr-16-run-12345-2")
            self.assertEqual(args[args.index("--hash") + 1], "b" * 40)
            self.assertNotIn("--github-actions", args)
            self.assertNotIn("--start-point", args)
            self.assertEqual(args[args.index("--threshold-min-sample-size") + 1], "3")
        candidate = calls[-1]
        self.assertEqual(candidate[candidate.index("--branch") + 1], "pr-16")
        self.assertEqual(
            candidate[candidate.index("--start-point") + 1], "base-pr-16-run-12345-2"
        )
        self.assertEqual(candidate[candidate.index("--start-point-hash") + 1], "b" * 40)
        self.assertIn("--start-point-reset", candidate)
        self.assertIn("--start-point-clone-thresholds", candidate)
        self.assertIn("--error-on-alert", candidate)
        self.assertEqual(
            candidate[candidate.index("--threshold-upper-boundary") + 1], "0.20"
        )

    def test_pair_rejects_wrong_revision_hardware_or_measurement_job_before_upload(
        self,
    ):
        original = (self.base / "environment.json").read_text()
        changes = [
            dict(commit="a" * 40),
            dict(commit_after="c" * 40),
            dict(dirty=True),
            dict(dirty_after=True),
            dict(runs=2),
            dict(cpu="other CPU"),
            dict(rust="other Rust"),
            dict(testbed="other testbed"),
            dict(runner_image="other image"),
            dict(suite="pipeline"),
            dict(
                measurement_job={
                    "run_id": "12345",
                    "attempt": "2",
                    "runner": "other runner",
                }
            ),
            dict(measurement_job=None),
        ]
        for change in changes:
            (self.base / "environment.json").write_text(original)
            self.change_base(**change)
            with (
                self.subTest(change=change),
                patch.dict(os.environ, self.env, clear=True),
                patch.object(perf.subprocess, "run") as call,
                patch.object(perf, "ensure_name"),
                self.assertRaises(ValueError),
            ):
                perf.upload(self.head, False, paired_base=self.base)
            call.assert_not_called()

    def test_incomplete_base_and_candidate_logs_are_rejected_before_upload(self):
        for directory, number in [(self.base, 3), (self.head, 1)]:
            path = directory / f"criterion-{number}.txt"
            saved = path.read_text()
            path.write_text("")
            with (
                patch.dict(os.environ, self.env, clear=True),
                patch.object(perf.subprocess, "run") as call,
                patch.object(perf, "ensure_name"),
                self.assertRaises(ValueError),
            ):
                perf.upload(self.head, False, paired_base=self.base)
            call.assert_not_called()
            path.write_text(saved)

    def test_pair_does_not_make_missing_boundaries_or_slowdown_a_pass(self):
        for candidate in [report(boundary=False), report(value=1_250_000)]:
            with self.subTest(candidate=candidate), self.assertRaises(ValueError):
                self.upload(candidate)

    def test_pair_is_not_main_bootstrap(self):
        with (
            patch.dict(os.environ, self.env, clear=True),
            self.assertRaises(ValueError),
        ):
            perf.upload(self.head, True, paired_base=self.base)


if __name__ == "__main__":
    unittest.main()
