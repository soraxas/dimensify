"""Real-scene results must retain identity and reject incomparable runs."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import perf_scenes as scenes


def native(value=10_000_000):
    rows = [{"completed_frame_ns": value, "app_update_wall_ns": value / 2,
             "acknowledged_sort_and_draw": True} for _ in range(240)]
    return {"protocol": "dimensify-native-offscreen-v1", "publication_eligible": True,
            "classification": "hardware_performance", "gpu_timestamps_requested": False,
            "environment": {"git_commit": "a" * 40, "git_commit_after": "a" * 40,
                            "git_dirty": False, "git_dirty_after": False,
                            "build_commit": "a" * 40, "build_dirty": "false",
                            "build_opt_level": "3", "debug_assertions": False,
                            "adapter": {"name": "test GPU", "device_type": "DiscreteGpu", "backend": "Vulkan"},
                            "os": "Linux", "architecture": "x86_64", "cpu": "test CPU", "build_rustc": "rustc test",
                            "renderer_setup_ns": 30},
            "source": {"sha256": "b" * 64, "source_prepare_ns": 20, "decode_ns": 40},
            "render": {"physical_resolution": [1280, 720], "settings": {"sort_mode": "Radix"}},
            "cases": [{"name": name, "first_acknowledged_completed_draw_ns": 50,
                       "capture": {"non_background_pixels": 12}, "rows": copy.deepcopy(rows)}
                      for name in ("main-static", "main-moving", "side-static", "side-moving")]}


class SceneResults(unittest.TestCase):
    def test_physical_browser_testbed_uses_target_instead_of_host_inventory(self):
        target = {"kind": "physical-mobile", "id": "phone-one", "model": "Pixel 9", "os": "Android 16", "provider": "local-usb"}
        report = {"metadata": {"hardware": "Pixel 9/Android 16/phone-one", "platform": "darwin", "arch": "arm64", "physicalDevice": target}, "protocol": {}}
        identity = scenes.browser_identity(report, ({"name": "", "backend": "BrowserWebGpu"}, "153.0", {"dpr": 3}, {}))
        name = scenes.testbed(identity)
        self.assertIn("pixel-9", name)
        self.assertIn("android-16", name)
        self.assertNotIn("darwin", name)
        self.assertNotIn("arm64", name)

    def test_profiled_browser_is_diagnostic_and_discards_replay_timings(self):
        from test_perf_profiles import report as profile_report
        report = profile_report()
        report.update(metadata={"workerProfiling": True, "hardware": "host", "platform": "darwin", "arch": "arm64", "commit": "a" * 40, "sourceState": "clean"},
                      protocol={"workerProfiling": "sampled"}, errors=[], performancePublicationEligible=True)
        report["runs"] = [{"round": n, "mode": mode, "adapter": {}, "browser": "Chromium", "browserEnvironment": {}, "final": {"settings": {}}}
                          for n in range(1, 4) for mode in ("cold", "warm")]
        result = scenes.browser_results(report, {"frame-time": {"latency": {"value": 1}}})
        self.assertFalse(result["publication_eligible"])
        self.assertNotIn("frame-time", result["metrics"])
        self.assertTrue(all(name.startswith("browser-worker-profile-") for name in result["metrics"]))
        publication = scenes.diagnostic_publication({"kind": "browser", "testbed": "test", "identity": result["identity"]}, result["metrics"], False)
        self.assertEqual(publication["branch"], "worker-profiles")
        scenes.validate_bmf(result["metrics"])
        for mutation in (lambda r: r["metadata"].update(workerProfiling=False),
                         lambda r: r["protocol"].update(workerProfiling="disabled"),
                         lambda r: r.update(workerProfiles=[])):
            conflicting = copy.deepcopy(report)
            mutation(conflicting)
            with self.assertRaisesRegex(ValueError, "profil"):
                scenes.browser_results(conflicting, {"frame-time": {"latency": {"value": 1}}})
            with patch.object(scenes.subprocess, "run") as child:
                with self.assertRaisesRegex(ValueError, "profil"):
                    scenes.browser_metrics(conflicting)
                child.assert_not_called()

    def test_lod_fixture_binds_entrypoint_not_first_chunk(self):
        data = {"dataset_id": "playbot-lod-v1", "asset_identity": "sha256:" + "b" * 64,
                "format": "sog-v2-hierarchy", "entrypoint_relative": "lod-meta.json",
                "files": [{"path": "0/means.webp", "sha256": "c" * 64}, {"path": "lod-meta.json", "sha256": "d" * 64}]}
        summary = {"kind": "native-lod", "dataset_id": data["dataset_id"], "asset_identity": data["asset_identity"]}
        scenes.bind_fixture(summary, data, [{"source": {"sha256": "d" * 64}}])
        with self.assertRaisesRegex(ValueError, "hierarchy"):
            scenes.bind_fixture(summary, data, [{"source": {"sha256": "c" * 64}}])

    def test_browser_bundle_fixture_binds_entrypoint_not_first_file(self):
        data = {"dataset_id": "scene-v1", "asset_identity": "sha256:" + "b" * 64,
                "format": "sog-v2-bundle", "entrypoint_relative": "scene.sog",
                "files": [{"path": "camera.json", "sha256": "c" * 64},
                          {"path": "scene.sog", "sha256": "d" * 64}]}
        summary = {"kind": "browser", "dataset_id": data["dataset_id"], "asset_identity": data["asset_identity"]}
        scenes.bind_fixture(summary, data, [{"metadata": {"asset": {"identity": "sha256:" + "d" * 64}}}])
        with self.assertRaisesRegex(ValueError, "fixture"):
            scenes.bind_fixture(summary, data, [{"metadata": {"asset": {"identity": "sha256:" + "c" * 64}}}])

    def test_resource_units_and_repeat_identity_are_preserved(self):
        values = [{"frame": 10, "rss": {"process-memory-bytes": {"value": 100}}},
                  {"frame": 20, "rss": {"process-memory-bytes": {"value": 300}}},
                  {"frame": 30, "rss": {"process-memory-bytes": {"value": 200}}}]
        result = scenes.aggregate(values)
        self.assertEqual(result["metrics"]["rss"], {"process-memory-bytes": {"value": 200}})
        self.assertEqual(result["repeat_measures"]["rss"], "process-memory-bytes")
        values[2]["rss"] = {"latency": {"value": 200}}
        with self.assertRaisesRegex(ValueError, "unit"):
            scenes.aggregate(values)

    def test_process_supervision_versions_workload_without_breaking_old_reports(self):
        report = native()
        original = scenes.native_identity(report)
        report["process_usage"] = {"schema": "dimensify-process-usage/1", "method": "wait4", "platform": "linux"}
        instrumented = scenes.native_identity(report)
        config = {"schema": "dimensify-real-scene/2", "dataset_id": "test-v1", "asset_identity": "sha256:" + "f" * 64, "preset": "default"}
        self.assertNotEqual(scenes.benchmark_prefix({**config, "identity": original}),
                            scenes.benchmark_prefix({**config, "identity": instrumented}))

    def test_new_native_suite_can_revalidate_its_saved_workload_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scene.sog").write_bytes(b"test fixture")
            sha = hashlib.sha256(b"test fixture").hexdigest()
            files = [{"path": "scene.sog", "bytes": 12, "sha256": sha}]
            manifest = {"schema": "inlier-gaussian-fixture/1", "dataset_id": "scene-v1", "root": tmp,
                        "entrypoint_relative": "scene.sog", "format": "sog-v2-bundle", "files": files,
                        "asset_identity": "sha256:" + hashlib.sha256(scenes.canonical(files).encode()).hexdigest()}
            (root / "fixture.json").write_text(json.dumps(manifest))
            report = native()
            report["source"]["sha256"] = sha

            def producer(command, _log):
                usage = Path(command[command.index("--output") + 1])
                producer_args = command[command.index("--") + 1:]
                folder = Path(producer_args[producer_args.index("--output") + 1])
                folder.mkdir()
                report["environment"]["executable_sha256"] = "c" * 64
                usage.write_text(json.dumps({"schema": "dimensify-process-usage/1", "exit_code": 0,
                                            "executable_sha256": "c" * 64, "method": "test-wait4", "platform": "linux",
                                            "peak_rss_bytes": 1000, "cpu_user_ns": 100, "cpu_system_ns": 100}))
                (folder / "raw.json").write_text(json.dumps(report))

            args = SimpleNamespace(kind="native", preset="default", fixture=root / "fixture.json",
                                   rounds=3, runner=root / "runner", output=root / "results", camera=None,
                                   gpu_timestamps=False)
            with patch.object(scenes, "execute", side_effect=producer):
                scenes.run(args)
            summary = json.loads((args.output / "summary.json").read_text())
            metrics = json.loads((args.output / "bmf.json").read_text())
            scenes.verify_saved(args.output, summary, metrics)
            self.assertEqual(summary["schema"], "dimensify-real-scene/2")
            self.assertTrue(all(name.startswith("real-scene-v2/") and "/workload-" in name for name in metrics))

    def test_native_lod_run_and_saved_verification_check_capture_files(self):
        from test_perf_lod import report as lod_report
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            shutil.copytree(scenes.ROOT / "crates/splat/tests/fixtures/hosted-stream", assets)
            files = sorted([{"path": str(path.relative_to(assets)), "bytes": path.stat().st_size,
                             "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                            for path in assets.rglob("*") if path.is_file()], key=lambda row: row["path"])
            manifest = {"schema": "inlier-gaussian-fixture/1", "dataset_id": "scene-v1", "root": str(assets),
                        "entrypoint_relative": "lod-meta.json", "format": "sog-v2-hierarchy", "files": files,
                        "asset_identity": "sha256:" + hashlib.sha256(
                            json.dumps(files, sort_keys=True, separators=(",", ":")).encode()).hexdigest()}
            fixture_path = root / "fixture.json"
            fixture_path.write_text(json.dumps(manifest))

            def producer(command, _log):
                usage = Path(command[command.index("--output") + 1])
                producer_args = command[command.index("--") + 1:]
                folder = Path(producer_args[producer_args.index("--output") + 1])
                report = lod_report(folder)
                report["gpu_timestamps_requested"] = False
                report["environment"]["adapter"]["backend"] = "Vulkan"
                report["source"]["sha256"] = hashlib.sha256((assets / "lod-meta.json").read_bytes()).hexdigest()
                usage.write_text(json.dumps({"schema": "dimensify-process-usage/1", "exit_code": 0,
                                            "executable_sha256": "b" * 64, "method": "test-wait4", "platform": "linux",
                                            "peak_rss_bytes": 1000, "cpu_user_ns": 100, "cpu_system_ns": 100}))
                (folder / "raw.json").write_text(json.dumps(report))

            args = SimpleNamespace(kind="native-lod", preset="default", fixture=fixture_path,
                                   rounds=3, runner=root / "runner", output=root / "results", camera=None,
                                   gpu_timestamps=False)
            with patch.object(scenes, "execute", side_effect=producer):
                scenes.run(args)
            summary = json.loads((args.output / "summary.json").read_text())
            metrics = json.loads((args.output / "bmf.json").read_text())
            scenes.verify_saved(args.output, summary, metrics)
            (args.output / "round-2/static.ppm").unlink()
            with self.assertRaisesRegex(ValueError, "capture"):
                scenes.verify_saved(args.output, summary, metrics)

    def test_bencher_names_are_short_unique_and_keep_tail_identity(self):
        prefix = "observed-real-scene-v1/playbot-lod-v1/abcdef012345/default/"
        first = scenes.bencher_name(prefix + "browser-cold-static-near-side-renderer-encoded-interval-p95-v1")
        second = scenes.bencher_name(prefix + "browser-cold-static-near-side-renderer-encoded-interval-p99-v1")
        self.assertLessEqual(len(first), 48)
        self.assertNotEqual(first, second)
        self.assertIn("p95", first)
        self.assertIn("p99", second)

    def test_service_diagnostics_redact_credentials(self):
        from unittest.mock import patch
        with patch.dict("os.environ", {"BENCHER_API_KEY": "bencher_user_example-secret-value"}):
            result = scenes.service_error("HTTP 409: key=bencher_user_example-secret-value Authorization: Bearer another-value")
        self.assertIn("409", result)
        self.assertNotIn("example-secret-value", result)
        self.assertNotIn("another-value", result)

    def test_diagnostic_rejects_software_measured_adapter_under_hardware_metadata(self):
        report = {"metadata": {"sourceState": "clean", "profile": "release", "adapter": {"backend": "BrowserWebGpu", "device_type": "Other"}},
                  "runs": [{"adapter": {"backend": "BrowserWebGpu", "device_type": "Cpu", "name": "SwiftShader"}}]}
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
            scenes.diagnostic_publication({**summary, "kind": "native"}, metrics, check=False)

    def test_explicit_camera_gets_a_distinct_metric_identity(self):
        summary = {"dataset_id": "scene-v1", "asset_identity": "sha256:" + "b" * 64,
                   "preset": "default", "identity": scenes.native_identity(native())}
        automatic = scenes.benchmark_prefix(summary)
        summary["identity"]["render"]["camera_input"] = {"sha256": "c" * 64}
        explicit = scenes.benchmark_prefix(summary)
        self.assertNotEqual(automatic, explicit)
        self.assertIn("camera-cccccccccccc", explicit)

    def test_site_refuses_missing_worker_before_launching_browser(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            report = {"feature": "splat-viewer-perf", "files": {}}
            (root / "build.json").write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "worker|incomplete"):
                scenes.verify_site(root)

    def test_service_response_must_preserve_submitted_value(self):
        report = {"results": [[{"benchmark": {"name": "test"}, "measures": [
            {"measure": {"slug": "latency"}, "metric": {"value": 999}, "boundary": None}]}]], "alerts": []}
        with self.assertRaisesRegex(ValueError, "value"):
            scenes.validate_report(report, {"test": {"latency": {"value": 12}}}, check=False)

    def test_readable_benchmark_name_preserves_submission_through_stable_slug(self):
        report = {"results": [[{"benchmark": {"name": "Apartment moving frame P95", "slug": "rs1-stable-id"},
                  "measures": [{"measure": {"slug": "latency"}, "metric": {"value": 12}, "boundary": None}]}]], "alerts": []}
        metrics = {"rs1-stable-id": {"latency": {"value": 12}}}
        scenes.validate_report(report, metrics, check=False)
        report["results"][0][0]["benchmark"]["slug"] = "different-benchmark"
        with self.assertRaisesRegex(ValueError, "unexpected|Incomplete"):
            scenes.validate_report(report, metrics, check=False)

    def test_service_collapsed_slug_survives_readable_name_without_alias_collisions(self):
        report = {"results": [[{"benchmark": {"name": "Playbot JS heap total", "slug": "obs2-heap-total-id"},
                  "measures": [{"measure": {"slug": "js-heap-bytes"}, "metric": {"value": 123}}]}]], "alerts": []}
        expected = {"obs2-heap--total-id": {"js-heap-bytes": {"value": 123}}}
        scenes.validate_report(report, expected, check=False)
        expected["obs2-heap-total-id"] = {"js-heap-bytes": {"value": 123}}
        with self.assertRaises(ValueError):
            scenes.validate_report(report, expected, check=False)

    def test_saved_fixture_cannot_relabel_measurements(self):
        summary = {"dataset_id": "different-v1", "asset_identity": "sha256:" + "b" * 64, "kind": "native"}
        data = {"dataset_id": "actual-v1", "asset_identity": "sha256:" + "b" * 64, "files": [{"sha256": "b" * 64}]}
        with self.assertRaisesRegex(ValueError, "fixture"):
            scenes.bind_fixture(summary, data, [native()])

    def test_browser_base_metrics_are_recomputed_from_raw(self):
        run = {"mode": "cold", "loadReadyMs": 12, "rendererEncodedMs": 20, "settleMs": 32,
               "appIntervals": [10, 20, 30, 40], "rendererIntervals": [20, 30], "rafIntervals": [16, 17],
               "final": {"phase": "ready", "renderer_encoded_frames": 2, "render_ready_splats": 8, "streaming_busy": False}, "errors": []}
        report = {"metadata": {"sourceState": "clean", "commit": "a" * 40, "profile": "release", "buildIdentity": "sha256:served-wasm",
                               "hardware": "host", "asset": {"identity": "sha256:fixture"}, "browser": "Chromium 1",
                               "viewport": {"width": 1000, "height": 760}, "adapter": {"name": "", "device_type": "Other", "backend": "BrowserWebGpu"}},
                  "performancePublicationEligible": True, "protocol": {"steps": []}, "runs": [run]}
        run["adapter"] = report["metadata"]["adapter"]
        run["browser"] = "Chromium 1"
        result = scenes.browser_metrics(report)
        self.assertEqual(result["metrics"]["browser-cold-app-interval-p99-v1"], {"latency": {"value": 40_000_000}})
        self.assertFalse(result["publication_eligible"])
        run["final"]["renderer_encoded_frames"] = 0
        with self.assertRaises(ValueError):
            scenes.browser_metrics(report)

    def test_browser_reports_observed_cache_peaks_counts_and_elapsed_stage_totals(self):
        runs = []
        for number in range(1, 4):
            for mode in ("cold", "warm"):
                runs.append({"round": number, "mode": mode, "adapter": {"name": "GPU", "device_type": "IntegratedGpu"},
                             "browser": "Chromium 1", "browserEnvironment": {"dpr": 1},
                             "observations": [{"cacheReadyBytes": 100, "cacheReservedBytes": 20, "renderReadySplats": 3},
                                              {"cacheReadyBytes": 300, "cacheReservedBytes": 30, "renderReadySplats": 9}],
                             "final": {"settings": {}, "cpu": {"cpu_stages": {"decode": {"attempts": 2, "total_ms": number, "max_ms": .5}}},
                                       "cache": {"events_total": 7, "dropped_events": 0}}})
        report = {"metadata": {"hardware": "host", "platform": "darwin", "arch": "arm64", "commit": "a" * 40, "sourceState": "clean"},
                  "protocol": {"durationMs": 2000}, "runs": runs, "errors": [], "performancePublicationEligible": True}
        result = scenes.browser_results(report, {"browser-cold-load-ready-v1": {"latency": {"value": 12}}})
        self.assertEqual(result["metrics"]["browser-observed-cache-ready-peak"], {"cache-payload-bytes": {"value": 300}})
        self.assertEqual(result["metrics"]["browser-whole-replay-cpu-decode-total-wall"], {"latency": {"value": 2_000_000}})
        self.assertEqual(result["metrics"]["browser-observed-ready-splats-peak"], {"count": {"value": 9}})
        report["performancePublicationEligible"] = False
        self.assertFalse(scenes.browser_results(report, {"load": {"latency": {"value": 12}}})["publication_eligible"])

    def test_custom_byte_and_count_metrics_allow_measured_zero_but_not_negative(self):
        scenes.validate_bmf({"cache": {"cache-payload-bytes": {"value": 0}}, "evictions": {"count": {"value": 0}}})
        with self.assertRaises(ValueError):
            scenes.validate_bmf({"cache": {"cache-payload-bytes": {"value": -1}}})

    def test_quantiles_keep_tails_separate_and_reject_incomplete_native_work(self):
        self.assertEqual(scenes.distribution([1, 2, 3, 100]), {"median": 2.5, "p95": 100, "p99": 100})
        report = native()
        report["cases"][0]["rows"] = report["cases"][0]["rows"][:4]
        with self.assertRaises(ValueError):
            scenes.native_results([report] * 3)

    def test_constrained_native_preset_cannot_silently_run_default_settings(self):
        # These flags affect the streaming viewer only; accepting them for a
        # whole-file native run would invent a separate, identical history.
        import subprocess
        import sys
        process = subprocess.run([sys.executable, str(Path(scenes.__file__)), "run", "--kind", "native",
                                  "--preset", "constrained", "--fixture", "/missing", "--output", "/missing"],
                                 capture_output=True, text=True)
        self.assertNotEqual(process.returncode, 0)
        self.assertIn("constrained", process.stderr)

    def test_fixture_verifies_bytes_and_complete_asset_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scene.sog").write_bytes(b"abc")
            files = [{"path": "scene.sog", "bytes": 3,
                      "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]
            identity = hashlib.sha256(json.dumps(files, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
            manifest = {"schema": "inlier-gaussian-fixture/1", "dataset_id": "scene-v1",
                        "asset_identity": "sha256:" + identity, "root": str(root),
                        "entrypoint_relative": "scene.sog", "files": files}
            path = root / "fixture.json"
            path.write_text(json.dumps(manifest))
            self.assertEqual(scenes.fixture(path)["entrypoint"], str(root.resolve() / "scene.sog"))
            (root / "scene.sog").write_bytes(b"bad")
            with self.assertRaisesRegex(ValueError, "hash"):
                scenes.fixture(path)

    def test_fixture_rejects_escape_and_wrong_set_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "fixture.json"
            report = {"schema": "inlier-gaussian-fixture/1", "dataset_id": "scene-v1",
                      "root": tmp, "entrypoint_relative": "../escape.sog",
                      "asset_identity": "sha256:" + "0" * 64,
                      "files": [{"path": "../escape.sog", "bytes": 1, "sha256": "0" * 64}]}
            path.write_text(json.dumps(report))
            with self.assertRaises(ValueError):
                scenes.fixture(path)

    def test_hierarchy_fixture_requires_every_chunk_texture_and_environment_dependency(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            hierarchy = root / "scene"
            shutil.copytree(scenes.ROOT / "crates/splat/tests/fixtures/hosted-stream", hierarchy)
            shutil.copytree(hierarchy / "chunks", hierarchy / "environment")
            entrypoint = hierarchy / "lod-meta.json"
            manifest = json.loads(entrypoint.read_text())
            manifest["environment"] = "environment/meta.json"
            entrypoint.write_text(json.dumps(manifest))
            meta_path = hierarchy / "chunks/meta.json"
            meta = json.loads(meta_path.read_text())
            meta["shN"] = {"count": 1, "bands": 1, "files": ["centroids.webp", "labels.webp"]}
            meta_path.write_text(json.dumps(meta))
            for name in ("centroids.webp", "labels.webp"):
                (meta_path.parent / name).write_bytes(b"higher-band dependency")
            files = sorted([{"path": str(path.relative_to(root)), "bytes": path.stat().st_size,
                             "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                            for path in hierarchy.rglob("*") if path.is_file()], key=lambda row: row["path"])
            data = {"schema": "inlier-gaussian-fixture/1", "dataset_id": "scene-v1", "root": tmp,
                    "entrypoint_relative": "scene/lod-meta.json", "format": "sog-v2-hierarchy"}
            fixture_path = root / "fixture.json"

            def save(rows):
                data["files"] = rows
                data["asset_identity"] = "sha256:" + hashlib.sha256(
                    json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
                fixture_path.write_text(json.dumps(data))

            save(files)
            self.assertEqual(scenes.fixture(fixture_path)["entrypoint"], str(entrypoint.resolve()))
            for missing in ("scene/chunks/meta.json", "scene/chunks/means_l.webp",
                            "scene/chunks/centroids.webp", "scene/chunks/labels.webp",
                            "scene/environment/meta.json", "scene/environment/sh0.webp"):
                with self.subTest(missing=missing):
                    save([row for row in files if row["path"] != missing])
                    with self.assertRaisesRegex(ValueError, "dependency"):
                        scenes.fixture(fixture_path)

    def test_native_aggregates_run_percentiles_without_pooling_or_confidence_bounds(self):
        reports = [native(10_000_000), native(20_000_000), native(300_000_000)]
        reports[2]["cases"][0]["rows"][0]["completed_frame_ns"] = 999_000_000
        result = scenes.native_results(reports)
        self.assertEqual(result["metrics"]["main-static/completed-frame/p95"], {"latency": {"value": 20_000_000}})
        self.assertEqual(result["metrics"]["loading/decode-expanded"], {"latency": {"value": 40}})
        self.assertEqual(result["repeat_values"]["main-static/completed-frame/p95"], [10_000_000, 20_000_000, 300_000_000])

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
        self.assertEqual(result["repeat_values"]["loading/first-completed-draw-after-spawn"], [50, 50, 50])

    def test_upload_rebuild_rejects_changed_saved_metrics(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            summary = {"kind": "native", "rounds": 3, "preset": "default", "dataset_id": "scene-v1",
                       "asset_identity": "sha256:" + "b" * 64, "commit": "a" * 40,
                       "identity": scenes.native_identity(native()), "publication_eligible": True}
            summary["testbed"] = scenes.testbed(summary["identity"])
            for number in range(1, 4):
                folder = root / f"round-{number}"
                folder.mkdir()
                (folder / "raw.json").write_text(json.dumps(native()))
            with self.assertRaisesRegex(ValueError, "saved|match"):
                scenes.verify_saved(root, summary, {"invented": {"latency": {"value": 1}}})

    def test_native_rejects_missing_case_failed_draw_and_changed_configuration(self):
        for mutation in (lambda r: r["cases"].pop(),
                         lambda r: r["cases"][0]["rows"][0].update(acknowledged_sort_and_draw=False),
                         lambda r: r["render"].update(physical_resolution=[640, 480]),
                         lambda r: r["source"].update(sha256="c" * 64)):
            reports = [native(), native(), native()]
            mutation(reports[1])
            with self.assertRaises(ValueError):
                scenes.native_results(reports)

    def test_native_rejects_smoke_dirty_software_and_fewer_than_three_processes(self):
        for field, value in (("classification", "smoke_not_performance"), ("publication_eligible", False)):
            report = native()
            report[field] = value
            with self.assertRaises(ValueError):
                scenes.native_results([report] * 3)
        for field, value in (("git_dirty", True), ("build_commit", "c" * 40), ("debug_assertions", True)):
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

    def test_testbed_describes_hardware_while_workload_separates_render_settings(self):
        first = scenes.native_identity(native())
        summary = {"schema": "dimensify-real-scene/2", "dataset_id": "scene-v1",
                   "asset_identity": "sha256:" + "b" * 64, "preset": "default", "identity": first}
        self.assertIn("test-gpu", scenes.testbed(first))
        self.assertIn("vulkan", scenes.testbed(first))
        self.assertNotRegex(scenes.testbed(first), r"[a-f0-9]{16}")
        other_commit = native()
        other_commit["environment"]["git_commit"] = "c" * 40
        self.assertEqual(scenes.testbed(first), scenes.testbed(scenes.native_identity(other_commit)))
        changed = native()
        changed["environment"]["adapter"]["driver_info"] = "new driver"
        self.assertNotEqual(scenes.testbed(first), scenes.testbed(scenes.native_identity(changed)))
        changed = native()
        changed["gpu_timestamps_requested"] = True
        instrumented = copy.deepcopy(scenes.native_identity(changed))
        self.assertEqual(scenes.testbed(first), scenes.testbed(instrumented))
        self.assertNotEqual(scenes.benchmark_prefix(summary), scenes.benchmark_prefix({**summary, "identity": instrumented}))
        changed["render"]["physical_resolution"] = [640, 360]
        smaller = scenes.native_identity(changed)
        self.assertEqual(scenes.testbed(first), scenes.testbed(smaller))
        self.assertNotEqual(scenes.benchmark_prefix({**summary, "identity": instrumented}),
                            scenes.benchmark_prefix({**summary, "identity": smaller}))
        changed["render"]["camera_input"] = {"sha256": "c" * 64}
        self.assertEqual(scenes.testbed(first), scenes.testbed(scenes.native_identity(changed)))

    def test_browser_budget_and_viewport_share_hardware_testbed_but_not_workload(self):
        identity = {"kind": "browser", "adapter": {"name": "", "backend": "BrowserWebGpu",
                    "browser_device": "ANGLE (Apple, ANGLE Metal Renderer: Apple M5 Max, Version 26.7.1)"},
                    "browser": "153.0.8010.12", "hardware": "host/macOS-26.7.1-arm64-arm-64bit",
                    "platform": "darwin", "arch": "arm64", "protocol": {"sampling": "v2"},
                    "settings": {"splat_budget": 4000000}, "browser_environment": {"dpr": 1, "canvas": {"width": 1000, "height": 760}}}
        summary = {"schema": "dimensify-real-scene/2", "dataset_id": "scene-v1",
                   "asset_identity": "sha256:" + "b" * 64, "preset": "default", "identity": identity}
        baseline = scenes.testbed(identity)
        self.assertIn("m5-max", baseline)
        self.assertIn("153-0-8010-12", baseline)
        changed = copy.deepcopy(identity)
        changed["settings"]["splat_budget"] = 150000
        changed["browser_environment"]["canvas"]["width"] = 500
        self.assertEqual(baseline, scenes.testbed(changed))
        self.assertNotEqual(scenes.benchmark_prefix(summary), scenes.benchmark_prefix({**summary, "identity": changed}))
        changed["browser"] = "154.0.1.2"
        self.assertNotEqual(baseline, scenes.testbed(changed))

    def test_bmf_rejects_nan_empty_and_unexpected_measures(self):
        for data in ({}, {"case": {"latency": {"value": float("nan")}}},
                     {"case": {"throughput": {"value": 1}}}):
            with self.assertRaises(ValueError):
                scenes.validate_bmf(data)

    def test_upload_report_requires_exact_metrics_and_existing_boundary_in_check_mode(self):
        report = {"results": [[{"benchmark": {"name": "real-scene-v1/a/loading"},
                                "measures": [{"measure": {"slug": "latency"},
                                              "metric": {"value": 10}, "boundary": None}]}]], "alerts": []}
        scenes.validate_report(report, {"real-scene-v1/a/loading"}, check=False)
        with self.assertRaisesRegex(ValueError, "baseline"):
            scenes.validate_report(report, {"real-scene-v1/a/loading"}, check=True)
        with self.assertRaises(ValueError):
            scenes.validate_report(report, {"other"}, check=False)


if __name__ == "__main__":
    unittest.main()
