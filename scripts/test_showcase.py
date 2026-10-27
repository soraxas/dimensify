"""Behavior checks for Cargo-driven showcase discovery."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("showcase", Path(__file__).with_name("showcase.py"))
showcase = importlib.util.module_from_spec(spec)
spec.loader.exec_module(showcase)

class DiscoveryTests(unittest.TestCase):
    def test_new_cargo_example_is_discovered_without_a_gallery_entry(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "examples").mkdir()
            (root / "src").mkdir()
            (root / "Cargo.toml").write_text('[package]\nname="discovery-fixture"\nversion="0.1.0"\nedition="2024"\n')
            (root / "src/lib.rs").write_text("")
            (root / "examples/new_camera.rs").write_text('//! A newly added camera example.\nfn main() {}\n')
            metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--offline", "--no-deps", "--format-version", "1"], cwd=root, text=True))
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
            package["metadata"] = {"showcase": [{"target": "new_camera", "browser": True, "features": ["browser-demo"]}]}
            browser = showcase.discover(metadata, root)[0]
            self.assertIn("browser", browser["targets"])
            self.assertEqual(browser["href"], "../examples/discovery-fixture/new_camera/")
            self.assertIn("--features browser-demo", browser["command"])
            package["metadata"]["showcase"][0]["target"] = "missing_example"
            with self.assertRaisesRegex(ValueError, "Invalid"):
                showcase.discover(metadata, root)

if __name__ == "__main__":
    unittest.main()
