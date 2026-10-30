"""Behavior checks for Cargo-driven showcase discovery."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "showcase", Path(__file__).with_name("showcase.py")
)
showcase = importlib.util.module_from_spec(spec)
spec.loader.exec_module(showcase)


class DiscoveryTests(unittest.TestCase):
    def test_new_cargo_example_is_discovered_without_a_gallery_entry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "examples").mkdir()
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text(
                '[package]\nname="discovery-fixture"\nversion="0.1.0"\nedition="2024"\n'
            )
            (root / "src/lib.rs").write_text("")
            (root / "examples/new_camera.rs").write_text(
                "//! A newly added camera example.\nfn main() {}\n"
            )
            metadata = json.loads(
                subprocess.check_output(
                    [
                        "cargo",
                        "metadata",
                        "--offline",
                        "--no-deps",
                        "--format-version",
                        "1",
                    ],
                    cwd=root,
                    text=True,
                )
            )
            entries = showcase.discover(metadata, root)
            self.assertEqual(len(entries), 1)
            self.assertEqual(entries[0]["title"], "New camera")
            self.assertEqual(entries[0]["targets"], ["local"])
            self.assertIn("--example new_camera", entries[0]["command"])
            self.assertIsNone(entries[0]["href"])
            # A newly opted-in example requires no separate catalog edit.
            package = metadata["packages"][0]
            package["features"] = {"viewer": [], "browser-demo": ["viewer"]}
            package["targets"][-1]["required-features"] = ["viewer"]
            package["metadata"] = {
                "showcase": [
                    {
                        "target": "new_camera",
                        "browser": True,
                        "features": ["browser-demo"],
                    }
                ]
            }
            browser = showcase.discover(metadata, root)[0]
            self.assertIn("browser", browser["targets"])
            self.assertEqual(
                browser["href"], "../examples/discovery-fixture/new_camera/"
            )
            self.assertIn("--features browser-demo", browser["command"])
            package["metadata"]["showcase"][0]["target"] = "missing_example"
            with self.assertRaisesRegex(ValueError, "Invalid"):
                showcase.discover(metadata, root)


class PackagingTests(unittest.TestCase):
    def site(self, root):
        site = root / "source"
        for name in [
            "index.html",
            "gallery/index.html",
            "gallery/gallery.mjs",
            "gallery/gallery.css",
            "pkg/splat.js",
            "pkg/splat_bg.wasm",
            "sog-codec/sog_codec.js",
            "sog-codec/sog_codec_bg.wasm",
            "sog-decode-worker.mjs",
            "examples/camera/index.html",
            "examples/camera/pkg/camera.js",
            "examples/camera/pkg/camera_bg.wasm",
            "pkg/snippets/codec/runtime.mjs",
        ]:
            file = site / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(
                b"\x00asm\x01\x00\x00\x00" if name.endswith(".wasm") else b"fixture"
            )
        (site / "stale-build.wasm").write_text("do not publish")
        entries = [
            dict(
                id="splat",
                href="../index.html",
                build=dict(target="splat", sog_worker=True),
            ),
            dict(
                id="camera",
                href="../examples/camera/",
                build=dict(target="camera", sog_worker=False),
            ),
            dict(id="native", href=None, build={}),
        ]
        return site, entries

    def test_packages_only_catalog_assets_and_lands_at_gallery(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            site, entries = self.site(root)
            output = root / "public"
            showcase.package_site(entries, site, output)
            self.assertIn("gallery/", (output / "index.html").read_text())
            self.assertEqual((output / "viewer.html").read_text(), "fixture")
            self.assertIn(
                "../viewer.html", (output / "gallery/catalog.mjs").read_text()
            )
            self.assertTrue((output / "examples/camera/pkg/camera_bg.wasm").is_file())
            self.assertTrue((output / "sog-codec/sog_codec_bg.wasm").is_file())
            self.assertTrue((output / "pkg/snippets/codec/runtime.mjs").is_file())
            self.assertFalse((output / "stale-build.wasm").exists())
            self.assertEqual(entries[0]["href"], "../index.html")

    def test_missing_wasm_prevents_partial_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            site, entries = self.site(root)
            (site / "examples/camera/pkg/camera_bg.wasm").unlink()
            with self.assertRaisesRegex(ValueError, "camera_bg.wasm"):
                showcase.package_site(entries, site, root / "public")
            self.assertFalse((root / "public").exists())

    def test_invalid_worker_binary_prevents_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            site, entries = self.site(root)
            (site / "sog-codec/sog_codec_bg.wasm").write_text("HTML error page")
            with self.assertRaisesRegex(ValueError, "Invalid WebAssembly"):
                showcase.package_site(entries, site, root / "public")
            self.assertFalse((root / "public").exists())

    def test_refuses_to_overwrite_existing_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            site, entries = self.site(root)
            with self.assertRaises(FileExistsError):
                showcase.package_site(entries, site, site)
            self.assertTrue((site / "index.html").is_file())


if __name__ == "__main__":
    unittest.main()
