//! Distribution and BMF conversion shared by the native GPU runner.
use serde_json::{Value, json};

pub fn distribution(samples: &[f64]) -> Result<Value, String> {
    if samples.is_empty()
        || samples
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err("timing samples must be finite, nonnegative and nonempty".into());
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let count = sorted.len();
    let median = if count.is_multiple_of(2) {
        sorted[count / 2 - 1] / 2.0 + sorted[count / 2] / 2.0
    } else {
        sorted[count / 2]
    };
    let rank = |percent: usize| sorted[(count * percent).div_ceil(100) - 1];
    Ok(json!({"count":count,"median_ns":median,"p95_ns":rank(95),"p99_ns":rank(99)}))
}

pub fn bmf_metric(samples: &[f64]) -> Result<Value, String> {
    let summary = distribution(samples)?;
    Ok(json!({"latency":{"value":summary["median_ns"]}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_uses_nearest_rank_tails_and_even_median() {
        let samples: Vec<_> = (1..=100).rev().map(f64::from).collect();
        assert_eq!(
            distribution(&samples).unwrap(),
            json!({"count":100,"median_ns":50.5,"p95_ns":95.0,"p99_ns":99.0})
        );
    }

    #[test]
    fn bmf_preserves_nanoseconds_without_mislabeling_tails_as_confidence_bounds() {
        let metric = bmf_metric(&[1_000_000.0, 2_000_000.0, 3_000_000.0]).unwrap();
        assert_eq!(metric, json!({"latency":{"value":2_000_000.0}}));
    }

    #[test]
    fn invalid_or_empty_samples_cannot_be_published() {
        for values in [&[][..], &[f64::NAN], &[f64::INFINITY], &[-1.0]] {
            assert!(distribution(values).is_err());
        }
    }
}
