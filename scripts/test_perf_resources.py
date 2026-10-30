import copy
import json
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import perf_resources as resources


class ResourceMetrics(unittest.TestCase):
    def test_os_peak_units_are_platform_specific(self):
        self.assertEqual(resources.peak_rss_bytes(512, "darwin"), 512)
        self.assertEqual(resources.peak_rss_bytes(512, "linux"), 512 * 1024)
        self.assertIsNone(resources.peak_rss_bytes(512, "win32"))
        with self.assertRaises(ValueError):
            resources.peak_rss_bytes(-1, "linux")

    def test_supervisor_preserves_child_failure_and_binds_executable(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "usage.json"
            result = subprocess.run([sys.executable, str(Path(resources.__file__)),
                                     "--output", str(path), "--", sys.executable,
                                     "-c", "data=bytearray(4*1024*1024); raise SystemExit(7)"],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 7, result.stderr)
            report = json.loads(path.read_text())
            self.assertEqual(report["exit_code"], 7)
            self.assertEqual(len(report["executable_sha256"]), 64)
            if sys.platform in ("darwin", "linux"):
                self.assertGreater(report["peak_rss_bytes"], 4 * 1024 * 1024)
                self.assertGreater(report["cpu_user_ns"] + report["cpu_system_ns"], 0)

    def test_supervisor_resolves_path_command_and_hashes_the_executed_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            name = 'dimensify-resource-test-python'
            executable = directory / name
            executable.symlink_to(sys.executable)
            output = directory / 'usage.json'
            from unittest.mock import patch
            with patch.dict(os.environ, {'PATH': str(directory) + os.pathsep + os.environ.get('PATH', '')}):
                self.assertEqual(resources.supervisor([name, '-c', 'pass'], output), 0)
            self.assertEqual(json.loads(output.read_text())['executable_sha256'],
                             hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest())

    def test_supervisor_preserves_virtual_environment_launcher(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            environment = directory / 'venv'
            subprocess.run([sys.executable, '-m', 'venv', '--without-pip', str(environment)], check=True)
            binary = environment / ('Scripts/python.exe' if sys.platform == 'win32' else 'bin/python')
            observed = directory / 'prefix.txt'
            output = directory / 'usage.json'
            code = 'import sys; from pathlib import Path; Path(sys.argv[1]).write_text(sys.prefix)'
            self.assertEqual(resources.supervisor([str(binary), '-c', code, str(observed)], output), 0)
            self.assertEqual(Path(observed.read_text()).resolve(), environment.resolve())

    def test_native_memory_never_becomes_latency_or_vram(self):
        report = {"resource_usage": {
            "schema_version": 1,
            "checkpoints": [{"phase": "after_decode", "rss_bytes": 1234, "virtual_memory_bytes": 9000}],
            "decoded": {"capacity_bytes": 640},
            "gpu_checkpoints": [{"phase": "after_view:main-static", "tracked_buffer_bytes": 720,
                                 "buffers": [{"size_bytes": 512}, {"size_bytes": 208}]}]}}
        metrics = resources.native_metrics(report)
        self.assertEqual(metrics["memory/process-rss/after_decode"], {"process-memory-bytes": {"value": 1234}})
        self.assertEqual(metrics["memory/tracked-gpu-buffers/after_view:main-static"], {"gpu-buffer-bytes": {"value": 720}})
        self.assertFalse(any("vram" in name for name in metrics))
        report["resource_usage"]["checkpoints"][0]["rss_bytes"] = -1
        with self.assertRaises(ValueError):
            resources.native_metrics(report)

    def test_os_measurement_must_match_native_executable(self):
        report = {"environment": {"executable_sha256": "a" * 64},
                  "process_usage": {"schema": "dimensify-process-usage/1", "exit_code": 0,
                                    "executable_sha256": "b" * 64, "peak_rss_bytes": 100,
                                    "cpu_user_ns": 100, "cpu_system_ns": 100}}
        with self.assertRaisesRegex(ValueError, "executable"):
            resources.native_metrics(report)

    def test_browser_missing_capability_is_not_zero_memory(self):
        runs = [{"round": i, "mode": mode, "observations": [
            {"mainWasmLinearMemoryBytes": 100 * i, "mainJsHeapUsedBytes": None,
             "workerPeakWasmLinearMemoryBytes": None}], "final": {}}
            for i in range(1, 4) for mode in ("cold", "warm")]
        metrics = resources.browser_metrics(runs)
        self.assertEqual(metrics["browser-observed-main-wasm-capacity-peak"], {"wasm-memory-bytes": {"value": 200}})
        self.assertFalse(any("js-heap" in name or "worker-wasm" in name for name in metrics))
        bad = copy.deepcopy(runs)
        bad[0]["observations"][0]["mainWasmLinearMemoryBytes"] = float("nan")
        with self.assertRaises(ValueError):
            resources.browser_metrics(bad)

    def test_null_worker_and_partial_worker_timings_keep_coverage_explicit(self):
        runs = [{"round": i, "mode": "warm", "final": {"worker": None}} for i in range(1, 4)]
        self.assertEqual(resources.browser_metrics(runs), {})
        for run in runs:
            run["final"]["worker"] = {"started": 2, "completed": 1, "failed": 1,
                "handlerElapsedMs": 5, "timingSamples": {"handlerElapsedMs": 1}}
        metrics = resources.browser_metrics(runs)
        self.assertEqual(metrics["browser-worker-started"], {"count": {"value": 2}})
        self.assertEqual(metrics["browser-worker-handler-observed-samples"], {"count": {"value": 1}})
        self.assertEqual(metrics["browser-worker-observed-handler-wall-sum"], {"latency": {"value": 5_000_000}})
        self.assertFalse(any("total-wall" in name for name in metrics))

    def test_pass_segments_sum_within_scope_and_preserve_frame_identity(self):
        sample = {"scope": "gaussian_raster_render_pass", "identity": {"selection": "12"},
                  "measurement": {"duration_ms": 1.5, "failure": None}}
        case = {"name": "main-static", "gpu_pass_reports": [{"scope": sample["scope"]}],
                "rows": [{"sample_id": 12, "gpu_pass_samples": [sample, copy.deepcopy(sample)]}]}
        result = resources.gpu_pass_metrics(case)
        name = "main-static/gpu-pass/gaussian-raster-render-pass/median"
        self.assertEqual(result[name], {"latency": {"value": 3_000_000}})
        case["rows"][0]["sample_id"] = 13
        with self.assertRaisesRegex(ValueError, "another frame"):
            resources.gpu_pass_metrics(case)

    def test_incomplete_pass_or_zero_timestamp_does_not_become_latency(self):
        case = {"name": "main-static", "gpu_pass_reports": [{"scope": "gaussian_projection_compute_pass"}],
                "rows": [{"sample_id": 1, "gpu_pass_samples": []}]}
        result = resources.gpu_pass_metrics(case)
        self.assertEqual(len(result), 1)
        self.assertEqual(next(iter(result.values())), {"count": {"value": 0}})
        case["rows"][0]["gpu_pass_samples"] = [{"scope": "gaussian_projection_compute_pass",
            "identity": {"selection": "1"}, "measurement": {"duration_ms": 0, "failure": None}}]
        result = resources.gpu_pass_metrics(case)
        self.assertEqual(len(result), 1)
        self.assertEqual(next(iter(result.values())), {"count": {"value": 1}})


if __name__ == "__main__":
    unittest.main()
