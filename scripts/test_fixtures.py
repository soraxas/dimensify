"""Keep the tiny hosted-stream fixture usable in ordinary source clones."""
from pathlib import Path
import unittest
import zipfile

class FixtureTests(unittest.TestCase):
    def test_hosted_textures_match_the_bundled_sog_bytes(self):
        root = Path(__file__).resolve().parents[1] / "crates/splat/tests/fixtures"
        with zipfile.ZipFile(root / "tiny.sog") as archive:
            textures = [name for name in archive.namelist() if name.endswith(".webp")]
            self.assertEqual(len(textures), 5)
            for name in textures:
                with self.subTest(texture=name):
                    actual = (root / "hosted-stream/chunks" / name).read_bytes()
                    self.assertEqual(actual, archive.read(name))
                    self.assertTrue(actual.startswith(b"RIFF"))
