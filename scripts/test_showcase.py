"""Behavior checks for Cargo-driven showcase discovery."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

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
                        "release_profile": "wasm-release",
                    }
                ]
            }
            browser = showcase.discover(metadata, root)[0]
            self.assertIn("browser", browser["targets"])
            self.assertEqual(
                browser["href"], "../examples/discovery-fixture/new_camera/"
            )
            self.assertIn("--features browser-demo", browser["command"])
            self.assertEqual(browser["build"]["release_profile"], "wasm-release")
            package["metadata"]["showcase"][0]["release_profile"] = "unknown"
            with self.assertRaisesRegex(ValueError, "release profile"):
                showcase.discover(metadata, root)
            package["metadata"]["showcase"][0]["release_profile"] = "release"
            package["metadata"]["showcase"][0]["target"] = "missing_example"
            with self.assertRaisesRegex(ValueError, "Invalid"):
                showcase.discover(metadata, root)


class BuildTests(unittest.TestCase):
    def test_release_profile_override_uses_matching_cargo_and_bindgen_paths(self):
        for requested, override, expected in [
            ("release", "release", "release"),
            ("release", "wasm-release", "wasm-release"),
            ("dev", "wasm-release", "debug"),
            ("wasm-release", "release", "wasm-release"),
        ]:
            with (
                self.subTest(profile=requested, override=override),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                site = root / "site"
                (site / "gallery").mkdir(parents=True)
                entry = dict(
                    id="custom-viewer",
                    title="Viewer",
                    href="../viewer/",
                    build=dict(
                        package="viewer", target="viewer", kind="example",
                        features=[], sog_worker=False, release_profile=override,
                    ),
                )
                with (
                    patch.object(showcase, "SITE", site),
                    patch.object(showcase, "run") as run,
                ):
                    showcase.build(
                        [entry], {"target_directory": str(root / "target")}, requested
                    )
                cargo, bindgen = [call.args[0] for call in run.call_args_list]
                self.assertEqual(
                    cargo[cargo.index("--profile") + 1],
                    "dev" if requested == "dev" else expected,
                )
                self.assertEqual(
                    Path(bindgen[-1]),
                    root / "target/wasm32-unknown-unknown" / expected / "examples/viewer.wasm",
                )


class BuildGroupTests(unittest.TestCase):
    def test_groups_cover_every_browser_entry_exactly_once(self):
        metadata = json.loads(
            subprocess.check_output(
                ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
                cwd=showcase.ROOT,
                text=True,
            )
        )
        entries = showcase.discover(metadata, showcase.ROOT)
        browser = sorted(entry["id"] for entry in entries if entry["href"])
        grouped = sorted(
            entry["id"]
            for group in showcase.BUILD_GROUPS
            for entry in showcase.build_group(entries, group)
        )
        self.assertEqual(grouped, browser)
        for group in showcase.BUILD_GROUPS:
            self.assertTrue(showcase.build_group(entries, group), group)

    def test_fat_lto_builds_never_share_a_group(self):
        entries = [
            dict(id=name, href=f"../{name}/") for name in ("splat", "robot")
        ]
        self.assertEqual(
            [len(showcase.build_group(entries, group)) for group in ("splat", "robot")],
            [1, 1],
        )

    def test_new_browser_entry_joins_examples_and_local_entries_are_skipped(self):
        entries = [
            dict(id="brand-new", href="../new/"),
            dict(id="local-only", href=None),
        ]
        self.assertEqual(
            [entry["id"] for entry in showcase.build_group(entries, "examples")],
            ["brand-new"],
        )
        with self.assertRaises(ValueError):
            showcase.build_group(entries, "unknown")


class PackagingTests(unittest.TestCase):
    def test_named_viewer_route_uses_shared_assets_and_deterministic_scene(self):
        with tempfile.TemporaryDirectory() as directory:
            site = Path(directory)
            host = site / "index.html"
            host.write_text('<html><head></head><body><script type="module">import init from "./pkg/splat_viewer.js";</script></body></html>')
            showcase.viewer_assets(site, host)
            markup = (site / "splat-viewer/index.html").read_text()
            self.assertIn('<base href="../"', markup)
            self.assertFalse((site / "splat-viewer/pkg").exists())
            scene = site / "scenes/rings.ply"
            first = scene.read_bytes()
            self.assertIn(b"element vertex 384\n", first)
            self.assertEqual(len(first.split(b"end_header\n", 1)[1]), 384 * 59 * 4)
            showcase.viewer_assets(site, host)
            self.assertEqual(scene.read_bytes(), first)

    def site(self, root):
        site = root / "source"
        for name in [
            "index.html",
            "gallery/index.html",
            "gallery/gallery.mjs",
            "gallery/launch.mjs",
            "gallery/build-link.html",
            "gallery/build-link.mjs",
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
                "../splat-viewer/", (output / "gallery/catalog.mjs").read_text()
            )
            self.assertTrue((output / "splat-viewer/index.html").is_file())
            self.assertTrue((output / "gallery/build-link.html").is_file())
            self.assertTrue((output / "gallery/build-link.mjs").is_file())
            self.assertTrue((output / "scenes/rings.ply").is_file())
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
