"""Trusted reporting must consume only a bounded, identified PNG artifact."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

SCRIPT = Path(__file__).with_name("visual-artifact.py")


class ArtifactTests(unittest.TestCase):
    def module(self):
        self.assertTrue(SCRIPT.is_file(), "Trusted artifact validation is missing")
        spec = importlib.util.spec_from_file_location("visual_artifact", SCRIPT)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def archive(self, extra=None):
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            captures = [{"name": name, "file": name + ".png", "width": 256, "height": 256} for name in self.module().CASES]
            archive.writestr("captures.json", json.dumps({"schema": "dimensify-visual-captures/1", "metadata": {}, "captures": captures}))
            for capture in captures:
                archive.writestr(capture["file"], b"\x89PNG\r\n\x1a\n" + b"test")
            if extra:
                archive.writestr(*extra)
        return data.getvalue()

    def test_only_expected_pngs_and_manifest_are_extracted(self):
        module = self.module()
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "images"
            module.extract(self.archive(("build.log", "untrusted log")), output)
            self.assertEqual({p.name for p in output.iterdir()}, {"captures.json"} | {name + ".png" for name in module.CASES})

    def test_zip_paths_and_symlinks_fail_before_any_write(self):
        module = self.module()
        for name in ["../escape", "/absolute", "a/../../escape", "a\\..\\escape"]:
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "images"
                with self.assertRaises(ValueError):
                    module.extract(self.archive((name, "bad")), output)
                self.assertFalse(output.exists())
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            link = zipfile.ZipInfo("symlink")
            link.create_system = 3
            link.external_attr = 0o120777 << 16
            archive.writestr(link, "../outside")
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                module.extract(buffer.getvalue(), Path(directory) / "images")

    def test_source_workflow_run_is_bound_to_repository_path_and_commit(self):
        module = self.module()
        run = {"id": 123, "path": ".github/workflows/visual-correctness.yml", "head_sha": "a" * 40,
               "event": "pull_request", "repository": {"full_name": "example/viewer"}, "status": "completed"}
        module.validate_run(run, "example/viewer", 123, "a" * 40)
        for key, value in [("head_sha", "b" * 40), ("path", ".github/workflows/other.yml"), ("id", 124),
                           ("event", "pull_request_target"), ("repository", {"full_name": "attacker/viewer"})]:
            with self.assertRaises(ValueError):
                module.validate_run(dict(run, **{key: value}), "example/viewer", 123, "a" * 40)


if __name__ == "__main__":
    unittest.main()
