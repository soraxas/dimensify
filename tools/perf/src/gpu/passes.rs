//! Collect bounded pass histories without merging distinct encoded segments.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct PassSamples {
    seen: BTreeSet<(String, u64)>,
    samples: BTreeMap<u64, Vec<Value>>,
}

impl PassSamples {
    pub fn collect(&mut self, report: &Value, ids: &BTreeSet<u64>) {
        let (Some(scope), Some(completed), Some(samples)) = (
            report["scope"].as_str(),
            report["completed"].as_u64(),
            report["samples"].as_array(),
        ) else {
            return;
        };
        let Some(first) = completed.checked_sub(samples.len() as u64) else {
            return;
        };
        for (index, sample) in samples.iter().enumerate() {
            let selection = sample["identity"]["selection"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok());
            let Some(selection) = selection.filter(|id| ids.contains(id)) else {
                continue;
            };
            if self.seen.insert((scope.to_owned(), first + index as u64)) {
                self.samples
                    .entry(selection)
                    .or_default()
                    .push(json!({"scope":scope,
                    "identity":sample["identity"],"measurement":sample["measurement"]}));
            }
        }
    }
    pub fn for_selection(&self, selection: u64) -> Vec<Value> {
        self.samples.get(&selection).cloned().unwrap_or_default()
    }
    pub fn durations_ns(&self) -> BTreeMap<String, Vec<f64>> {
        let mut output = BTreeMap::<String, Vec<f64>>::new();
        for samples in self.samples.values() {
            let mut frame = BTreeMap::<String, Option<f64>>::new();
            for sample in samples {
                let scope = sample["scope"].as_str().expect("collected scope");
                let value = sample["measurement"]["duration_ms"]
                    .as_f64()
                    .filter(|duration| {
                        duration.is_finite()
                            && *duration >= 0.0
                            && sample["measurement"]["failure"].is_null()
                    });
                let total = frame.entry(scope.to_owned()).or_insert(Some(0.0));
                *total = total
                    .zip(value)
                    .map(|(sum, value)| sum + value * 1_000_000.0)
                    .filter(|value| value.is_finite());
            }
            for (scope, value) in frame {
                if let Some(value) = value {
                    output.entry(scope).or_default().push(value);
                }
            }
        }
        output
    }

    pub fn coverage(&self, reports: &[Value], measured_frames: usize) -> Value {
        let durations = self.durations_ns();
        let mut coverage = serde_json::Map::new();
        for report in reports {
            let Some(scope) = report["scope"].as_str() else {
                continue;
            };
            let samples: Vec<_> = self
                .samples
                .values()
                .flatten()
                .filter(|sample| sample["scope"] == scope)
                .collect();
            let frames = self
                .samples
                .values()
                .filter(|samples| samples.iter().any(|sample| sample["scope"] == scope))
                .count();
            let valid = durations.get(scope).map_or(0, Vec::len);
            let reason = if report["capability"] != "timestamp_query" {
                Some("timestamp_query_unavailable")
            } else if frames == 0 {
                Some("no_attributed_eligible_pass_samples")
            } else if valid < measured_frames {
                Some("incomplete_or_invalid_pass_sample_coverage")
            } else {
                None
            };
            coverage.insert(scope.to_owned(),json!({"measured_frames":measured_frames,"frames_with_samples":frames,
                "frames_with_valid_segment_sum":valid,"segments":samples.len(),
                "invalid_segments":samples.iter().filter(|s|s["measurement"]["duration_ms"].as_f64().is_none_or(|n| !n.is_finite() || n<0.0) || !s["measurement"]["failure"].is_null()).count(),
                "unavailable_reason":reason}));
        }
        Value::Object(coverage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(selection: u64, duration: Value, failure: Value) -> Value {
        json!({"identity":{"selection":selection.to_string(),"frame":"17"},
            "measurement":{"duration_ms":duration,"failure":failure}})
    }
    fn report(scope: &str, completed: u64, samples: Vec<Value>) -> Value {
        json!({"scope":scope,"completed":completed,"samples":samples})
    }

    #[test]
    fn overlapping_completed_histories_are_idempotent_across_eviction() {
        let mut output = PassSamples::default();
        let ids = BTreeSet::from([1, 2, 3]);
        let first = report(
            "raster",
            2,
            vec![
                sample(1, json!(1), Value::Null),
                sample(2, json!(2), Value::Null),
            ],
        );
        output.collect(&first, &ids);
        output.collect(&first, &ids);
        output.collect(
            &report(
                "raster",
                3,
                vec![
                    sample(2, json!(2), Value::Null),
                    sample(3, json!(3), Value::Null),
                ],
            ),
            &ids,
        );
        assert_eq!(output.for_selection(1).len(), 1);
        assert_eq!(output.for_selection(2).len(), 1);
        assert_eq!(output.for_selection(3).len(), 1);
    }

    #[test]
    fn equal_identity_segments_survive_and_sum_only_within_scope() {
        let mut output = PassSamples::default();
        let ids = BTreeSet::from([4]);
        output.collect(
            &report(
                "raster",
                2,
                vec![
                    sample(4, json!(1), Value::Null),
                    sample(4, json!(2), Value::Null),
                ],
            ),
            &ids,
        );
        output.collect(
            &report("encode", 1, vec![sample(4, json!(5), Value::Null)]),
            &ids,
        );
        assert_eq!(output.for_selection(4).len(), 3);
        assert_eq!(output.durations_ns()["raster"], vec![3_000_000.0]);
        assert_eq!(output.durations_ns()["encode"], vec![5_000_000.0]);
    }

    #[test]
    fn delayed_samples_are_filtered_by_originating_selection() {
        let mut output = PassSamples::default();
        output.collect(
            &report(
                "raster",
                2,
                vec![
                    sample(8, json!(1), Value::Null),
                    sample(9, json!(2), Value::Null),
                ],
            ),
            &BTreeSet::from([8]),
        );
        assert_eq!(output.for_selection(8).len(), 1);
        assert!(output.for_selection(9).is_empty());
    }

    #[test]
    fn zero_is_valid_and_failed_segments_remain_raw_without_partial_totals() {
        let mut output = PassSamples::default();
        let invalid = sample(2, Value::Null, json!("unwritten_timestamp"));
        output.collect(
            &report(
                "raster",
                3,
                vec![
                    sample(1, json!(0), Value::Null),
                    sample(2, json!(1), Value::Null),
                    invalid.clone(),
                ],
            ),
            &BTreeSet::from([1, 2]),
        );
        assert_eq!(
            output.for_selection(2)[1]["measurement"],
            invalid["measurement"]
        );
        assert_eq!(output.durations_ns()["raster"], vec![0.0]);
    }
}
