//! Weak slot-content metadata. No extra working allocation, GPU readback or
//! source owner is retained. Copies always run before decoding and drawing.
use super::super::work::LocalView;
use super::*;
use std::{collections::HashMap, sync::Weak};

pub(super) struct CachedRange {
    source: Weak<WgpuWrapper<GpuSogSource>>,
    range: BakeRange,
    view: LocalView,
    travel: f32,
}

type Key = (usize, u32, u32);
fn key(source: *const WgpuWrapper<GpuSogSource>, range: BakeRange) -> Key {
    (source as usize, range.source_start, range.count)
}

struct CopyRange {
    work: Arc<WgpuWrapper<CompactWork>>,
    from: u32,
    to: u32,
    count: u32,
}

pub(super) struct Plan {
    pub contents: Vec<CachedRange>,
    full: Vec<usize>,
    color: Vec<usize>,
    copies: Vec<CopyRange>,
}

impl Plan {
    pub fn new(
        sources: &Sources,
        local: LocalView,
        previous: &BakeState,
        donor: Option<&BakeState>,
    ) -> Self {
        let mut result = Self {
            contents: Vec::with_capacity(sources.ranges.len()),
            full: Vec::new(),
            color: Vec::new(),
            copies: Vec::new(),
        };
        // Camera-only updates preserve ordered ranges. Avoid building identity
        // maps or cloning GPU handles on this per-frame path.
        if previous
            .previous
            .as_ref()
            .is_some_and(|(old, _)| std::ptr::eq(old.as_ptr(), sources))
        {
            for (index, (entry, old)) in sources.ranges.iter().zip(&previous.contents).enumerate() {
                result.remember(entry, Some(old), index, local, sources.color_refresh_ratio);
            }
            return result;
        }
        // Prefer in-place reuse. A previous state can also be a donor when a
        // caller carries its state across replacement allocations.
        let candidates: Vec<_> = [Some(previous), donor]
            .into_iter()
            .flatten()
            .filter_map(|state| state.work.upgrade().map(|work| (state, work)))
            .collect();
        let mut in_place = HashMap::new();
        let mut movable = HashMap::new();
        for (state, work) in &candidates {
            for old in &state.contents {
                let identity = key(old.source.as_ptr(), old.range);
                if Arc::ptr_eq(work, &sources.work) {
                    in_place.insert((identity, old.range.destination_start), old);
                } else {
                    movable.entry(identity).or_insert((old, work));
                }
            }
        }
        for (index, entry) in sources.ranges.iter().enumerate() {
            let identity = key(Arc::as_ptr(&entry.source), entry.range);
            let cached = if let Some(old) = in_place.get(&(identity, entry.range.destination_start))
            {
                Some(*old)
            } else if let Some((old, work)) = movable.get(&identity) {
                result.copies.push(CopyRange {
                    work: (*work).clone(),
                    from: old.range.destination_start,
                    to: entry.range.destination_start,
                    count: entry.range.count,
                });
                Some(*old)
            } else {
                None
            };
            result.remember(entry, cached, index, local, sources.color_refresh_ratio);
        }
        result
    }

    fn remember(
        &mut self,
        entry: &SourceRange,
        cached: Option<&CachedRange>,
        index: usize,
        local: LocalView,
        ratio: f32,
    ) {
        let mut travel = 0.0;
        if let Some(old) = cached {
            travel = local
                .translation_since(old.view)
                .map_or(f32::INFINITY, |d| old.travel + d);
            // Exact equality matters when the threshold is zero: a new
            // dense generation need not recolour an unchanged view.
            if entry.source.bands() > 0
                && travel > 0.0
                && (ratio == 0.0
                    || travel >= local.color_refresh_distance(entry.source.center_bounds, ratio))
            {
                self.color.push(index);
                travel = 0.0;
            }
        } else {
            self.full.push(index);
        }
        self.contents.push(CachedRange {
            source: Arc::downgrade(&entry.source),
            range: entry.range,
            view: local,
            travel,
        });
    }

    pub fn has_work(&self) -> bool {
        !(self.full.is_empty() && self.color.is_empty() && self.copies.is_empty())
    }

    pub fn copy(
        &self,
        target: &CompactWork,
        encoder: &mut wgpu::CommandEncoder,
        diagnostics: &BakeDiagnostics,
    ) {
        for copy in &self.copies {
            for ((from, to), stride) in copy
                .work
                .buffers()
                .into_iter()
                .zip(target.buffers())
                .zip([16, 4, 4])
            {
                encoder.copy_buffer_to_buffer(
                    from,
                    u64::from(copy.from) * stride,
                    to,
                    u64::from(copy.to) * stride,
                    u64::from(copy.count) * stride,
                );
            }
            diagnostics.record(|r| {
                r.copied_rows = r.copied_rows.saturating_add(u64::from(copy.count));
                r.copied_bytes = r.copied_bytes.saturating_add(u64::from(copy.count) * 24);
            });
        }
    }

    pub fn dispatches<'a>(
        &'a self,
        sources: &'a Sources,
        color_only: bool,
        max_count: u32,
    ) -> impl Iterator<Item = (&'a GpuSogSource, BakeRange)> {
        let indices = if color_only { &self.color } else { &self.full };
        indices.iter().flat_map(move |&index| {
            let entry = &sources.ranges[index];
            (0..entry.range.count)
                .step_by(max_count as usize)
                .map(move |offset| {
                    (
                        &**entry.source,
                        BakeRange {
                            source_start: entry.range.source_start + offset,
                            destination_start: entry.range.destination_start + offset,
                            count: (entry.range.count - offset).min(max_count),
                        },
                    )
                })
        })
    }
}
