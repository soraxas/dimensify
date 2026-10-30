"""Bencher names, testbeds and service responses must keep history identity."""

import copy
import unittest

import perf_scenes as scenes
from test_perf_scenes_native import native


class ScenePublication(unittest.TestCase):
    def test_process_supervision_versions_workload_without_breaking_old_reports(self):
        report = native()
        original = scenes.native_identity(report)
        report["process_usage"] = {
            "schema": "dimensify-process-usage/1",
            "method": "wait4",
            "platform": "linux",
        }
        instrumented = scenes.native_identity(report)
        config = {
            "schema": "dimensify-real-scene/2",
            "dataset_id": "test-v1",
            "asset_identity": "sha256:" + "f" * 64,
            "preset": "default",
        }
        self.assertNotEqual(
            scenes.benchmark_prefix({**config, "identity": original}),
            scenes.benchmark_prefix({**config, "identity": instrumented}),
        )

    def test_explicit_camera_gets_a_distinct_metric_identity(self):
        summary = {
            "dataset_id": "scene-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "preset": "default",
            "identity": scenes.native_identity(native()),
        }
        automatic = scenes.benchmark_prefix(summary)
        summary["identity"]["render"]["camera_input"] = {"sha256": "c" * 64}
        explicit = scenes.benchmark_prefix(summary)
        self.assertNotEqual(automatic, explicit)
        self.assertIn("camera-cccccccccccc", explicit)

    def test_testbed_describes_hardware_while_workload_separates_render_settings(self):
        first = scenes.native_identity(native())
        summary = {
            "schema": "dimensify-real-scene/2",
            "dataset_id": "scene-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "preset": "default",
            "identity": first,
        }
        self.assertIn("test-gpu", scenes.testbed(first))
        self.assertIn("vulkan", scenes.testbed(first))
        self.assertNotRegex(scenes.testbed(first), r"[a-f0-9]{16}")
        other_commit = native()
        other_commit["environment"]["git_commit"] = "c" * 40
        self.assertEqual(
            scenes.testbed(first), scenes.testbed(scenes.native_identity(other_commit))
        )
        changed = native()
        changed["environment"]["adapter"]["driver_info"] = "new driver"
        self.assertNotEqual(
            scenes.testbed(first), scenes.testbed(scenes.native_identity(changed))
        )
        changed = native()
        changed["gpu_timestamps_requested"] = True
        instrumented = copy.deepcopy(scenes.native_identity(changed))
        self.assertEqual(scenes.testbed(first), scenes.testbed(instrumented))
        self.assertNotEqual(
            scenes.benchmark_prefix(summary),
            scenes.benchmark_prefix({**summary, "identity": instrumented}),
        )
        changed["render"]["physical_resolution"] = [640, 360]
        smaller = scenes.native_identity(changed)
        self.assertEqual(scenes.testbed(first), scenes.testbed(smaller))
        self.assertNotEqual(
            scenes.benchmark_prefix({**summary, "identity": instrumented}),
            scenes.benchmark_prefix({**summary, "identity": smaller}),
        )
        changed["render"]["camera_input"] = {"sha256": "c" * 64}
        self.assertEqual(
            scenes.testbed(first), scenes.testbed(scenes.native_identity(changed))
        )

    def test_browser_budget_and_viewport_share_hardware_testbed_but_not_workload(self):
        identity = {
            "kind": "browser",
            "adapter": {
                "name": "",
                "backend": "BrowserWebGpu",
                "browser_device": "ANGLE (Apple, ANGLE Metal Renderer: Apple M5 Max, Version 26.7.1)",
            },
            "browser": "153.0.8010.12",
            "hardware": "host/macOS-26.7.1-arm64-arm-64bit",
            "platform": "darwin",
            "arch": "arm64",
            "protocol": {"sampling": "v2"},
            "settings": {"splat_budget": 4000000},
            "browser_environment": {"dpr": 1, "canvas": {"width": 1000, "height": 760}},
        }
        summary = {
            "schema": "dimensify-real-scene/2",
            "dataset_id": "scene-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "preset": "default",
            "identity": identity,
        }
        baseline = scenes.testbed(identity)
        self.assertIn("m5-max", baseline)
        self.assertIn("153-0-8010-12", baseline)
        changed = copy.deepcopy(identity)
        changed["settings"]["splat_budget"] = 150000
        changed["browser_environment"]["canvas"]["width"] = 500
        self.assertEqual(baseline, scenes.testbed(changed))
        self.assertNotEqual(
            scenes.benchmark_prefix(summary),
            scenes.benchmark_prefix({**summary, "identity": changed}),
        )
        changed["browser"] = "154.0.1.2"
        self.assertNotEqual(baseline, scenes.testbed(changed))

    def test_bencher_names_are_short_unique_and_keep_tail_identity(self):
        prefix = "observed-real-scene-v1/playbot-lod-v1/abcdef012345/default/"
        first = scenes.bencher_name(
            prefix + "browser-cold-static-near-side-renderer-encoded-interval-p95-v1"
        )
        second = scenes.bencher_name(
            prefix + "browser-cold-static-near-side-renderer-encoded-interval-p99-v1"
        )
        self.assertLessEqual(len(first), 48)
        self.assertNotEqual(first, second)
        self.assertIn("p95", first)
        self.assertIn("p99", second)

    def test_service_diagnostics_redact_credentials(self):
        from unittest.mock import patch

        with patch.dict(
            "os.environ", {"BENCHER_API_KEY": "bencher_user_example-secret-value"}
        ):
            result = scenes.service_error(
                "HTTP 409: key=bencher_user_example-secret-value Authorization: Bearer another-value"
            )
        self.assertIn("409", result)
        self.assertNotIn("example-secret-value", result)
        self.assertNotIn("another-value", result)

    def test_diagnostic_rejects_software_measured_adapter_under_hardware_metadata(self):
        report = {
            "metadata": {
                "sourceState": "clean",
                "profile": "release",
                "adapter": {"backend": "BrowserWebGpu", "device_type": "Other"},
            },
            "runs": [
                {
                    "adapter": {
                        "backend": "BrowserWebGpu",
                        "device_type": "Cpu",
                        "name": "SwiftShader",
                    }
                }
            ],
        }
        with self.assertRaises(ValueError):
            scenes.validate_diagnostic(report)

    def test_diagnostic_history_is_separate_and_never_a_regression_gate(self):
        summary = {"kind": "browser", "testbed": "real-browser-v1-abc"}
        metrics = {"real-scene-v1/scene/load": {"latency": {"value": 12}}}
        publication = scenes.diagnostic_publication(summary, metrics, check=False)
        self.assertEqual(publication["branch"], "browser-diagnostics")
        self.assertEqual(publication["testbed"], "observed-browser-v1-abc")
        self.assertIn("observed-real-scene-v1/scene/load", publication["metrics"])
        with self.assertRaises(ValueError):
            scenes.diagnostic_publication(summary, metrics, check=True)
        with self.assertRaises(ValueError):
            scenes.diagnostic_publication(
                {**summary, "kind": "native"}, metrics, check=False
            )

    def test_service_response_must_preserve_submitted_value(self):
        report = {
            "results": [
                [
                    {
                        "benchmark": {"name": "test"},
                        "measures": [
                            {
                                "measure": {"slug": "latency"},
                                "metric": {"value": 999},
                                "boundary": None,
                            }
                        ],
                    }
                ]
            ],
            "alerts": [],
        }
        with self.assertRaisesRegex(ValueError, "value"):
            scenes.validate_report(
                report, {"test": {"latency": {"value": 12}}}, check=False
            )

    def test_readable_benchmark_name_preserves_submission_through_stable_slug(self):
        report = {
            "results": [
                [
                    {
                        "benchmark": {
                            "name": "Apartment moving frame P95",
                            "slug": "rs1-stable-id",
                        },
                        "measures": [
                            {
                                "measure": {"slug": "latency"},
                                "metric": {"value": 12},
                                "boundary": None,
                            }
                        ],
                    }
                ]
            ],
            "alerts": [],
        }
        metrics = {"rs1-stable-id": {"latency": {"value": 12}}}
        scenes.validate_report(report, metrics, check=False)
        report["results"][0][0]["benchmark"]["slug"] = "different-benchmark"
        with self.assertRaisesRegex(ValueError, "unexpected|Incomplete"):
            scenes.validate_report(report, metrics, check=False)

    def test_service_collapsed_slug_survives_readable_name_without_alias_collisions(
        self,
    ):
        report = {
            "results": [
                [
                    {
                        "benchmark": {
                            "name": "Playbot JS heap total",
                            "slug": "obs2-heap-total-id",
                        },
                        "measures": [
                            {
                                "measure": {"slug": "js-heap-bytes"},
                                "metric": {"value": 123},
                            }
                        ],
                    }
                ]
            ],
            "alerts": [],
        }
        expected = {"obs2-heap--total-id": {"js-heap-bytes": {"value": 123}}}
        scenes.validate_report(report, expected, check=False)
        expected["obs2-heap-total-id"] = {"js-heap-bytes": {"value": 123}}
        with self.assertRaises(ValueError):
            scenes.validate_report(report, expected, check=False)

    def test_upload_report_requires_exact_metrics_and_existing_boundary_in_check_mode(
        self,
    ):
        report = {
            "results": [
                [
                    {
                        "benchmark": {"name": "real-scene-v1/a/loading"},
                        "measures": [
                            {
                                "measure": {"slug": "latency"},
                                "metric": {"value": 10},
                                "boundary": None,
                            }
                        ],
                    }
                ]
            ],
            "alerts": [],
        }
        scenes.validate_report(report, {"real-scene-v1/a/loading"}, check=False)
        with self.assertRaisesRegex(ValueError, "baseline"):
            scenes.validate_report(report, {"real-scene-v1/a/loading"}, check=True)
        with self.assertRaises(ValueError):
            scenes.validate_report(report, {"other"}, check=False)


if __name__ == "__main__":
    unittest.main()
