import copy
import unittest

import perf_browser_gpu as gpu


def sample(ordinal, frame, duration, *, scope="raster", scene="2", failure=None):
    return {"scope": scope, "ordinal": ordinal,
            "identity": {"frame": str(frame), "view": "1", "scene": scene, "selection": str(frame)},
            "measurement": {"duration_ms": duration, "failure": failure}}


def runs(samples=None):
    samples = [sample(0, 1, 1.0)] if samples is None else samples
    completed = max((item["ordinal"] for item in samples), default=-1) + 1
    return [{"round": number, "mode": mode, "observations": [],
             "gpuSamples": {"samples": copy.deepcopy(samples), "historyGaps": 0,
                            "reports": [{"scope": "raster", "capability": "timestamp_query",
                                         "completed": completed, "invalid": 0,
                                         "dropped_busy": 2, "evicted_history": 0}]}}
            for number in (1, 2, 3) for mode in ("cold", "warm")]


def buffers(size=100):
    return {"live_bytes": size, "live_buffers": 1, "peak_live_bytes": size,
            "observed_bytes_total": size, "observed_buffers_total": 1,
            "categories": {"projection": {"bytes": size, "buffers": 1},
                           "compact_selection": {"bytes": size, "buffers": 1}},
            "entries": [{"id": "BufferId(1)", "bytes": size,
                         "categories": ["projection", "compact_selection"]}],
            "entries_truncated": 0, "packed_source_payload_bytes": 80,
            "resources": {"packed_resident": True, "prepared_planar": True}}


class BrowserGpuMetrics(unittest.TestCase):
    def test_segments_sum_only_within_scope_frame_view_then_round_quantiles(self):
        data = runs([sample(0, 1, 1), sample(1, 1, 2, scene="3"), sample(2, 2, 5)])
        # Cold snapshots are prefixes of warm: they must never be counted twice.
        data[0]["gpuSamples"]["samples"] = [sample(0, 1, 99)]
        for run in data:
            if run["round"] == 3 and run["mode"] == "warm":
                for item in run["gpuSamples"]["samples"]:
                    item["measurement"]["duration_ms"] *= 10
        result = gpu.metrics(data)
        self.assertEqual(result["browser-observed-gpu-raster-median"], {"latency": {"value": 4_000_000}})
        self.assertEqual(result["browser-observed-gpu-raster-p95"], {"latency": {"value": 5_000_000}})
        self.assertEqual(result["browser-observed-gpu-raster-valid-frames"], {"count": {"value": 2}})
        self.assertEqual(result["browser-observed-gpu-raster-valid-segments"], {"count": {"value": 3}})
        self.assertFalse(any("total" in key and "latency" in value for key, value in result.items()))

    def test_distinct_scopes_and_views_never_merge(self):
        data = runs([sample(0, 1, 1), sample(1, 1, 3), sample(0, 1, 7, scope="projection")])
        for run in data:
            run["gpuSamples"]["samples"][1]["identity"]["view"] = "2"
            run["gpuSamples"]["reports"][0]["completed"] = 2
            run["gpuSamples"]["reports"].append({"scope": "projection", "capability": "timestamp_query", "completed": 1})
        result = gpu.metrics(data)
        self.assertEqual(result["browser-observed-gpu-raster-median"]["latency"]["value"], 2_000_000)
        self.assertEqual(result["browser-observed-gpu-projection-median"]["latency"]["value"], 7_000_000)

    def test_invalid_segment_invalidates_whole_origin_frame_and_keeps_coverage_counts(self):
        data = runs([sample(0, 1, 9), sample(1, 1, None, failure="map_failed"), sample(2, 2, 2)])
        for run in data:
            run["gpuSamples"].update(historyGaps=4)
            run["gpuSamples"]["reports"][0]["invalid"] = 1
            run["gpuSamples"]["reports"][0]["evicted_history"] = 3
        result = gpu.metrics(data)
        self.assertEqual(result["browser-observed-gpu-raster-median"]["latency"]["value"], 2_000_000)
        self.assertEqual(result["browser-observed-gpu-raster-invalid-frames"], {"count": {"value": 1}})
        self.assertEqual(result["browser-observed-gpu-raster-invalid-segments"], {"count": {"value": 1}})
        self.assertEqual(result["browser-observed-gpu-history-gaps"], {"count": {"value": 4}})
        self.assertEqual(result["browser-observed-gpu-raster-dropped-busy"], {"count": {"value": 2}})
        self.assertEqual(result["browser-observed-gpu-raster-evicted-history"], {"count": {"value": 3}})

    def test_duplicate_ordinals_are_rejected_not_summed(self):
        data = runs([sample(0, 1, 1), sample(0, 1, 1)])
        with self.assertRaisesRegex(ValueError, "Duplicate.*ordinal"):
            gpu.metrics(data)

    def test_64bit_ids_remain_distinct_and_unsafe_numeric_ids_are_rejected(self):
        data = runs([sample(0, "18446744073709551614", 1), sample(1, "18446744073709551615", 3)])
        result = gpu.metrics(data)
        self.assertEqual(result["browser-observed-gpu-raster-valid-frames"]["count"]["value"], 2)
        data[1]["gpuSamples"]["samples"][0]["identity"]["frame"] = 9007199254740992
        with self.assertRaisesRegex(ValueError, "identity"):
            gpu.metrics(data)

    def test_zero_and_all_invalid_samples_never_fabricate_latency(self):
        for items in ([sample(0, 1, 0)], [sample(0, 1, None, failure="map_failed")], []):
            with self.subTest(items=items):
                data = runs(items)
                if items and items[0]["measurement"]["failure"]:
                    for run in data:
                        run["gpuSamples"]["reports"][0]["invalid"] = 1
                result = gpu.metrics(data)
                self.assertFalse(any("latency" in entry for entry in result.values()))
                self.assertIn("browser-observed-gpu-raster-valid-frames", result)

    def test_missing_or_unsupported_capability_is_omitted_changes_are_rejected(self):
        data = runs([])
        for run in data:
            run["gpuSamples"]["reports"][0]["capability"] = "unsupported_timestamp_query"
        self.assertEqual(gpu.metrics(data), {})
        data[1]["gpuSamples"]["reports"][0]["capability"] = "timestamp_query"
        with self.assertRaisesRegex(ValueError, "capability"):
            gpu.metrics(data)
        data = runs()
        del data[1]["gpuSamples"]
        with self.assertRaisesRegex(ValueError, "capability"):
            gpu.metrics(data)

    def test_buffer_peak_is_per_round_across_cold_and_warm_category_overlap_is_not_total(self):
        data = runs([])
        for run in data:
            size = run["round"] * (100 if run["mode"] == "cold" else 10)
            run["observations"] = [{"gpuBuffers": buffers(size)}]
        result = gpu.metrics(data)
        self.assertEqual(result["browser-observed-gpu-buffer-live-peak"], {"gpu-buffer-bytes": {"value": 200}})
        self.assertEqual(result["browser-observed-gpu-buffer-count-peak"], {"count": {"value": 1}})
        self.assertEqual(result["browser-observed-packed-source-payload-peak"], {"cache-payload-bytes": {"value": 80}})
        self.assertEqual(result["browser-observed-gpu-buffer-projection-peak"], {"gpu-buffer-bytes": {"value": 200}})
        self.assertFalse(any("vram" in name or "texture-allocation" in name for name in result))

    def test_buffer_duplicate_ids_inconsistent_sums_and_missing_rounds_are_rejected(self):
        original = runs([])
        for run in original:
            run["observations"] = [{"gpuBuffers": buffers()}]
        for kind in ("duplicate", "sum", "count", "partial", "category", "nan", "resources"):
            data = copy.deepcopy(original)
            point = data[0]["observations"][0]["gpuBuffers"]
            if kind == "duplicate":
                point["entries"].append(copy.deepcopy(point["entries"][0]))
            elif kind == "sum":
                point["live_bytes"] += 1
            elif kind == "count":
                point["live_buffers"] += 1
            elif kind == "partial":
                data = [run for run in data if run["round"] != 3] + [
                    {"round": 3, "mode": "warm", "observations": []}]
            elif kind == "category":
                point["categories"]["projection"]["bytes"] += 1
            elif kind == "nan":
                point["live_bytes"] = float("nan")
            else:
                point["resources"]["packed_resident"] = False
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                gpu.metrics(data)

    def test_truncated_inventory_keeps_actual_aggregate_and_does_not_require_full_sum(self):
        data = runs([])
        for run in data:
            point = buffers()
            point.update(live_bytes=200, live_buffers=2, peak_live_bytes=200,
                         observed_bytes_total=200, observed_buffers_total=2, entries_truncated=1)
            point["categories"] = {}
            run["observations"] = [{"gpuBuffers": point}]
        result = gpu.metrics(data)
        self.assertEqual(result["browser-observed-gpu-buffer-live-peak"]["gpu-buffer-bytes"]["value"], 200)

    def test_invalid_durations_ordinals_and_report_counters_are_rejected(self):
        for kind in ("negative-duration", "overflow-duration", "nan", "negative-ordinal", "ordinal-bound", "invalid-count", "selection"):
            data = runs([sample(0, 1, 1), sample(1, 1, 2)])
            item = data[1]["gpuSamples"]["samples"][0]
            if kind == "negative-duration":
                item["measurement"]["duration_ms"] = -1
            elif kind == "nan":
                item["measurement"]["duration_ms"] = float("nan")
            elif kind == "overflow-duration":
                item["measurement"]["duration_ms"] = 1e308
            elif kind == "negative-ordinal":
                item["ordinal"] = -1
            elif kind == "ordinal-bound":
                item["ordinal"] = 2
            elif kind == "invalid-count":
                data[1]["gpuSamples"]["reports"][0]["invalid"] = True
            else:
                item["identity"]["selection"] = "2"
            with self.subTest(kind=kind), self.assertRaises(ValueError):
                gpu.metrics(data)

    def test_category_labels_cannot_overwrite_the_actual_buffer_total(self):
        data = runs([])
        for run in data:
            point = buffers()
            point["categories"] = {"live": {"bytes": 100, "buffers": 1}}
            point["entries"][0]["categories"] = ["live"]
            run["observations"] = [{"gpuBuffers": point}]
        with self.assertRaisesRegex(ValueError, "Colliding"):
            gpu.metrics(data)

    def test_four_through_twenty_rounds_aggregate_every_independent_round(self):
        for count in (4, 20):
            data = []
            for number in range(1, count + 1):
                run = copy.deepcopy(runs()[1])
                run["round"] = number
                run["gpuSamples"]["samples"][0]["measurement"]["duration_ms"] = number
                data.append(run)
            with self.subTest(rounds=count):
                result = gpu.metrics(data)
                self.assertEqual(result["browser-observed-gpu-raster-median"]["latency"]["value"],
                                 (count + 1) / 2 * 1_000_000)

    def test_at_least_three_rounds_and_unique_warm_snapshots_are_required(self):
        with self.assertRaisesRegex(ValueError, "three"):
            gpu.metrics(runs()[:4])
        data = runs()
        data.append(copy.deepcopy(data[1]))
        with self.assertRaisesRegex(ValueError, "warm"):
            gpu.metrics(data)


if __name__ == "__main__":
    unittest.main()
