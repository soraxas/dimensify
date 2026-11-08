import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location("captures", Path(__file__).with_name("pointcloud-captures.py"))
captures = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(captures)


class PointcloudCaptureTests(unittest.TestCase):
    def fixture(self, root):
        raw = root / "raw"
        raw.mkdir()
        (raw / "provenance.json").write_text(json.dumps({"gitHead": "captured-head", "sourceFingerprint": "captured-code", "os": "Captured OS"}))
        (raw / "scene.json").write_text('{"fixture":"sphere"}')
        for label, (width, height) in captures.SIZES.items():
            (raw / f"{label}-adapter.txt").write_text("Observed GPU\nMetal\n\n\n")
            (raw / f"{label}-settings.json").write_text(json.dumps({"size": [width, height]}))
            for case in captures.reporter.POINTCLOUD_CASES:
                (raw / f"{label}-{case}.ppm").write_bytes(f"P6\n{width} {height}\n255\n".encode() + b"\x12\x34\x56" * width * height)
                (raw / f"{label}-{case}.txt").write_text(case)
        return raw

    def test_repackaging_preserves_capture_identity_and_does_not_hash_current_code(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = self.fixture(root)
            with mock.patch.object(captures, "source_identity", side_effect=AssertionError("packaging must not relabel sources")):
                with mock.patch.object(captures.platform, "platform", return_value="First packaging OS"):
                    captures.package(raw, root / "first")
                (raw / "provenance.json").write_text(json.dumps({"gitHead": "other-captured-head", "sourceFingerprint": "other-captured-code", "os": "Captured OS"}))
                with mock.patch.object(captures.platform, "platform", return_value="Other packaging OS"):
                    captures.package(raw, root / "second")
            for label in captures.SIZES:
                first = root / "first" / label
                second = root / "second" / label
                self.assertEqual((first / "captures.json").read_bytes(), (second / "captures.json").read_bytes())
                provenance = json.loads((second / "provenance.json").read_text())
                self.assertEqual(provenance["gitHead"], "other-captured-head")
                self.assertEqual(provenance["sourceFingerprint"], "other-captured-code")
                self.assertEqual((first / "scene.json").read_bytes(), (raw / "scene.json").read_bytes())

    def test_wrong_sized_or_truncated_readback_is_rejected_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = self.fixture(root)
            (raw / "desktop-ready.ppm").write_bytes(b"P6\n1 1\n255\n\0\0\0")
            with self.assertRaisesRegex(ValueError, "malformed PPM"):
                captures.package(raw, root / "output")
            self.assertFalse((root / "output").exists())


if __name__ == "__main__":
    unittest.main()
