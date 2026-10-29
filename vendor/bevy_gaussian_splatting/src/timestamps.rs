//! Pass-descriptor boundaries shared by optional diagnostic consumers.

/// Reset/histogram (including enabled compaction) through the last radix digit.
/// Preceding buffer clears are outside this interval.
pub fn radix_timestamp_indices(
    begin: u32,
    end: u32,
    digit: Option<u32>,
    digits: u32,
) -> (Option<u32>, Option<u32>) {
    (
        digit.is_none().then_some(begin),
        digit
            .filter(|i| i.checked_add(1) == Some(digits))
            .map(|_| end),
    )
}

/// Descriptor only; the consumer owns allocation, identity and readback.
#[derive(Clone)]
pub struct GaussianTimestampInterval {
    pub query_set: wgpu::QuerySet,
    pub begin: u32,
    pub end: u32,
    /// Optional disjoint query indices for intermediate radix passes. Browser
    /// backends can lose the final write when merging untimed intervening passes.
    /// These guard writes are not separately reported or subtracted from duration.
    pub intermediate: Option<std::ops::Range<u32>>,
}

impl GaussianTimestampInterval {
    #[cfg(feature = "gpu_timing")]
    pub(crate) fn render_writes(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(self.begin),
            end_of_pass_write_index: Some(self.end),
        }
    }

    #[cfg(feature = "gpu_timing")]
    pub(crate) fn compute_writes(&self) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(self.begin),
            end_of_pass_write_index: Some(self.end),
        }
    }

    pub(crate) fn writes(
        &self,
        digit: Option<u32>,
        digits: u32,
    ) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let (mut beginning_of_pass_write_index, end_of_pass_write_index) =
            radix_timestamp_indices(self.begin, self.end, digit, digits);
        if beginning_of_pass_write_index.is_none() && end_of_pass_write_index.is_none() {
            beginning_of_pass_write_index = digit.and_then(|digit| {
                let range = self.intermediate.as_ref()?;
                let index = range.start.checked_add(digit)?;
                (index < range.end && index != self.begin && index != self.end).then_some(index)
            });
        }
        if beginning_of_pass_write_index.is_none() && end_of_pass_write_index.is_none() {
            return None;
        }
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index,
            end_of_pass_write_index,
        })
    }
}
