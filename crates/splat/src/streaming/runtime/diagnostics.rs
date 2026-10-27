//! Bounded, opt-in evidence from the production selection pass.

use super::manifest::{Bounds, SplatRange};
use bevy::prelude::*;

pub const MAX_LEAF_DIAGNOSTICS: usize = 256;

impl super::SogStream {
    fn range_resident(&self, range: SplatRange) -> bool {
        range.count == 0
            || self.pages.as_ref().map_or_else(
                || {
                    self.entries
                        .get(range.file)
                        .is_some_and(|entry| entry.ready.is_some())
                },
                |pages| pages.covers(range),
            )
    }

    /// Return the authored leaf count before diagnostic truncation.
    pub fn leaf_count(&self) -> usize {
        self.manifest.leaves.len()
    }

    /// Join selection evidence with live cache and renderer handoff state.
    /// This is opt-in and bounded; no attribute scans or GPU readback.
    pub fn leaf_snapshot(&self, limit: usize) -> Vec<SogLeafPipeline> {
        self.diagnostics
            .iter()
            .flatten()
            .take(limit.min(MAX_LEAF_DIAGNOSTICS))
            .map(|trace| {
                let leaf = self.manifest.leaves.get(trace.leaf);
                let in_ranges = |ranges: &[SplatRange]| {
                    leaf.and_then(|leaf| leaf.lods.iter().find(|(_, range)| ranges.contains(range)))
                        .and_then(|(lod, range)| u32::try_from(*lod).ok().map(|lod| (lod, *range)))
                };
                let selected = in_ranges(&self.active);
                let submitted = in_ranges(&self.submitted);
                let displayed = in_ranges(&self.displayed);
                let target = leaf.and_then(|leaf| {
                    leaf.lods
                        .iter()
                        .find(|(lod, _)| Some(*lod as u32) == trace.desired_lod)
                });
                let target_entry = target.and_then(|(_, range)| self.entries.get(range.file));
                let target_source_state = match target_entry {
                    _ if target.is_some_and(|(_, range)| range.count == 0) => "empty range",
                    _ if target.is_some_and(|(_, range)| self.range_resident(*range)) => "resident",
                    Some(entry) if entry.ready.is_some() && self.pages.is_some() => {
                        "extracting pages"
                    }
                    Some(entry) if entry.loading.is_some() => "loading/decode",
                    Some(entry) if !entry.can_retry() => "failed",
                    Some(entry) if entry.attempts > 0 => "retry pending",
                    Some(_) => "not requested",
                    None => "no target",
                };
                let finest_available_lod = leaf
                    .and_then(|leaf| leaf.lods.first())
                    .map(|(lod, _)| *lod as u32);
                let target_range = target.map(|(_, range)| *range);
                let refinement_reason = if trace.culled {
                    "culled"
                } else if target_range.is_none() {
                    "no target"
                } else if displayed.map(|(_, range)| range) == target_range {
                    if trace.desired_lod == finest_available_lod {
                        "finest available displayed"
                    } else {
                        "budgeted target displayed; finer LOD exists"
                    }
                } else if submitted.map(|(_, range)| range) == target_range {
                    "awaiting renderer handoff"
                } else if selected.map(|(_, range)| range) == target_range {
                    "awaiting renderer submission"
                } else if target_source_state == "failed" {
                    "target source failed after retries"
                } else if target_source_state == "extracting pages" {
                    "target source decoded; extracting pages"
                } else if target_source_state == "loading/decode" {
                    "target source loading/decode"
                } else if trace.reason == "source transition" {
                    if self.pages.is_some() {
                        "page source admission blocked"
                    } else {
                        "whole-file source admission blocked"
                    }
                } else if trace.reason == "device/slot cap" {
                    "selection capacity blocked"
                } else if target_source_state == "retry pending" {
                    "target source retry pending"
                } else if target_source_state == "resident" {
                    "target resident; awaiting selection"
                } else {
                    "target source not requested"
                };
                SogLeafPipeline {
                    selection: trace.clone(),
                    finest_available_lod,
                    eligible_lods: leaf.map_or_else(Vec::new, |leaf| {
                        leaf.lods.iter().map(|(lod, _)| *lod as u32).collect()
                    }),
                    refinement_reason,
                    resident_lod: leaf
                        .and_then(|leaf| {
                            leaf.lods
                                .iter()
                                .find(|(_, range)| self.range_resident(*range))
                        })
                        .map(|(lod, _)| *lod as u32),
                    selected_lod: selected.map(|(lod, _)| lod),
                    submitted_lod: submitted.map(|(lod, _)| lod),
                    displayed_lod: displayed.map(|(lod, _)| lod),
                    selected_range: selected.map(|(_, range)| range),
                    submitted_range: submitted.map(|(_, range)| range),
                    displayed_range: displayed.map(|(_, range)| range),
                    target_range: target.map(|(_, range)| *range),
                    target_file: target
                        .and_then(|(_, range)| self.manifest.filenames.get(range.file))
                        .cloned(),
                    target_source_state,
                }
            })
            .collect()
    }

    /// Add current activity without allocating UI reason strings or scanning
    /// decoded attribute arrays. Lifetime counters cannot identify pending work.
    pub fn accumulate_perf_activity(&self, activity: &mut crate::diagnostics::SogPerfActivity) {
        activity.active_streams += 1;
        activity.awaiting_first_evaluation += usize::from(self.generation == 0);
        activity.pending_downloads += self.entries.iter().filter(|e| e.loading.is_some()).count();
        activity.pending_generations +=
            usize::from(self.snapshot.is_pending() || self.needs_upload_frame());
        activity.refining_streams += usize::from(self.pending_work);
        activity.displayed_splats = self
            .displayed
            .iter()
            .fold(activity.displayed_splats, |n, range| {
                n.saturating_add(range.count)
            });
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SogLeafPipeline {
    #[serde(flatten)]
    pub selection: SogLeafDiagnostic,
    pub finest_available_lod: Option<u32>,
    /// Manifest levels retained after count-dominated levels are removed.
    /// Existence is not evidence of successfully decoded source data.
    pub eligible_lods: Vec<u32>,
    pub refinement_reason: &'static str,
    /// Best decoded representation, whether or not it fits the current selection.
    pub resident_lod: Option<u32>,
    pub selected_lod: Option<u32>,
    pub submitted_lod: Option<u32>,
    pub displayed_lod: Option<u32>,
    pub selected_range: Option<SplatRange>,
    pub submitted_range: Option<SplatRange>,
    pub displayed_range: Option<SplatRange>,
    pub target_range: Option<SplatRange>,
    pub target_file: Option<String>,
    pub target_source_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SogLeafDiagnostic {
    pub leaf: usize,
    pub projected_bounds: Option<[f32; 4]>,
    pub culled: bool,
    pub desired_lod: Option<u32>,
    pub chosen_lod: Option<u32>,
    pub range: Option<SplatRange>,
    pub reason: &'static str,
}

/// Project authored bounds into normalized clip coordinates.
pub(super) fn project(bound: Bounds, clip: Option<Mat4>) -> Option<[f32; 4]> {
    let clip = clip?;
    if !clip.is_finite()
        || clip.determinant() == 0.0
        || !bound.min.is_finite()
        || !bound.max.is_finite()
        || !bound.min.cmple(bound.max).all()
    {
        return None;
    }
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let point = Vec3::new(
            if i & 1 == 0 { bound.min.x } else { bound.max.x },
            if i & 2 == 0 { bound.min.y } else { bound.max.y },
            if i & 4 == 0 { bound.min.z } else { bound.max.z },
        );
        let p = clip * point.extend(1.0);
        if !p.is_finite() || p.w <= 1e-6 {
            return None;
        }
        let xy = p.truncate().truncate() / p.w;
        if !xy.is_finite() {
            return None;
        }
        lo = lo.min(xy);
        hi = hi.max(xy);
    }
    Some([lo.x, lo.y, hi.x, hi.y])
}

#[cfg(test)]
mod tests {
    use super::super::SogStream;
    use super::*;

    fn stream() -> SogStream {
        let mut stream = SogStream::parse(
            br#"{"lodLevels":6,"filenames":["fine/meta.json","coarse/meta.json"],"tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{"0":{"file":0,"offset":0,"count":8},"5":{"file":1,"offset":0,"count":2}}}}"#,
            "https://example.test/lod-meta.json", "test".into(), "test".into(),
        ).expect("manifest");
        let coarse = stream.manifest.leaves[0].lods[1].1;
        stream.active = vec![coarse];
        stream.submitted = vec![coarse];
        stream.displayed = vec![coarse];
        stream.diagnostics = Some(vec![SogLeafDiagnostic {
            leaf: 0,
            projected_bounds: None,
            culled: false,
            desired_lod: Some(5),
            chosen_lod: Some(5),
            range: Some(coarse),
            reason: "selected target",
        }]);
        stream
    }

    fn reason(stream: &SogStream) -> serde_json::Value {
        serde_json::to_value(stream.leaf_snapshot(1)).expect("snapshot")[0]["refinement_reason"]
            .clone()
    }

    #[test]
    fn reached_coarse_target_is_not_mistaken_for_missing_fine_data() {
        let mut stream = stream();
        let json = serde_json::to_value(stream.leaf_snapshot(1)).expect("snapshot");
        assert_eq!(json[0]["eligible_lods"], serde_json::json!([0, 5]));
        assert_eq!(
            reason(&stream),
            "budgeted target displayed; finer LOD exists"
        );
        stream.manifest.leaves[0].lods.remove(0);
        assert_eq!(reason(&stream), "finest available displayed");
    }

    #[test]
    fn refinement_reports_source_failure_and_renderer_handoff_separately() {
        let mut stream = stream();
        let trace = &mut stream.diagnostics.as_mut().expect("diagnostics")[0];
        trace.desired_lod = Some(0);
        trace.reason = "source transition";
        assert_eq!(reason(&stream), "whole-file source admission blocked");
        stream.entries[0].attempts = 3;
        assert_eq!(reason(&stream), "target source failed after retries");
        stream.entries[0].ready = Some(crate::renderer::DecodedChunk {
            cloud: default(),
            count: 8,
        });
        let fine = stream.manifest.leaves[0].lods[0].1;
        stream.active = vec![fine];
        stream.submitted = vec![fine];
        assert_eq!(reason(&stream), "awaiting renderer handoff");
        stream.displayed = vec![fine];
        assert_eq!(reason(&stream), "finest available displayed");
        stream.diagnostics.as_mut().expect("diagnostics")[0].culled = true;
        assert_eq!(reason(&stream), "culled");
    }
}
