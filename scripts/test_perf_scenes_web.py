"""The served browser site must match its recorded build."""

import json
from pathlib import Path
import tempfile
import unittest

import perf_scenes as scenes


class SceneSite(unittest.TestCase):
    def test_site_refuses_missing_worker_before_launching_browser(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            report = {"feature": "splat-viewer-perf", "files": {}}
            (root / "build.json").write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "worker|incomplete"):
                scenes.verify_site(root)


if __name__ == "__main__":
    unittest.main()
