"""Worker sampling diagnostics never become thread CPU or frame latency."""

import copy
import unittest

import perf_profiles


def sample(round_number=1, multiplier=1):
    raw = {
        "startTime": 100,
        "endTime": 100 + 1000 * multiplier,
        "nodes": [
            {"id": 1, "callFrame": {"functionName": "decode"}, "children": [2]},
            {"id": 2, "callFrame": {"functionName": "(idle)"}},
        ],
        "samples": [1, 2],
        "timeDeltas": [100 * multiplier, 200 * multiplier],
    }
    return {
        "round": round_number,
        "schema": "dimensify-worker-sampled-profiles/1",
        "cohort": "worker-profile-diagnostic",
        "stopped": True,
        "exactWorkerThreadCpuMs": None,
        "samplingIntervalUs": 1000,
        "parentTerminationRequests": 1,
        "parentTerminationFlushTimeouts": 0,
        "sampleLimitReached": False,
        "errors": [],
        "workers": [
            {
                "targetId": "worker-one",
                "detached": True,
                "unflushedTail": True,
                "completionStatus": "unavailable; possibly cancelled",
                "segments": [
                    {
                        "profile": raw,
                        "summary": {
                            "samples": 999999999,
                            "sampledProfileTimeUs": 999999999,
                        },
                    }
                ],
            }
        ],
    }


def report():
    return {"workerProfiles": [sample(1, 1), sample(2, 3), sample(3, 9)]}


class ProfileMetrics(unittest.TestCase):
    def test_rebuilds_raw_samples_and_medians_by_independent_round(self):
        result = perf_profiles.metrics(report())
        self.assertEqual(
            result["browser-worker-profile-sampled-elapsed"],
            {"sampled-profile-time": {"value": 900000}},
        )
        self.assertEqual(
            result["browser-worker-profile-samples"], {"count": {"value": 2}}
        )
        self.assertEqual(
            result["browser-worker-profile-unflushed-tails"], {"count": {"value": 1}}
        )
        self.assertTrue(
            all(
                "cpu" not in key and "latency" not in metric
                for key, metric in result.items()
            )
        )

    def test_multiple_segments_share_one_worker_without_double_counting_workers(self):
        data = report()
        for profile in data["workerProfiles"]:
            second = copy.deepcopy(profile["workers"][0]["segments"][0])
            second["profile"]["startTime"] += 20000
            second["profile"]["endTime"] += 20000
            profile["workers"][0]["segments"].append(second)
        result = perf_profiles.metrics(data)
        self.assertEqual(
            result["browser-worker-profile-profiled-workers"]["count"]["value"], 1
        )
        self.assertEqual(result["browser-worker-profile-segments"]["count"]["value"], 2)
        self.assertEqual(result["browser-worker-profile-samples"]["count"]["value"], 4)

    def test_missing_repeats_profiles_duplicate_rounds_and_exact_cpu_claim_fail_closed(
        self,
    ):
        for data in (
            {},
            {"workerProfiles": []},
            {"workerProfiles": [sample(1), sample(2)]},
            {"workerProfiles": [sample(1), sample(2), sample(2)]},
        ):
            with self.subTest(data=data), self.assertRaises(ValueError):
                perf_profiles.metrics(data)
        for mutate in (
            lambda row: row.pop("exactWorkerThreadCpuMs"),
            lambda row: row.update(exactWorkerThreadCpuMs=0),
            lambda row: row.update(stopped=False),
            lambda row: row.update(parentTerminationFlushTimeouts=2),
            lambda row: row.update(parentTerminationRequests=True),
        ):
            data = report()
            mutate(data["workerProfiles"][0])
            with self.assertRaises(ValueError):
                perf_profiles.metrics(data)

    def test_raw_profile_integrity_rejects_nonfinite_negative_missing_or_unknown_samples(
        self,
    ):
        for mutate in (
            lambda raw: raw.update(samples=[1]),
            lambda raw: raw.update(timeDeltas=[-1, 200]),
            lambda raw: raw.update(timeDeltas=[float("nan"), 200]),
            lambda raw: raw.update(endTime=float("inf")),
            lambda raw: raw.update(endTime=0),
            lambda raw: raw.update(samples=[1, 7]),
            lambda raw: raw["nodes"].append(copy.deepcopy(raw["nodes"][0])),
            lambda raw: raw["nodes"][0].update(children=[99]),
            lambda raw: raw.pop("samples"),
            lambda raw: raw.update(timeDeltas=[100, 100000]),
        ):
            data = report()
            mutate(data["workerProfiles"][0]["workers"][0]["segments"][0]["profile"])
            with self.assertRaises(ValueError):
                perf_profiles.metrics(data)

    def test_first_interval_can_straddle_profile_window_without_relaxing_later_deltas(
        self,
    ):
        raw = sample()["workers"][0]["segments"][0]["profile"]
        raw.update(startTime=100, endTime=200, timeDeltas=[500, 100])
        self.assertEqual(perf_profiles._raw_profile(raw), (2, 600))
        raw["timeDeltas"][1] = 101
        with self.assertRaises(ValueError):
            perf_profiles._raw_profile(raw)

    def test_duplicate_targets_and_overlapping_segments_fail_closed(self):
        data = report()
        worker = data["workerProfiles"][0]["workers"][0]
        data["workerProfiles"][0]["workers"].append(copy.deepcopy(worker))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            perf_profiles.metrics(data)
        data = report()
        worker = data["workerProfiles"][0]["workers"][0]
        worker["segments"].append(copy.deepcopy(worker["segments"][0]))
        with self.assertRaisesRegex(ValueError, "Overlapping"):
            perf_profiles.metrics(data)

    def test_missing_sample_coverage_keeps_counts_but_never_invents_zero_time(self):
        data = report()
        data["workerProfiles"][1]["workers"] = []
        result = perf_profiles.metrics(data)
        self.assertNotIn("browser-worker-profile-sampled-elapsed", result)
        for profile in data["workerProfiles"]:
            profile["workers"] = []
        result = perf_profiles.metrics(data)
        self.assertEqual(
            result["browser-worker-profile-profiled-workers"]["count"]["value"], 0
        )
        self.assertNotIn("browser-worker-profile-sampled-elapsed", result)

    def test_errors_timeouts_retention_limits_and_cancelled_tails_remain_observable(
        self,
    ):
        data = report()
        for profile in data["workerProfiles"]:
            profile.update(
                errors=["omitted worker"],
                sampleLimitReached=True,
                parentTerminationFlushTimeouts=1,
            )
            profile["workers"][0]["error"] = "worker detached before final flush"
        result = perf_profiles.metrics(data)
        for suffix, value in (
            ("errors", 2),
            ("parent-termination-flush-timeouts", 1),
            ("retention-limit-reached", 1),
            ("unflushed-tails", 1),
        ):
            self.assertEqual(
                result[f"browser-worker-profile-{suffix}"]["count"]["value"], value
            )


if __name__ == "__main__":
    unittest.main()
