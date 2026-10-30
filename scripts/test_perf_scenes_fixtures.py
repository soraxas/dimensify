"""Fixtures must verify every byte and bind measurements to their entrypoint."""

import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import perf_scenes as scenes
from test_perf_scenes_native import native


class SceneFixtures(unittest.TestCase):
    def test_lod_fixture_binds_entrypoint_not_first_chunk(self):
        data = {
            "dataset_id": "playbot-lod-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "format": "sog-v2-hierarchy",
            "entrypoint_relative": "lod-meta.json",
            "files": [
                {"path": "0/means.webp", "sha256": "c" * 64},
                {"path": "lod-meta.json", "sha256": "d" * 64},
            ],
        }
        summary = {
            "kind": "native-lod",
            "dataset_id": data["dataset_id"],
            "asset_identity": data["asset_identity"],
        }
        scenes.bind_fixture(summary, data, [{"source": {"sha256": "d" * 64}}])
        with self.assertRaisesRegex(ValueError, "hierarchy"):
            scenes.bind_fixture(summary, data, [{"source": {"sha256": "c" * 64}}])

    def test_browser_bundle_fixture_binds_entrypoint_not_first_file(self):
        data = {
            "dataset_id": "scene-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "format": "sog-v2-bundle",
            "entrypoint_relative": "scene.sog",
            "files": [
                {"path": "camera.json", "sha256": "c" * 64},
                {"path": "scene.sog", "sha256": "d" * 64},
            ],
        }
        summary = {
            "kind": "browser",
            "dataset_id": data["dataset_id"],
            "asset_identity": data["asset_identity"],
        }
        scenes.bind_fixture(
            summary, data, [{"metadata": {"asset": {"identity": "sha256:" + "d" * 64}}}]
        )
        with self.assertRaisesRegex(ValueError, "fixture"):
            scenes.bind_fixture(
                summary,
                data,
                [{"metadata": {"asset": {"identity": "sha256:" + "c" * 64}}}],
            )

    def test_saved_fixture_cannot_relabel_measurements(self):
        summary = {
            "dataset_id": "different-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "kind": "native",
        }
        data = {
            "dataset_id": "actual-v1",
            "asset_identity": "sha256:" + "b" * 64,
            "files": [{"sha256": "b" * 64}],
        }
        with self.assertRaisesRegex(ValueError, "fixture"):
            scenes.bind_fixture(summary, data, [native()])

    def test_fixture_verifies_bytes_and_complete_asset_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scene.sog").write_bytes(b"abc")
            files = [
                {
                    "path": "scene.sog",
                    "bytes": 3,
                    "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
                }
            ]
            identity = hashlib.sha256(
                json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            manifest = {
                "schema": "inlier-gaussian-fixture/1",
                "dataset_id": "scene-v1",
                "asset_identity": "sha256:" + identity,
                "root": str(root),
                "entrypoint_relative": "scene.sog",
                "files": files,
            }
            path = root / "fixture.json"
            path.write_text(json.dumps(manifest))
            self.assertEqual(
                scenes.fixture(path)["entrypoint"], str(root.resolve() / "scene.sog")
            )
            (root / "scene.sog").write_bytes(b"bad")
            with self.assertRaisesRegex(ValueError, "hash"):
                scenes.fixture(path)

    def test_fixture_rejects_escape_and_wrong_set_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "fixture.json"
            report = {
                "schema": "inlier-gaussian-fixture/1",
                "dataset_id": "scene-v1",
                "root": tmp,
                "entrypoint_relative": "../escape.sog",
                "asset_identity": "sha256:" + "0" * 64,
                "files": [{"path": "../escape.sog", "bytes": 1, "sha256": "0" * 64}],
            }
            path.write_text(json.dumps(report))
            with self.assertRaises(ValueError):
                scenes.fixture(path)

    def test_hierarchy_fixture_requires_every_chunk_texture_and_environment_dependency(
        self,
    ):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            hierarchy = root / "scene"
            shutil.copytree(
                scenes.ROOT / "crates/splat/tests/fixtures/hosted-stream", hierarchy
            )
            shutil.copytree(hierarchy / "chunks", hierarchy / "environment")
            entrypoint = hierarchy / "lod-meta.json"
            manifest = json.loads(entrypoint.read_text())
            manifest["environment"] = "environment/meta.json"
            entrypoint.write_text(json.dumps(manifest))
            meta_path = hierarchy / "chunks/meta.json"
            meta = json.loads(meta_path.read_text())
            meta["shN"] = {
                "count": 1,
                "bands": 1,
                "files": ["centroids.webp", "labels.webp"],
            }
            meta_path.write_text(json.dumps(meta))
            for name in ("centroids.webp", "labels.webp"):
                (meta_path.parent / name).write_bytes(b"higher-band dependency")
            files = sorted(
                [
                    {
                        "path": str(path.relative_to(root)),
                        "bytes": path.stat().st_size,
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    }
                    for path in hierarchy.rglob("*")
                    if path.is_file()
                ],
                key=lambda row: row["path"],
            )
            data = {
                "schema": "inlier-gaussian-fixture/1",
                "dataset_id": "scene-v1",
                "root": tmp,
                "entrypoint_relative": "scene/lod-meta.json",
                "format": "sog-v2-hierarchy",
            }
            fixture_path = root / "fixture.json"

            def save(rows):
                data["files"] = rows
                data["asset_identity"] = (
                    "sha256:"
                    + hashlib.sha256(
                        json.dumps(rows, sort_keys=True, separators=(",", ":")).encode()
                    ).hexdigest()
                )
                fixture_path.write_text(json.dumps(data))

            save(files)
            self.assertEqual(
                scenes.fixture(fixture_path)["entrypoint"], str(entrypoint.resolve())
            )
            for missing in (
                "scene/chunks/meta.json",
                "scene/chunks/means_l.webp",
                "scene/chunks/centroids.webp",
                "scene/chunks/labels.webp",
                "scene/environment/meta.json",
                "scene/environment/sh0.webp",
            ):
                with self.subTest(missing=missing):
                    save([row for row in files if row["path"] != missing])
                    with self.assertRaisesRegex(ValueError, "dependency"):
                        scenes.fixture(fixture_path)


if __name__ == "__main__":
    unittest.main()
