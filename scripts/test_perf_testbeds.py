"""Naming must preserve history and keep environment labels readable."""

import json
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import perf_testbeds as names


class TestbedNames(unittest.TestCase):
    def test_cpu_labels_are_readable_and_fit_service_limit(self):
        name = names.cpu_name(
            "Ubuntu 24.04.5 LTS",
            "x86_64",
            "AMD EPYC 9V74 80-Core Processor",
            "rustc 1.98.1 (48a229cea 2026-09-01)",
            "pipeline",
        )
        self.assertEqual(name, "pipeline-cpu-epyc-9v74-ubuntu-24-04-5-x64-rust1-98-1")
        self.assertLessEqual(len(name), 64)
        self.assertNotRegex(name, r"[a-f0-9]{16}")

    def test_rename_cannot_silently_replace_a_testbed_uuid(self):
        before = subprocess.CompletedProcess(
            [], 0, json.dumps({"uuid": "original", "name": "old"}), ""
        )
        after = subprocess.CompletedProcess(
            [], 0, json.dumps({"uuid": "replacement", "name": "readable"}), ""
        )
        with (
            patch.object(names.subprocess, "run", side_effect=[before, after]),
            self.assertRaisesRegex(ValueError, "identity"),
        ):
            names.ensure_name("project", "old-slug", "readable", rename_existing=True)

    def test_service_errors_cannot_create_a_replacement_testbed(self):
        denied = subprocess.CompletedProcess([], 1, "", "HTTP 403 forbidden")
        with (
            patch.object(names.subprocess, "run", return_value=denied) as call,
            self.assertRaises(ValueError),
        ):
            names.ensure_name("project", "old-slug", "readable")
        call.assert_called_once_with(
            ["bencher", "testbed", "view", "project", "old-slug", "--attempts", "3"],
            text=True,
            capture_output=True,
        )

    def test_ci_keeps_existing_identity_without_requesting_forbidden_rename(self):
        before = subprocess.CompletedProcess(
            [], 0, json.dumps({"uuid": "original", "name": "old"}), ""
        )
        with patch.object(names.subprocess, "run", return_value=before) as call:
            names.ensure_name("project", "old-slug", "readable")
        call.assert_called_once_with(
            ["bencher", "testbed", "view", "project", "old-slug", "--attempts", "3"],
            text=True,
            capture_output=True,
        )

    def test_alternate_uuid_spellings_cannot_rename_the_same_testbed_twice(self):
        with tempfile.TemporaryDirectory() as tmp:
            plan = Path(tmp) / "plan.json"
            plan.write_text(
                json.dumps(
                    [
                        {
                            "uuid": "6a4fd288-52c2-4985-a8e5-7463e5c1ef98",
                            "name": "first",
                        },
                        {
                            "uuid": "{6A4FD288-52C2-4985-A8E5-7463E5C1EF98}",
                            "name": "second",
                        },
                    ]
                )
            )
            with (
                patch.object(
                    sys, "argv", ["perf_testbeds.py", "--plan", str(plan), "--apply"]
                ),
                patch.object(names, "ensure_name") as rename,
                self.assertRaisesRegex(ValueError, "duplicate"),
            ):
                names.main()
            rename.assert_not_called()
