"""Baseline identity drift must not silently downgrade CI acceptance."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("visual_check", Path(__file__).with_name("visual-check.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReferenceSelectionTests(unittest.TestCase):
    def test_same_lane_selects_exact_adapter_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            lanes = []
            for adapter in ["M1", "M5"]:
                path = Path(directory) / adapter
                path.mkdir()
                metadata = {key: "same" for key in module.IDENTITY}
                metadata.update(lane="native-metal", adapter=adapter)
                (path / "manifest.json").write_text(json.dumps({"metadata": metadata}))
                lanes.append({"lane": "native-metal", "directory": str(path)})
            self.assertEqual(module.select_baseline(metadata, {"lanes": lanes}), str(path))
            metadata["driver"] = "changed"
            with self.assertRaises(ValueError):
                module.select_baseline(metadata, {"lanes": lanes})

    def test_empty_registry_does_not_invent_a_reference(self):
        self.assertIsNone(module.select_baseline({"lane": "browser"}, {"lanes": []}))


if __name__ == "__main__":
    unittest.main()
