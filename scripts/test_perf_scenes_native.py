"""Native real-scene results must retain identity and reject incomparable runs."""

import copy
import hashlib
import json
import unittest

import perf_scenes as scenes


def native(value=10_000_000):
    rows = [
        {
            "completed_frame_ns": value,
            "app_update_wall_ns": value / 2,
            "sample_id": index + 1,
            "sort_encoded": 1,
            "draw_encoded": 1,
            "acknowledged_sort_and_draw": True,
        }
        for index in range(240)
    ]
    return {
        "protocol": "dimensify-native-offscreen-v1",
        "publication_eligible": True,
        "classification": "hardware_performance",
        "gpu_timestamps_requested": False,
        "environment": {
            "git_commit": "a" * 40,
            "git_commit_after": "a" * 40,
            "git_dirty": False,
            "git_dirty_after": False,
            "build_commit": "a" * 40,
            "build_dirty": "false",
            "build_opt_level": "3",
            "debug_assertions": False,
            "adapter": {
                "name": "test GPU",
                "device_type": "DiscreteGpu",
                "backend": "Vulkan",
            },
            "os": "Linux",
            "architecture": "x86_64",
            "cpu": "test CPU",
            "build_rustc": "rustc test",
            "renderer_setup_ns": 30,
        },
        "source": {"sha256": "b" * 64, "source_prepare_ns": 20, "decode_ns": 40},
        "render": {"physical_resolution": [4, 3], "settings": {"sort_mode": "Radix"}},
        "cases": [
            {
                "name": name,
                "first_acknowledged_completed_draw_ns": 50,
                "capture": {
                    "file": name + ".ppm",
                    "non_background_pixels": 12,
                    "rgba_sha256": hashlib.sha256(
                        bytes([255, 0, 0, 255] * 12)
                    ).hexdigest(),
                },
                "rows": copy.deepcopy(rows),
            }
            for name in ("main-static", "main-moving", "side-static", "side-moving")
        ],
    }


def save_native(directory, report):
    directory.mkdir(parents=True, exist_ok=True)
    for case in report["cases"]:
        (directory / case["capture"]["file"]).write_bytes(
            b"P6\n4 3\n255\n" + bytes([255, 0, 0] * 12)
        )
    (directory / "raw.json").write_text(json.dumps(report))


class NativeSceneResults(unittest.TestCase):
    def test_native_rejects_contradictory_completion_and_draw_evidence(self):
        for fields in (
            {"app_update_wall_ns": 11_000_000},
            {"sort_encoded": 0},
            {"draw_encoded": 0},
            {"sort_encoded": True},
            {"sample_id": True},
            {"sort_encoded": 999, "draw_encoded": 2},
            {"draw_encoded": 2},
            {"sort_encoded": 2, "draw_encoded": 2},
        ):
            with self.subTest(fields=fields):
                reports = [native() for _ in range(3)]
                reports[1]["cases"][0]["rows"][0].update(fields)
                with self.assertRaisesRegex(ValueError, "frame|draw|sort|completion"):
                    scenes.native_results(reports)

    def test_native_rejects_a_duplicated_frame_instead_of_counting_it_as_work(self):
        reports = [native() for _ in range(3)]
        rows = reports[1]["cases"][0]["rows"]
        rows[1] = copy.deepcopy(rows[0])
        with self.assertRaisesRegex(ValueError, "frame|ID"):
            scenes.native_results(reports)

    def test_native_radix_samples_must_belong_to_the_originating_frame(self):
        reports = [native() for _ in range(3)]
        for report in reports:
            report["gpu_timestamps_requested"] = True
            for case in report["cases"]:
                for row in case["rows"]:
                    row["gpu_radix_sample"] = {
                        "identity": {"selection": str(row["sample_id"])},
                        "measurement": {"duration_ms": 1, "failure": None},
                    }
        result = scenes.native_results(reports)
        self.assertEqual(
            result["metrics"]["main-static/gpu-radix-and-compaction/median"],
            {"latency": {"value": 1_000_000}},
        )
        reports[1]["cases"][0]["rows"][0]["gpu_radix_sample"]["identity"][
            "selection"
        ] = "9999"
        with self.assertRaisesRegex(ValueError, "frame"):
            scenes.native_results(reports)

    def test_failed_radix_timestamp_cannot_publish_a_positive_duration(self):
        reports = [native() for _ in range(3)]
        for report in reports:
            report["gpu_timestamps_requested"] = True
            for case in report["cases"]:
                for row in case["rows"]:
                    row["gpu_radix_sample"] = {
                        "identity": {"selection": str(row["sample_id"])},
                        "measurement": {"duration_ms": 1, "failure": "query failed"},
                    }
        with self.assertRaisesRegex(ValueError, "failed|failure|timestamp"):
            scenes.native_results(reports)
        for report in reports:
            for case in report["cases"]:
                for row in case["rows"]:
                    row["gpu_radix_sample"]["measurement"]["duration_ms"] = None
        result = scenes.native_results(reports)
        self.assertNotIn(
            "main-static/gpu-radix-and-compaction/median", result["metrics"]
        )
        self.assertIn("main-static/completed-frame/median", result["metrics"])

    def test_resource_units_and_repeat_identity_are_preserved(self):
        values = [
            {"frame": 10, "rss": {"process-memory-bytes": {"value": 100}}},
            {"frame": 20, "rss": {"process-memory-bytes": {"value": 300}}},
            {"frame": 30, "rss": {"process-memory-bytes": {"value": 200}}},
        ]
        result = scenes.aggregate(values)
        self.assertEqual(
            result["metrics"]["rss"], {"process-memory-bytes": {"value": 200}}
        )
        self.assertEqual(result["repeat_measures"]["rss"], "process-memory-bytes")
        values[2]["rss"] = {"latency": {"value": 200}}
        with self.assertRaisesRegex(ValueError, "unit"):
            scenes.aggregate(values)

    def test_default_transparency_metadata_preserves_existing_native_history(self):
        report = native()
        old = copy.deepcopy(scenes.native_identity(report))
        report["render"].update(
            transparency="sorted", stochastic_seed=0, stochastic_sample=0
        )
        self.assertEqual(scenes.native_identity(report), old)
        report["render"]["transparency"] = "stochastic"
        self.assertNotEqual(scenes.native_identity(report), old)

    def test_quantiles_keep_tails_separate_and_reject_incomplete_native_work(self):
        self.assertEqual(
            scenes.distribution([1, 2, 3, 100]), {"median": 2.5, "p95": 100, "p99": 100}
        )
        report = native()
        report["cases"][0]["rows"] = report["cases"][0]["rows"][:4]
        with self.assertRaises(ValueError):
            scenes.native_results([report] * 3)

    def test_native_aggregates_run_percentiles_without_pooling_or_confidence_bounds(
        self,
    ):
        reports = [native(10_000_000), native(20_000_000), native(300_000_000)]
        reports[2]["cases"][0]["rows"][0]["completed_frame_ns"] = 999_000_000
        result = scenes.native_results(reports)
        self.assertEqual(
            result["metrics"]["main-static/completed-frame/p95"],
            {"latency": {"value": 20_000_000}},
        )
        self.assertEqual(
            result["metrics"]["loading/decode-expanded"], {"latency": {"value": 40}}
        )
        self.assertEqual(
            result["repeat_values"]["main-static/completed-frame/p95"],
            [10_000_000, 20_000_000, 300_000_000],
        )

    def test_native_rejects_short_completed_work(self):
        short = native(1)
        with self.assertRaises(ValueError):
            scenes.native_results([short] * 3)

    def test_native_repeats_require_the_same_executable(self):
        reports = [native() for _ in range(3)]
        for report in reports:
            report["environment"]["executable_sha256"] = "c" * 64
        scenes.native_results(reports)
        reports[1]["environment"]["executable_sha256"] = "d" * 64
        with self.assertRaisesRegex(ValueError, "Incomparable"):
            scenes.native_results(reports)

    def test_native_first_draw_uses_main_static_when_cases_are_reordered(self):
        reports = [native() for _ in range(3)]
        for report in reports:
            for index, case in enumerate(report["cases"]):
                case["first_acknowledged_completed_draw_ns"] = 50 + index * 100
            report["cases"].reverse()
        result = scenes.native_results(reports)
        self.assertEqual(
            result["repeat_values"]["loading/first-completed-draw-after-spawn"],
            [50, 50, 50],
        )

    def test_native_rejects_missing_case_failed_draw_and_changed_configuration(self):
        for mutation in (
            lambda r: r["cases"].pop(),
            lambda r: r["cases"][0]["rows"][0].update(acknowledged_sort_and_draw=False),
            lambda r: r["render"].update(physical_resolution=[640, 480]),
            lambda r: r["source"].update(sha256="c" * 64),
        ):
            reports = [native(), native(), native()]
            mutation(reports[1])
            with self.assertRaises(ValueError):
                scenes.native_results(reports)

    def test_native_rejects_smoke_dirty_software_and_fewer_than_three_processes(self):
        for field, value in (
            ("classification", "smoke_not_performance"),
            ("publication_eligible", False),
        ):
            report = native()
            report[field] = value
            with self.assertRaises(ValueError):
                scenes.native_results([report] * 3)
        for field, value in (
            ("git_dirty", True),
            ("build_commit", "c" * 40),
            ("debug_assertions", True),
        ):
            report = native()
            report["environment"][field] = value
            with self.assertRaises(ValueError):
                scenes.native_results([report] * 3)
        with self.assertRaises(ValueError):
            scenes.native_results([native()])
        report = native()
        report["environment"]["adapter"]["device_type"] = "Cpu"
        with self.assertRaises(ValueError):
            scenes.native_results([report] * 3)


if __name__ == "__main__":
    unittest.main()
