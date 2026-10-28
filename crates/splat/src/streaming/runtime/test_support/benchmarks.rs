//! CPU benchmark fixtures. Transport and renderer completion are not simulated
//! performance measurements; the browser replay owns those observations.

use super::super::{SogStream, cache, chunk, relocation, selection};
use crate::streaming::{Manifest, PixelView, SogLodSettings, SplatRange, targets};
use bevy::prelude::*;
use bevy_gaussian_splatting::Gaussian3d;

/// Spatially varied, binary-tree hierarchy with 64 leaves per source file.
/// Counts are deliberately modest: this measures policy, not source decoding.
pub fn manifest_bytes(leaves: usize) -> Vec<u8> {
    assert!(leaves > 0);
    let side = (leaves as f64).cbrt().ceil() as usize;
    let groups = leaves.div_ceil(64);
    fn tree(start: usize, end: usize, side: usize, groups: usize) -> serde_json::Value {
        let bound = serde_json::json!({
            "min": [-(side as f32)-1., -(side as f32)-1., -2. * side as f32 - 5.],
            "max": [side as f32, side as f32, -2.]
        });
        if end - start > 1 {
            let middle = (start + end) / 2;
            return serde_json::json!({"bound":bound,"children":[
                tree(start, middle, side, groups), tree(middle, end, side, groups)
            ]});
        }
        let p = Vec3::new(
            (start % side) as f32 * 2. - side as f32,
            ((start / side) % side) as f32 * 2. - side as f32,
            -4. - (start / (side * side)) as f32 * 2.,
        );
        let mut lods = serde_json::Map::new();
        for (level, count) in [64, 16, 4].into_iter().enumerate() {
            lods.insert(
                level.to_string(),
                serde_json::json!({
                    "file": level * groups + start / 64,
                    "offset": start % 64 * count, "count": count
                }),
            );
        }
        serde_json::json!({"bound":{"min":(p-Vec3::splat(0.75)).to_array(),
            "max":(p+Vec3::splat(0.75)).to_array()},"lods":lods})
    }
    let filenames: Vec<_> = (0..3)
        .flat_map(|level| {
            (0..groups).map(move |group| format!("level-{level}/chunk-{group}/meta.json"))
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({"version":1,"lodLevels":3,
        "filenames":filenames,"tree":tree(0,leaves,side,groups)}))
    .unwrap()
}

fn view(frame: usize, leaves: usize) -> (Vec3, Vec3, PixelView) {
    let side = (leaves as f32).cbrt().ceil();
    let center = Vec3::new(0., 0., -side - 4.);
    let offsets = [
        Vec3::new(0., 0., 2.5 * side),
        Vec3::new(side, 0., side),
        Vec3::new(-side, 0., side),
        Vec3::new(0., side, side),
        Vec3::new(2.5 * side, 0., 0.),
        Vec3::new(-2.5 * side, 0., 0.),
        Vec3::new(0., 0., 1.2 * side),
        Vec3::new(0., 0., 2.5 * side),
    ];
    let eye = center + offsets[frame % offsets.len()];
    let forward = (center - eye).normalize();
    let projection = Mat4::perspective_infinite_reverse_rh(60_f32.to_radians(), 16. / 9., 0.01);
    (
        eye,
        forward,
        PixelView {
            clip_from_local: projection * Mat4::look_to_rh(eye, forward, Vec3::Y),
            viewport: Vec2::new(1280., 720.),
        },
    )
}

/// Full production screen-space targeting, culling and two-domain admission.
pub struct SelectionReplay {
    manifest: Manifest,
    quality: Vec<usize>,
    costs: Vec<usize>,
    cpu_costs: Vec<usize>,
    fixed: Vec<bool>,
    frame: usize,
    budget: usize,
    capacity: usize,
    cpu_capacity: usize,
}

impl SelectionReplay {
    pub fn new(leaves: usize) -> Self {
        let manifest = Manifest::parse(&manifest_bytes(leaves)).unwrap();
        let costs: Vec<_> = manifest
            .file_counts
            .iter()
            .map(|count| count * 32)
            .collect();
        let mut fixed = vec![false; costs.len()];
        for file in manifest.coarse_files() {
            fixed[file] = true;
        }
        let capacity = costs.iter().sum::<usize>() * 3 / 4;
        let cpu_costs: Vec<_> = manifest
            .file_counts
            .iter()
            .map(|count| cache::reserved_rows(*count) * chunk::DecodedChunk::bytes_per_splat())
            .collect();
        let cpu_capacity = cpu_costs.iter().sum::<usize>() * 2 / 3;
        Self {
            manifest,
            quality: Vec::new(),
            costs,
            fixed,
            frame: 0,
            budget: leaves * 24,
            capacity,
            cpu_costs,
            cpu_capacity,
        }
    }

    pub fn step(&mut self) -> Vec<Option<SplatRange>> {
        let (eye, forward, pixels) = view(self.frame, self.manifest.leaves.len());
        self.frame = (self.frame + 1) % 8;
        targets(&self.manifest, pixels, 2., &mut self.quality);
        let view = selection::View {
            eye,
            forward,
            fov_scale: 1.,
            clip_from_local: Some(pixels.clip_from_local),
            pixels: Some(pixels),
            quality: Some(&self.quality),
        };
        let admissions = [
            selection::SourceAdmission {
                costs: &self.costs,
                pinned: &self.fixed,
                capacity: self.capacity,
            },
            selection::SourceAdmission {
                costs: &self.cpu_costs,
                pinned: &self.fixed,
                capacity: self.cpu_capacity,
            },
        ];
        selection::select_with_summary(
            &self.manifest,
            view,
            self.budget,
            &admissions,
            |_| true,
            None,
            &mut selection::AdmissionSummary::default(),
        )
    }

    /// Validate outside the timed closure, including source-union accounting.
    pub fn validate(&self, ranges: &[Option<SplatRange>]) {
        assert_eq!(ranges.len(), self.manifest.leaves.len());
        assert!(ranges.iter().flatten().map(|r| r.count).sum::<usize>() <= self.budget);
        assert!(ranges.iter().flatten().next().is_some());
        let mut files = self.fixed.clone();
        for (leaf, range) in self.manifest.leaves.iter().zip(ranges) {
            if let Some(range) = range {
                assert!(leaf.lods.iter().any(|(_, r)| r == range));
                files[range.file] = true;
            }
        }
        assert!(
            files
                .iter()
                .zip(&self.costs)
                .filter(|(used, _)| **used)
                .map(|(_, cost)| cost)
                .sum::<usize>()
                <= self.capacity
        );
        assert!(
            files
                .iter()
                .zip(&self.cpu_costs)
                .filter(|(used, _)| **used)
                .map(|(_, cost)| cost)
                .sum::<usize>()
                <= self.cpu_capacity
        );
    }
}

#[derive(serde::Deserialize)]
struct Pressure {
    eye: Vec3,
    active: Vec<usize>,
    gpu_costs: Vec<usize>,
    gpu_budget: usize,
    environment_file: usize,
}

/// Replay the real whole-file relocation and covered bridge policy at a saved
/// pressure boundary. Each iteration starts from the same captured active set.
/// Controlled orientations are supplied; the capture has no original camera.
pub struct PressureReplay {
    manifest: Manifest,
    capture: Pressure,
    fixed: Vec<bool>,
    preferred: Vec<bool>,
    frame: usize,
    expected: Vec<Option<SplatRange>>,
}

impl Default for PressureReplay {
    fn default() -> Self {
        Self::new()
    }
}
impl PressureReplay {
    pub fn new() -> Self {
        let manifest = Manifest::parse(include_bytes!("../fixtures/sanatorium-lod.json")).unwrap();
        let capture: Pressure =
            serde_json::from_slice(include_bytes!("../fixtures/pressure-boundary.json")).unwrap();
        let mut fixed = vec![false; manifest.filenames.len()];
        for file in manifest.coarse_files() {
            fixed[file] = true;
        }
        let mut preferred = fixed.clone();
        for &file in &capture.active {
            // The environment is accounted separately from tree selection,
            // exactly as in SogStream::tick and the original pressure replay.
            if file != capture.environment_file {
                preferred[file] = true;
            }
        }
        Self {
            manifest,
            capture,
            fixed,
            preferred,
            frame: 0,
            expected: Vec::new(),
        }
    }

    fn capacity(&self) -> usize {
        self.capture.gpu_budget - self.capture.gpu_costs[self.capture.environment_file]
    }

    pub fn step(&mut self) -> (Vec<bool>, Vec<SplatRange>) {
        let forward = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ][self.frame];
        self.frame = (self.frame + 1) % 6;
        let up = if forward.dot(Vec3::Y).abs() > 0.9 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let pixels = PixelView {
            clip_from_local: Mat4::perspective_infinite_reverse_rh(
                75_f32.to_radians(),
                800. / 750.,
                0.01,
            ) * Mat4::look_to_rh(self.capture.eye, forward, up),
            viewport: Vec2::new(800., 750.),
        };
        let mut quality = Vec::new();
        targets(&self.manifest, pixels, 2., &mut quality);
        let view = selection::View {
            eye: self.capture.eye,
            forward,
            fov_scale: 1.,
            clip_from_local: Some(pixels.clip_from_local),
            pixels: Some(pixels),
            quality: Some(&quality),
        };
        let costs = &self.capture.gpu_costs[..self.manifest.filenames.len()];
        let admission = [selection::SourceAdmission {
            costs,
            pinned: &self.fixed,
            capacity: self.capacity(),
        }];
        let next = relocation::plan_files(
            &self.manifest,
            view,
            4_000_000 - 2783,
            &admission,
            &self.fixed,
            &self.preferred,
            |_| true,
        );
        let selected =
            relocation::select_files(&self.manifest, view, 4_000_000 - 2783, &admission, &next);
        let transition = [selection::SourceAdmission {
            costs,
            pinned: &self.preferred,
            capacity: self.capacity(),
        }];
        let bridge =
            relocation::covered_bridge(&self.manifest, &selected, &next, &transition, |file| {
                self.preferred[file]
            })
            .expect("coarse coverage is pinned");
        self.expected = selected;
        (next, bridge)
    }

    pub fn validate(&self, files: &[bool], bridge: &[SplatRange]) {
        assert_eq!(bridge.len(), self.expected.iter().flatten().count());
        for (leaf, target) in self.manifest.leaves.iter().zip(&self.expected) {
            if let Some(target) = target {
                let index = leaf
                    .lods
                    .iter()
                    .position(|(_, range)| range == target)
                    .unwrap();
                assert_eq!(
                    bridge
                        .iter()
                        .filter(|range| leaf.lods[index..]
                            .iter()
                            .any(|(_, candidate)| candidate == *range))
                        .count(),
                    1
                );
            }
        }
        assert!(
            files
                .iter()
                .zip(&self.capture.gpu_costs)
                .filter(|(used, _)| **used)
                .map(|(_, cost)| cost)
                .sum::<usize>()
                <= self.capacity()
        );
        assert!(
            bridge
                .iter()
                .all(|r| self.preferred[r.file] && files[r.file])
        );
        assert!(bridge.iter().map(|r| r.count).sum::<usize>() <= 4_000_000 - 2783);
        for range in bridge {
            assert_eq!(
                self.manifest
                    .leaves
                    .iter()
                    .filter(|leaf| leaf.lods.iter().any(|(_, r)| r == range))
                    .count(),
                1
            );
        }
    }
}

/// Actual cache admission with two obsolete sources to evict. Rebuild outside
/// measurement using Criterion's batched setup so each iteration does real work.
pub struct CacheReplay {
    entries: Vec<cache::Entry>,
    costs: Vec<usize>,
    wanted: Vec<bool>,
}
impl CacheReplay {
    pub fn new(entries: usize) -> Self {
        assert!(entries >= 4);
        let mut values: Vec<_> = (0..entries)
            .map(|i| cache::Entry {
                ready: Some(chunk::DecodedChunk {
                    cloud: bevy_gaussian_splatting::PlanarGaussian3d::default().into(),
                    count: 0,
                }),
                last_used: i as u64,
                ..Default::default()
            })
            .collect();
        values[entries - 1].ready = None;
        let mut wanted = vec![false; entries];
        wanted[0] = true;
        wanted[entries - 1] = true;
        Self {
            entries: values,
            costs: vec![1024; entries],
            wanted,
        }
    }

    pub fn admit(&mut self) -> (bool, usize) {
        let mut evicted = 0;
        let len = self.entries.len();
        let admitted = cache::admit_observed(
            &mut self.entries,
            &self.costs,
            &self.wanted,
            len - 1,
            (len - 2) * 1024,
            |_, _| evicted += 1,
        );
        (admitted, evicted)
    }

    pub fn pinned_is_ready(&self) -> bool {
        self.entries[0].ready.is_some()
    }
}

/// Production resident scheduler with sources already decoded. The renderer
/// acknowledgement is injected after each tick; transport and GPU work are
/// excluded. This measures policy/replacement CPU work, not full frame latency.
pub struct ResidentReplay {
    stream: SogStream,
    settings: SogLodSettings,
    frame: usize,
}
impl ResidentReplay {
    pub fn new(leaves: usize) -> Self {
        let mut stream = SogStream::parse(
            &manifest_bytes(leaves),
            "https://unused.invalid/lod-meta.json",
            "benchmark".into(),
            "benchmark".into(),
        )
        .unwrap();
        for (entry, &count) in stream.entries.iter_mut().zip(&stream.manifest.file_counts) {
            entry.ready = Some(chunk::DecodedChunk {
                cloud: vec![Gaussian3d::default(); cache::reserved_rows(count)].into(),
                count,
            });
        }
        Self {
            stream,
            settings: SogLodSettings {
                splat_budget: leaves * 24,
                cache_bytes: usize::MAX / 2,
                ..Default::default()
            },
            frame: 0,
        }
    }

    pub fn step(&mut self) -> usize {
        let (eye, forward, pixels) = view(self.frame, self.stream.manifest.leaves.len());
        self.frame += 1;
        let view = selection::View {
            eye,
            forward,
            fov_scale: 1.,
            clip_from_local: Some(pixels.clip_from_local),
            pixels: Some(pixels),
            quality: None,
        };
        self.stream.tick(
            view,
            self.frame as f64,
            &self.settings,
            self.settings.splat_budget,
            None,
            |_, _, _, _| panic!("resident fixture must never fetch"),
        );
        self.stream.submitted.clone_from(&self.stream.active);
        self.stream.displayed.clone_from(&self.stream.active);
        self.stream.active.iter().map(|r| r.count).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_selection_changes_with_perspective_and_respects_both_budgets() {
        let mut replay = SelectionReplay::new(1000);
        let mut history = Vec::new();
        for _ in 0..8 {
            let selected = replay.step();
            replay.validate(&selected);
            history.push(selected);
        }
        assert!(history.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn recorded_pressure_keeps_covered_ranges_and_a_fitting_destination() {
        let mut replay = PressureReplay::new();
        for _ in 0..6 {
            let (files, bridge) = replay.step();
            replay.validate(&files, &bridge);
        }
    }

    #[test]
    fn admission_evicts_obsolete_entries_but_preserves_the_pinned_source() {
        let mut replay = CacheReplay::new(1024);
        assert_eq!(replay.admit(), (true, 2));
        assert!(replay.pinned_is_ready());
    }

    #[test]
    fn resident_scheduler_moves_without_starting_network_requests() {
        let mut replay = ResidentReplay::new(1000);
        for _ in 0..8 {
            let selected = replay.step();
            assert!(selected > 0 && selected <= 24_000);
        }
    }
}
