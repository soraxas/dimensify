"""Published BMF values must be finite, scoped and measured."""

import unittest

import perf_scenes as scenes


class SceneMeasures(unittest.TestCase):
    def test_custom_byte_and_count_metrics_allow_measured_zero_but_not_negative(self):
        scenes.validate_bmf(
            {
                "cache": {"cache-payload-bytes": {"value": 0}},
                "evictions": {"count": {"value": 0}},
            }
        )
        with self.assertRaises(ValueError):
            scenes.validate_bmf({"cache": {"cache-payload-bytes": {"value": -1}}})

    def test_bmf_rejects_nan_empty_and_unexpected_measures(self):
        for data in (
            {},
            {"case": {"latency": {"value": float("nan")}}},
            {"case": {"throughput": {"value": 1}}},
        ):
            with self.assertRaises(ValueError):
                scenes.validate_bmf(data)


if __name__ == "__main__":
    unittest.main()
