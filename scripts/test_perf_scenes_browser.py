"""Browser real-scene results must be rebuilt from raw replay samples."""

import copy
import unittest
from unittest.mock import patch

import perf_scenes as scenes


class BrowserSceneResults(unittest.TestCase):
    def test_whole_file_metrics_preserve_missing_and_zero_elapsed_observations(self):
        from perf_scenes_browser import whole_file_metrics
        warm = [{"final": {"whole_file": {"stages": {
            "texture_decode": {"attempts": 4, "total_ms": n, "max_ms": n / 2},
            "allocation": {"attempts": 1, "total_ms": 0, "max_ms": 0},
            "ply_decode": {"attempts": 0, "total_ms": None, "max_ms": None},
        }}}} for n in (1, 2, 9)]
        result = whole_file_metrics(warm)
        self.assertEqual(result["browser-whole-file-texture-decode-total-wall"], {"latency": {"value": 2_000_000}})
        self.assertEqual(result["browser-whole-file-allocation-attempts"], {"count": {"value": 1}})
        # A quantized zero is not published as a positive latency; its attempt survives.
        self.assertNotIn("browser-whole-file-allocation-total-wall", result)
        self.assertFalse(any("ply-decode" in name or "gpu-upload" in name for name in result))
        warm[0]["final"].pop("whole_file")
        self.assertEqual(whole_file_metrics(warm), {})

    def test_whole_file_metrics_reject_malformed_present_attempts(self):
        from perf_scenes_browser import whole_file_metrics
        for value in (None, True, -1, 1.5, "1"):
            with self.subTest(attempts=value):
                warm = [{"final": {"whole_file": {"stages": {
                    "allocation": {"attempts": value, "total_ms": 1, "max_ms": 1}
                }}}}]
                with self.assertRaisesRegex(ValueError, "attempts"):
                    whole_file_metrics(warm)
        with self.assertRaisesRegex(ValueError, "attempts"):
            whole_file_metrics([{"final": {"whole_file": {"stages": {"allocation": {}}}}}])
        self.assertEqual(whole_file_metrics([{"final": {"whole_file": {"stages": None}}}]), {})

    def test_whole_file_metrics_publish_positive_median_with_quantized_round(self):
        from perf_scenes_browser import whole_file_metrics
        warm = [{"final": {"whole_file": {"stages": {
            "allocation": {"attempts": 1, "total_ms": n, "max_ms": n}
        }}}} for n in (0, 2, 3)]
        result = whole_file_metrics(warm)
        self.assertEqual(result["browser-whole-file-allocation-total-wall"], {"latency": {"value": 2_000_000}})

    def test_physical_browser_testbed_uses_target_instead_of_host_inventory(self):
        target = {
            "kind": "physical-mobile",
            "id": "phone-one",
            "model": "Pixel 9",
            "os": "Android 16",
            "provider": "local-usb",
        }
        report = {
            "metadata": {
                "hardware": "Pixel 9/Android 16/phone-one",
                "platform": "darwin",
                "arch": "arm64",
                "physicalDevice": target,
            },
            "protocol": {},
        }
        identity = scenes.browser_identity(
            report, ({"name": "", "backend": "BrowserWebGpu"}, "153.0", {"dpr": 3}, {})
        )
        name = scenes.testbed(identity)
        self.assertIn("pixel-9", name)
        self.assertIn("android-16", name)
        self.assertNotIn("darwin", name)
        self.assertNotIn("arm64", name)

    def test_profiled_browser_is_diagnostic_and_discards_replay_timings(self):
        from test_perf_profiles import report as profile_report

        report = profile_report()
        report.update(
            metadata={
                "workerProfiling": True,
                "hardware": "host",
                "platform": "darwin",
                "arch": "arm64",
                "commit": "a" * 40,
                "sourceState": "clean",
            },
            protocol={"workerProfiling": "sampled"},
            errors=[],
            performancePublicationEligible=True,
        )
        report["runs"] = [
            {
                "round": n,
                "mode": mode,
                "adapter": {},
                "browser": "Chromium",
                "browserEnvironment": {},
                "final": {"settings": {}},
            }
            for n in range(1, 4)
            for mode in ("cold", "warm")
        ]
        result = scenes.browser_results(
            report, {"frame-time": {"latency": {"value": 1}}}
        )
        self.assertFalse(result["publication_eligible"])
        self.assertNotIn("frame-time", result["metrics"])
        self.assertTrue(
            all(
                name.startswith("browser-worker-profile-") for name in result["metrics"]
            )
        )
        publication = scenes.diagnostic_publication(
            {"kind": "browser", "testbed": "test", "identity": result["identity"]},
            result["metrics"],
            False,
        )
        self.assertEqual(publication["branch"], "worker-profiles")
        scenes.validate_bmf(result["metrics"])
        for mutation in (
            lambda r: r["metadata"].update(workerProfiling=False),
            lambda r: r["protocol"].update(workerProfiling="disabled"),
            lambda r: r.update(workerProfiles=[]),
        ):
            conflicting = copy.deepcopy(report)
            mutation(conflicting)
            with self.assertRaisesRegex(ValueError, "profil"):
                scenes.browser_results(
                    conflicting, {"frame-time": {"latency": {"value": 1}}}
                )
            with patch.object(scenes.subprocess, "run") as child:
                with self.assertRaisesRegex(ValueError, "profil"):
                    scenes.browser_metrics(conflicting)
                child.assert_not_called()

    def test_browser_base_metrics_are_recomputed_from_raw(self):
        run = {
            "mode": "cold",
            "loadReadyMs": 12,
            "rendererEncodedMs": 20,
            "settleMs": 32,
            "appIntervals": [10, 20, 30, 40],
            "rendererIntervals": [20, 30],
            "rafIntervals": [16, 17],
            "final": {
                "phase": "ready",
                "renderer_encoded_frames": 2,
                "render_ready_splats": 8,
                "streaming_busy": False,
            },
            "errors": [],
        }
        report = {
            "metadata": {
                "sourceState": "clean",
                "commit": "a" * 40,
                "profile": "release",
                "buildIdentity": "sha256:served-wasm",
                "hardware": "host",
                "asset": {"identity": "sha256:fixture"},
                "browser": "Chromium 1",
                "viewport": {"width": 1000, "height": 760},
                "adapter": {
                    "name": "",
                    "device_type": "Other",
                    "backend": "BrowserWebGpu",
                },
            },
            "performancePublicationEligible": True,
            "protocol": {"steps": []},
            "runs": [run],
        }
        run["adapter"] = report["metadata"]["adapter"]
        run["browser"] = "Chromium 1"
        result = scenes.browser_metrics(report)
        self.assertEqual(
            result["metrics"]["browser-cold-app-interval-p99-v1"],
            {"latency": {"value": 40_000_000}},
        )
        self.assertFalse(result["publication_eligible"])
        report["protocol"]["cameraReplay"] = "unknown"
        with self.assertRaises(ValueError):
            scenes.browser_metrics(report)
        del report["protocol"]["cameraReplay"]
        report["cameraReference"] = {"schema": "dimensify-camera-reference/1"}
        with self.assertRaises(ValueError):
            scenes.browser_metrics(report)
        del report["cameraReference"]
        run["final"]["renderer_encoded_frames"] = 0
        with self.assertRaises(ValueError):
            scenes.browser_metrics(report)

    def test_camera_recording_discards_metrics_even_with_eligible_metadata(self):
        report = {
            "metadata": {
                "asset": {"identity": "sha256:fixture"},
                "viewport": {"width": 1000, "height": 760},
                "browserEnvironment": {
                    "dpr": 1,
                    "canvas": {"width": 1000, "height": 760},
                },
            },
            "protocol": {
                "durationMs": 500,
                "steps": [{"name": "static", "action": "hold"}],
                "cameraReplay": "input-checkpoints-v1",
            },
            "runs": [
                {
                    "mode": mode,
                    "cases": [
                        {
                            "name": "static",
                            "began": 0,
                            "ended": 1000,
                            "settings": {},
                            "cameraCheckpoints": [
                                {
                                    "label": label,
                                    "observedAtMs": timestamp,
                                    "camera": {
                                        "position": [0, 0, 3],
                                        "rotation_xyzw": [0, 0, 0, 1],
                                        "fov_radians": 0.8,
                                        "near": 0.01,
                                        "far": 100,
                                    },
                                }
                                for label, timestamp in (
                                    ("before", 100),
                                    ("after", 900),
                                )
                            ],
                        }
                    ],
                }
                for mode in ("cold", "warm")
            ],
        }
        report["metadata"].update(
            sourceState="clean",
            commit="a" * 40,
            profile="release",
            buildIdentity="sha256:served",
            hardware="host",
            browser="Chromium 1",
            platform="darwin",
            arch="arm64",
            adapter={"name": "GPU", "device_type": "IntegratedGpu", "backend": "Metal"},
        )
        report.update(errors=[], performancePublicationEligible=True)
        first_pair = report["runs"]
        report["runs"] = []
        for number in range(1, 4):
            for seed in first_pair:
                run = copy.deepcopy(seed)
                run.update(
                    round=number,
                    adapter=report["metadata"]["adapter"],
                    browser="Chromium 1",
                    browserEnvironment=report["metadata"]["browserEnvironment"],
                    loadReadyMs=1,
                    rendererEncodedMs=2,
                    settleMs=3,
                    appIntervals=[10],
                    rendererIntervals=[10],
                    rafIntervals=[10],
                    errors=[],
                    final={
                        "phase": "ready",
                        "renderer_encoded_frames": 1,
                        "render_ready_splats": 1,
                        "streaming_busy": False,
                        "settings": {},
                    },
                )
                for case in run["cases"]:
                    case.update(
                        appIntervals=[10], rendererIntervals=[10], rafIntervals=[10]
                    )
                report["runs"].append(run)
        rebuilt = scenes.browser_metrics(report)
        self.assertFalse(rebuilt["publication_eligible"])
        self.assertEqual(rebuilt["metrics"], {})
        result = scenes.browser_results(report, {"forged": {"latency": {"value": 1}}})
        self.assertFalse(result["publication_eligible"])
        self.assertEqual(result["metrics"], {})

    def test_browser_reports_observed_cache_peaks_counts_and_elapsed_stage_totals(self):
        runs = []
        for number in range(1, 4):
            for mode in ("cold", "warm"):
                runs.append(
                    {
                        "round": number,
                        "mode": mode,
                        "adapter": {"name": "GPU", "device_type": "IntegratedGpu"},
                        "browser": "Chromium 1",
                        "browserEnvironment": {"dpr": 1},
                        "observations": [
                            {
                                "cacheReadyBytes": 100,
                                "cacheReservedBytes": 20,
                                "renderReadySplats": 3,
                            },
                            {
                                "cacheReadyBytes": 300,
                                "cacheReservedBytes": 30,
                                "renderReadySplats": 9,
                            },
                        ],
                        "final": {
                            "settings": {},
                            "cpu": {
                                "cpu_stages": {
                                    "decode": {
                                        "attempts": 2,
                                        "total_ms": number,
                                        "max_ms": 0.5,
                                    }
                                }
                            },
                            "cache": {"events_total": 7, "dropped_events": 0},
                        },
                    }
                )
        report = {
            "metadata": {
                "hardware": "host",
                "platform": "darwin",
                "arch": "arm64",
                "commit": "a" * 40,
                "sourceState": "clean",
            },
            "protocol": {"durationMs": 2000},
            "runs": runs,
            "errors": [],
            "performancePublicationEligible": True,
        }
        result = scenes.browser_results(
            report, {"browser-cold-load-ready-v1": {"latency": {"value": 12}}}
        )
        self.assertEqual(
            result["metrics"]["browser-observed-cache-ready-peak"],
            {"cache-payload-bytes": {"value": 300}},
        )
        self.assertEqual(
            result["metrics"]["browser-whole-replay-cpu-decode-total-wall"],
            {"latency": {"value": 2_000_000}},
        )
        self.assertEqual(
            result["metrics"]["browser-observed-ready-splats-peak"],
            {"count": {"value": 9}},
        )
        report["performancePublicationEligible"] = False
        self.assertFalse(
            scenes.browser_results(report, {"load": {"latency": {"value": 12}}})[
                "publication_eligible"
            ]
        )


if __name__ == "__main__":
    unittest.main()
