import copy
import importlib.util
import io
import json
import pathlib
import struct
import tempfile
import unittest
from unittest import mock
from contextlib import redirect_stdout, redirect_stderr
import urllib.error
import zlib


SPEC = importlib.util.spec_from_file_location("visual_pixeleagle", pathlib.Path(__file__).with_name("visual-pixeleagle.py"))
reporter = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reporter)

PROJECT = "12345678-1234-1234-1234-123456789abc"
SOURCE = {"repository": "example/dimensify", "runId": 42, "commit": "c" * 40, "artifact": "visual-native-linux", "conclusion": "success"}


def png():
    def chunk(kind, value):
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value))
    rows = (b"\0" + bytes([200, 20, 10, 255]) * 256) * 256
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 256, 256, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def comparison():
    return {"project_id": PROJECT, "from": 22, "to": 11, "comparison_id": 33,
            "missing": [], "new": [], "diff": [],
            "unchanged": [{"name": name, "hash": "hash"} for name in reporter.CASES]}


class ComparisonTests(unittest.TestCase):
    def validate(self, value):
        return reporter.validate_comparison(value, reporter.CASES, 22, 11, PROJECT)

    def test_complete_exact_inventory_passes(self):
        self.assertEqual(self.validate(comparison()), "passed")

    def test_malformed_missing_duplicate_and_different_results_fail_closed(self):
        for damage in ["null", "missing-field", "missing", "new", "empty", "duplicate", "extra", "wrong-run", "wrong-project", "difference", "nan", "unknown-status"]:
            with self.subTest(damage=damage):
                value = comparison()
                entry = value["unchanged"][0]
                if damage == "null": value = None
                if damage == "missing-field": del value["diff"]
                if damage in ("missing", "new"):
                    value[damage].append(value["unchanged"].pop())
                if damage == "empty": value["unchanged"] = []
                if damage == "duplicate": value["unchanged"].append(entry)
                if damage == "extra": value["unchanged"].append({"name": "extra", "hash": "hash"})
                if damage == "wrong-run": value["to"] = 12
                if damage == "wrong-project": value["project_id"] = "another-project"
                if damage in ("difference", "nan", "unknown-status"):
                    value["unchanged"].pop(0)
                    value["diff"] = [{**entry, "previous_hash": "previous", "diff": {"Done": 0.01} if damage == "difference" else {"Done": float("nan")} if damage == "nan" else "surprise"}]
                with self.assertRaises(reporter.ReportingError): self.validate(value)

    def test_processing_polls_and_completed_zero_difference_passes(self):
        value = comparison()
        entry = value["unchanged"].pop()
        value["diff"] = [{**entry, "previous_hash": "previous", "diff": "Processing"}]
        self.assertEqual(self.validate(value), "pending")
        value["diff"][0]["diff"] = {"Done": 0.0}
        self.assertEqual(self.validate(value), "passed")


class ReporterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = pathlib.Path(self.temp.name)
        self.captures = self.directory / "captures"
        self.captures.mkdir()
        self.metadata = {"lane": "test", "target": "native", "backend": "Vulkan", "adapter": "software", "driver": "test", "browser": "none",
                         "sceneHash": "a" * 64, "settingsHash": "b" * 64, "viewport": {"width": 256, "height": 256}}
        self.manifest = {"schema": "dimensify-visual-captures/1", "metadata": self.metadata,
                         "captures": [{"name": name, "file": name + ".png", "width": 256, "height": 256} for name in reporter.CASES]}
        self.save()
        for entry in self.manifest["captures"]: (self.captures / entry["file"]).write_bytes(png())
        self.calls = []
        self.source = self.directory / "source.json"
        self.source.write_text(json.dumps(SOURCE))

    def save(self):
        (self.captures / "captures.json").write_text(json.dumps(self.manifest))

    def fake(self, method, route, body, content_type):
        self.calls.append((method, route, body, content_type))
        if method == "GET" and route == "/runs/11":
            return 200, {"id": 11, "timestamp": 1, "metadata": reporter.flatten_metadata(self.metadata), "screenshots": comparison()["unchanged"]}
        if route == "/runs": return 201, {"id": 22}
        if route == "/runs/22":
            name = body.split(b'name="screenshot"\r\n\r\n')[1].split(b"\r\n")[0].decode()
            return 200, {"name": name, "hash": "hash"}
        return 200, comparison()

    def run_report(self, transport=None, upload_only=False):
        return reporter.publish(self.captures, baseline_run=None if upload_only else 11, project=PROJECT,
                                upload_only=upload_only, source=SOURCE, transport=transport or self.fake, poll_interval=0, max_polls=2)

    def test_uploads_then_compares_only_explicit_run_and_checks_baseline_first(self):
        result = self.run_report()
        self.assertTrue(result["passed"])
        self.assertEqual(self.calls[0][:2], ("GET", "/runs/11"))
        self.assertEqual(sum(c[1] == "/runs/22" for c in self.calls), 5)
        metadata = json.loads(next(c[2] for c in self.calls if c[1] == "/runs"))
        self.assertIs(metadata["compare_with_golden"], False)
        self.assertEqual(result["source"], SOURCE)
        for key, value in SOURCE.items(): self.assertEqual(metadata["source" + key[0].upper() + key[1:]], str(value))
        self.assertFalse(any("golden" in c[1] or "auto" in c[1] for c in self.calls))
        self.assertEqual(result["url"], f"https://pixel-eagle.com/project/{PROJECT}/run/22/compare/11")

    def test_upload_only_is_explicit_bootstrap_and_never_comparison_pass(self):
        result = self.run_report(upload_only=True)
        self.assertEqual(result["state"], "uploaded-no-comparison")
        self.assertIs(result["passed"], False)
        self.assertFalse(any("compare" in c[1] or c[0] == "GET" for c in self.calls))

    def test_identity_mismatch_does_not_create_or_upload_run(self):
        def transport(*args):
            status, body = self.fake(*args)
            body["metadata"]["driver"] = "incompatible"
            return status, body
        with self.assertRaisesRegex(reporter.ReportingError, "identity"):
            self.run_report(transport)
        self.assertEqual(len(self.calls), 1)

    def test_incomplete_poll_and_bad_http_schema_upload_fail(self):
        for damage in ["timeout", "http", "schema", "upload-name"]:
            with self.subTest(damage=damage):
                self.calls.clear()
                def transport(*args):
                    status, body = self.fake(*args)
                    if damage == "http": return 401, {"error": "do not log server body"}
                    if damage == "schema": return status, None
                    if damage == "upload-name" and args[1] == "/runs/22": body["name"] = "wrong"
                    if damage == "timeout" and "/compare/" in args[1]:
                        entry = body["unchanged"].pop()
                        body["diff"] = [{**entry, "previous_hash": "previous", "diff": "Unknown"}]
                    return status, body
                with self.assertRaises(reporter.ReportingError): self.run_report(transport)

    def test_bad_artifacts_never_contact_service(self):
        original = copy.deepcopy(self.manifest)
        for damage in ["missing", "duplicate", "extra", "dimensions", "traversal", "symlink", "png", "crc", "compressed"]:
            with self.subTest(damage=damage):
                self.manifest = copy.deepcopy(original)
                self.calls.clear()
                scene = self.captures / self.manifest["captures"][0]["file"]
                scene.unlink(missing_ok=True)
                scene.write_bytes(png())
                extra = self.captures / "extra.png"
                extra.unlink(missing_ok=True)
                if damage == "missing": self.manifest["captures"].pop()
                if damage == "duplicate": self.manifest["captures"].append(self.manifest["captures"][0])
                if damage == "extra": extra.write_bytes(png())
                if damage == "dimensions": self.manifest["captures"][0]["width"] = 257
                if damage == "traversal": self.manifest["captures"][0]["file"] = "../escape.png"
                if damage == "symlink":
                    scene.unlink()
                    scene.symlink_to(self.captures / self.manifest["captures"][1]["file"])
                if damage == "png": scene.write_bytes(b"not png")
                if damage == "crc":
                    data = bytearray(png()); data[29] ^= 1; scene.write_bytes(data)
                if damage == "compressed":
                    # Recompute CRC so validation must reject the compressed stream itself.
                    data = bytearray(png())
                    length = struct.unpack_from(">I", data, 33)[0]
                    data[41] ^= 1
                    struct.pack_into(">I", data, 41 + length, zlib.crc32(data[37:41 + length]))
                    scene.write_bytes(data)
                self.save()
                with self.assertRaises(reporter.ReportingError): self.run_report()
                self.assertEqual(self.calls, [])

    def test_missing_credentials_is_a_failed_publication_with_a_report(self):
        output = self.directory / "out"
        with mock.patch.dict("os.environ", {}, clear=True), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            code = reporter.main(["--captures", str(self.captures), "--baseline-run", "11", "--out", str(output), "--source", str(self.source)])
        self.assertEqual(code, 1)
        report = json.loads((output / "publication.json").read_text())
        self.assertEqual(report["state"], "failed")
        self.assertFalse(report["passed"])
        self.assertEqual(report["source"], SOURCE)

    def test_transport_does_not_include_tokens_or_server_bodies_in_http_errors(self):
        token = "secret-token-for-test"
        transport = reporter.Transport(token)
        with mock.patch.object(transport.opener, "open", side_effect=urllib.error.HTTPError("https://pixel-eagle.com/runs", 401, token, {}, io.BytesIO(token.encode()))):
            with self.assertRaises(reporter.ReportingError) as caught:
                transport("POST", "/runs", b"{}", "application/json")
        self.assertNotIn(token, str(caught.exception))
        self.assertEqual(str(caught.exception), "Service HTTP status 401")

    def test_redirects_are_rejected_before_forwarding_authorization(self):
        with self.assertRaises(reporter.ReportingError):
            reporter.NoRedirects().redirect_request(None, None, 302, "redirect", {}, "https://another-host.test")

    def test_source_provenance_is_bounded_validated_and_never_used_as_baseline_identity(self):
        for key, value in [("repository", "owner/repo\n[unsafe]"), ("runId", True), ("runId", 0), ("commit", "short"),
                           ("artifact", "unknown"), ("conclusion", "surprise"), ("repository", "x" * 201 + "/repo")]:
            with self.subTest(key=key, value=value):
                invalid = {**SOURCE, key: value}
                self.source.write_text(json.dumps(invalid))
                with self.assertRaises(reporter.ReportingError): reporter.load_source(self.source)
        self.source.write_text(json.dumps({**SOURCE, "unexpected": "value"}))
        with self.assertRaises(reporter.ReportingError): reporter.load_source(self.source)
        # A different trusted source commit records the PR revision while the
        # rendering identity remains compatible with the reviewed baseline.
        source = {**SOURCE, "commit": "e" * 40, "conclusion": "failure"}
        result = reporter.publish(self.captures, baseline_run=11, project=PROJECT, source=source, transport=self.fake)
        self.assertEqual(result["source"], source)
        self.assertTrue(result["passed"])

    def test_summary_includes_safe_review_link_even_on_visual_failure(self):
        output = self.directory / "out"
        summary = self.directory / "summary.md"
        def transport(*args):
            status, value = self.fake(*args)
            if "/compare/" in args[1]:
                entry = value["unchanged"].pop()
                value["diff"] = [{**entry, "previous_hash": "previous", "diff": {"Done": 0.5}}]
            return status, value
        with mock.patch.dict("os.environ", {"PIXEL_EAGLE_TOKEN": "secret-token", "GITHUB_STEP_SUMMARY": str(summary)}, clear=True), mock.patch.object(reporter, "Transport", return_value=transport), redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            code = reporter.main(["--captures", str(self.captures), "--baseline-run", "11", "--out", str(output), "--source", str(self.source), "--project", PROJECT])
        self.assertEqual(code, 1)
        report = json.loads((output / "publication.json").read_text())
        self.assertFalse(report["passed"])
        self.assertEqual(report["source"], SOURCE)
        content = summary.read_text()
        self.assertIn(f"[Pixel Eagle comparison](https://pixel-eagle.com/project/{PROJECT}/run/22/compare/11)", content)
        self.assertIn("comparison passed: false", content)
        self.assertIn("https://github.com/example/dimensify/actions/runs/42", content)
        self.assertNotIn("secret-token", content)


if __name__ == "__main__": unittest.main()
