"""Real-scene runs and saved results must reproduce from raw evidence."""

import hashlib
import json
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import perf_scenes as scenes
from test_perf_scenes_native import native, save_native


class SceneResults(unittest.TestCase):
    def saved_native(self, root):
        reports = [native(10_000_000), native(20_000_000), native(30_000_000)]
        for number, report in enumerate(reports, 1):
            save_native(root / f"round-{number}", report)
        result = scenes.native_results(reports)
        summary = {
            **result,
            "schema": "dimensify-real-scene/2",
            "kind": "native",
            "rounds": 3,
            "preset": "default",
            "dataset_id": "scene-v1",
            "asset_identity": "sha256:" + "b" * 64,
        }
        metrics = {
            scenes.benchmark_prefix(summary) + "/" + name: metric
            for name, metric in result["metrics"].items()
        }
        return summary, metrics

    def test_saved_variance_evidence_cannot_be_changed_while_preserving_the_median(
        self,
    ):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            summary, metrics = self.saved_native(root)
            scenes.verify_saved(root, summary, metrics)
            summary["repeat_values"]["main-static/completed-frame/median"] = [
                1,
                20_000_000,
                900_000_000,
            ]
            with self.assertRaisesRegex(ValueError, "repeat|match"):
                scenes.verify_saved(root, summary, metrics)

    def test_saved_repeat_units_cannot_be_invented(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            summary, metrics = self.saved_native(root)
            summary["repeat_measures"] = {"main-static/completed-frame/median": "count"}
            with self.assertRaisesRegex(ValueError, "repeat|match"):
                scenes.verify_saved(root, summary, metrics)

    def test_saved_variance_evidence_cannot_be_omitted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            summary, metrics = self.saved_native(root)
            del summary["repeat_values"]
            with self.assertRaisesRegex(ValueError, "repeat|match"):
                scenes.verify_saved(root, summary, metrics)

    def test_flat_saved_captures_require_matching_nonblank_bytes(self):
        for damage in (
            "missing",
            "pixels",
            "blank",
            "count",
            "dimensions",
            "truncated",
            "unsafe",
            "symlink",
        ):
            with self.subTest(damage=damage), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                summary, metrics = self.saved_native(root)
                scenes.verify_saved(root, summary, metrics)
                path = root / "round-2/main-static.ppm"
                raw_path = path.parent / "raw.json"
                raw = json.loads(raw_path.read_text())
                if damage == "missing":
                    path.unlink()
                if damage == "pixels":
                    path.write_bytes(b"P6\n4 3\n255\n" + bytes([0, 0, 0] * 12))
                if damage == "blank":
                    path.write_bytes(b"P6\n4 3\n255\n" + bytes([0, 0, 0] * 12))
                    raw["cases"][0]["capture"]["rgba_sha256"] = hashlib.sha256(
                        bytes([0, 0, 0, 255] * 12)
                    ).hexdigest()
                if damage == "count":
                    raw["cases"][0]["capture"]["non_background_pixels"] = 1
                if damage == "dimensions":
                    path.write_bytes(b"P6\n3 4\n255\n" + bytes([255, 0, 0] * 12))
                if damage == "truncated":
                    path.write_bytes(b"P6\n4 3\n255\n" + bytes([255, 0, 0]))
                if damage == "unsafe":
                    raw["cases"][0]["capture"]["file"] = "../outside.ppm"
                if damage == "symlink":
                    outside = root / "outside.ppm"
                    path.rename(outside)
                    path.symlink_to(outside)
                raw_path.write_text(json.dumps(raw))
                with self.assertRaisesRegex(ValueError, "capture|Capture"):
                    scenes.verify_saved(root, summary, metrics)

    def test_new_native_suite_can_revalidate_its_saved_workload_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scene.sog").write_bytes(b"test fixture")
            sha = hashlib.sha256(b"test fixture").hexdigest()
            files = [{"path": "scene.sog", "bytes": 12, "sha256": sha}]
            manifest = {
                "schema": "inlier-gaussian-fixture/1",
                "dataset_id": "scene-v1",
                "root": tmp,
                "entrypoint_relative": "scene.sog",
                "format": "sog-v2-bundle",
                "files": files,
                "asset_identity": "sha256:"
                + hashlib.sha256(scenes.canonical(files).encode()).hexdigest(),
            }
            (root / "fixture.json").write_text(json.dumps(manifest))
            report = native()
            report["source"]["sha256"] = sha

            def producer(command, _log):
                usage = Path(command[command.index("--output") + 1])
                producer_args = command[command.index("--") + 1 :]
                folder = Path(producer_args[producer_args.index("--output") + 1])
                folder.mkdir()
                report["environment"]["executable_sha256"] = "c" * 64
                usage.write_text(
                    json.dumps(
                        {
                            "schema": "dimensify-process-usage/1",
                            "exit_code": 0,
                            "executable_sha256": "c" * 64,
                            "method": "test-wait4",
                            "platform": "linux",
                            "peak_rss_bytes": 1000,
                            "cpu_user_ns": 100,
                            "cpu_system_ns": 100,
                        }
                    )
                )
                save_native(folder, report)

            args = SimpleNamespace(
                kind="native",
                preset="default",
                fixture=root / "fixture.json",
                rounds=3,
                runner=root / "runner",
                output=root / "results",
                camera=None,
                gpu_timestamps=False,
            )
            with patch.object(scenes, "execute", side_effect=producer):
                scenes.run(args)
            summary = json.loads((args.output / "summary.json").read_text())
            metrics = json.loads((args.output / "bmf.json").read_text())
            scenes.verify_saved(args.output, summary, metrics)
            self.assertEqual(summary["schema"], "dimensify-real-scene/2")
            self.assertTrue(
                all(
                    name.startswith("real-scene-v2/") and "/workload-" in name
                    for name in metrics
                )
            )

    def test_native_lod_run_and_saved_verification_check_capture_files(self):
        from test_perf_lod import report as lod_report

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            assets = root / "assets"
            shutil.copytree(
                scenes.ROOT / "crates/splat/tests/fixtures/hosted-stream", assets
            )
            files = sorted(
                [
                    {
                        "path": str(path.relative_to(assets)),
                        "bytes": path.stat().st_size,
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    }
                    for path in assets.rglob("*")
                    if path.is_file()
                ],
                key=lambda row: row["path"],
            )
            manifest = {
                "schema": "inlier-gaussian-fixture/1",
                "dataset_id": "scene-v1",
                "root": str(assets),
                "entrypoint_relative": "lod-meta.json",
                "format": "sog-v2-hierarchy",
                "files": files,
                "asset_identity": "sha256:"
                + hashlib.sha256(
                    json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
                ).hexdigest(),
            }
            fixture_path = root / "fixture.json"
            fixture_path.write_text(json.dumps(manifest))

            def producer(command, _log):
                usage = Path(command[command.index("--output") + 1])
                producer_args = command[command.index("--") + 1 :]
                folder = Path(producer_args[producer_args.index("--output") + 1])
                report = lod_report(folder)
                report["gpu_timestamps_requested"] = False
                report["environment"]["adapter"]["backend"] = "Vulkan"
                report["source"]["sha256"] = hashlib.sha256(
                    (assets / "lod-meta.json").read_bytes()
                ).hexdigest()
                usage.write_text(
                    json.dumps(
                        {
                            "schema": "dimensify-process-usage/1",
                            "exit_code": 0,
                            "executable_sha256": "b" * 64,
                            "method": "test-wait4",
                            "platform": "linux",
                            "peak_rss_bytes": 1000,
                            "cpu_user_ns": 100,
                            "cpu_system_ns": 100,
                        }
                    )
                )
                (folder / "raw.json").write_text(json.dumps(report))

            args = SimpleNamespace(
                kind="native-lod",
                preset="default",
                fixture=fixture_path,
                rounds=3,
                runner=root / "runner",
                output=root / "results",
                camera=None,
                gpu_timestamps=False,
            )
            with patch.object(scenes, "execute", side_effect=producer):
                scenes.run(args)
            summary = json.loads((args.output / "summary.json").read_text())
            metrics = json.loads((args.output / "bmf.json").read_text())
            scenes.verify_saved(args.output, summary, metrics)
            (args.output / "round-2/static.ppm").unlink()
            with self.assertRaisesRegex(ValueError, "capture"):
                scenes.verify_saved(args.output, summary, metrics)

    def test_constrained_native_preset_cannot_silently_run_default_settings(self):
        # These flags affect the streaming viewer only; accepting them for a
        # whole-file native run would invent a separate, identical history.
        import subprocess
        import sys

        process = subprocess.run(
            [
                sys.executable,
                str(Path(scenes.__file__)),
                "run",
                "--kind",
                "native",
                "--preset",
                "constrained",
                "--fixture",
                "/missing",
                "--output",
                "/missing",
            ],
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(process.returncode, 0)
        self.assertIn("constrained", process.stderr)

    def test_upload_rebuild_rejects_changed_saved_metrics(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            summary = {
                "kind": "native",
                "rounds": 3,
                "preset": "default",
                "dataset_id": "scene-v1",
                "asset_identity": "sha256:" + "b" * 64,
                "commit": "a" * 40,
                "identity": scenes.native_identity(native()),
                "publication_eligible": True,
            }
            summary["testbed"] = scenes.testbed(summary["identity"])
            for number in range(1, 4):
                folder = root / f"round-{number}"
                folder.mkdir()
                save_native(folder, native())
            with self.assertRaisesRegex(ValueError, "saved|match"):
                scenes.verify_saved(
                    root, summary, {"invented": {"latency": {"value": 1}}}
                )


if __name__ == "__main__":
    unittest.main()
