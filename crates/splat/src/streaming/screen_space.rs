//! Screen-space density heuristic, not measured reconstruction error.
use super::manifest::{Bounds, Manifest};
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub struct PixelView {
    pub clip_from_local: Mat4,
    pub viewport: Vec2,
}

/// Update per-leaf LOD targets using projected density and hysteresis.
pub fn targets(
    manifest: &Manifest,
    view: PixelView,
    target_pixels: f32,
    previous: &mut Vec<usize>,
) {
    let target = if target_pixels.is_finite() {
        target_pixels.clamp(0.5, 8.)
    } else {
        2.
    };
    let initial = previous.len() != manifest.leaves.len();
    previous.resize(manifest.leaves.len(), 0);
    for (leaf, index) in manifest.leaves.iter().zip(previous) {
        let Some(extent) = projected_extent(leaf.bound, view) else {
            *index = 0;
            continue;
        };
        let spacing = |i: usize| {
            leaf.lods.get(i).map_or(f32::INFINITY, |(_, range)| {
                if range.count == 0 {
                    f32::INFINITY
                } else {
                    extent / (range.count as f32).sqrt()
                }
            })
        };
        let coarsest = leaf.lods.len().saturating_sub(1);
        if initial {
            *index = (0..=coarsest)
                .rev()
                .find(|&i| spacing(i) <= target)
                .unwrap_or(0);
        } else {
            *index = (*index).min(coarsest);
            while *index > 0 && spacing(*index) > target * 1.2 {
                *index -= 1;
            }
            while *index < coarsest && spacing(*index + 1) <= target * 0.8 {
                *index += 1;
            }
        }
    }
}

/// Full projected span is deliberately not clipped to the viewport: clipping
/// would make a huge near-camera region appear artificially cheap to coarsen.
fn projected_extent(bound: Bounds, view: PixelView) -> Option<f32> {
    let matrix = view.clip_from_local;
    if !matrix.is_finite()
        || matrix.determinant() == 0.
        || !view.viewport.is_finite()
        || view.viewport.min_element() <= 0.
        || !bound.min.is_finite()
        || !bound.max.is_finite()
        || bound.min.cmpgt(bound.max).any()
    {
        return None;
    }
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { bound.min.x } else { bound.max.x },
            if i & 2 == 0 { bound.min.y } else { bound.max.y },
            if i & 4 == 0 { bound.min.z } else { bound.max.z },
        );
        let clip = matrix * corner.extend(1.);
        // Supports conventional and reverse-Z clip depth. Unknown/near-plane
        // intersections fail open rather than arbitrarily dropping detail.
        if !clip.is_finite() || clip.w <= 1e-6 || clip.z < 0. || clip.z > clip.w {
            return None;
        }
        let xy = clip.truncate().truncate() / clip.w;
        lo = lo.min(xy);
        hi = hi.max(xy);
    }
    let span = ((hi - lo) * 0.5 * view.viewport).max_element();
    (span.is_finite() && span > 0.).then_some(span)
}

#[cfg(test)]
mod tests {
    use super::super::manifest::{Bounds, Leaf, SplatRange};
    use super::*;

    fn manifest() -> Manifest {
        let bound = Bounds {
            min: Vec3::new(-1., -1., -6.),
            max: Vec3::new(1., 1., -4.),
        };
        Manifest {
            bound,
            filenames: vec![],
            environment: None,
            file_counts: vec![],
            leaves: vec![Leaf {
                bound,
                lods: [(0, 10_000), (2, 2_500), (5, 625)]
                    .into_iter()
                    .enumerate()
                    .map(|(file, (lod, count))| {
                        (
                            lod,
                            SplatRange {
                                file,
                                offset: 0,
                                count,
                            },
                        )
                    })
                    .collect(),
            }],
        }
    }

    fn ortho(span: f32, pixels: f32) -> PixelView {
        PixelView {
            clip_from_local: Mat4::orthographic_rh(
                -span / 2.,
                span / 2.,
                -span / 2.,
                span / 2.,
                0.1,
                1000.,
            ),
            viewport: Vec2::splat(pixels),
        }
    }

    #[test]
    fn zoom_out_coarsens_sparse_levels_and_zoom_in_refines() {
        let m = manifest();
        let mut history = vec![];
        for (span, expected) in [(2., 0), (5., 1), (20., 2), (2., 0)] {
            targets(&m, ortho(span, 200.), 2., &mut history);
            assert_eq!(history, [expected]);
        }
    }

    #[test]
    fn resolution_and_fov_change_detail_without_changing_budget() {
        let m = manifest();
        for (span, pixels, expected) in [(20., 200., 2), (20., 2000., 0)] {
            let mut history = vec![];
            targets(&m, ortho(span, pixels), 2., &mut history);
            assert_eq!(history, [expected]);
        }
        for (fov, expected) in [(90_f32, 2), (20., 0)] {
            let mut history = vec![];
            targets(
                &m,
                PixelView {
                    clip_from_local: Mat4::perspective_rh(fov.to_radians(), 1., 0.1, 1000.),
                    viewport: Vec2::splat(200.),
                },
                2.,
                &mut history,
            );
            assert_eq!(history, [expected]);
        }
    }

    #[test]
    fn hysteresis_retains_detail_through_boundary_jitter() {
        let m = manifest();
        let mut history = vec![1];
        // At span 4 the middle level's spacing is 2px. Oscillating around it
        // stays inside the 1.6-2.4px hysteresis band.
        for span in [4., 3.8, 4.2, 3.8] {
            targets(&m, ortho(span, 200.), 2., &mut history);
            assert_eq!(history, [1]);
        }
        targets(&m, ortho(3., 200.), 2., &mut history);
        assert_eq!(history, [0]);
    }

    #[test]
    fn camera_translation_and_scaled_scene_use_projected_size() {
        let m = manifest();
        let projection = Mat4::perspective_infinite_reverse_rh(1., 1., 0.1);
        let mut history = vec![];
        for (distance, expected) in [(0., 0), (100., 2), (0., 0)] {
            targets(
                &m,
                PixelView {
                    clip_from_local: projection
                        * Mat4::from_translation(Vec3::new(0., 0., -distance)),
                    viewport: Vec2::splat(1000.),
                },
                2.,
                &mut history,
            );
            assert_eq!(history, [expected]);
        }
        let view = ortho(20., 200.);
        targets(
            &m,
            PixelView {
                clip_from_local: view.clip_from_local * Mat4::from_scale(Vec3::new(10., 2., 1.)),
                ..view
            },
            2.,
            &mut history,
        );
        assert_eq!(history, [0], "non-uniform scale must affect pixel density");
    }

    #[test]
    fn invalid_view_or_eye_crossing_never_claims_detail_is_sufficient() {
        let m = manifest();
        for view in [
            PixelView {
                viewport: Vec2::ZERO,
                ..ortho(20., 200.)
            },
            PixelView {
                clip_from_local: Mat4::ZERO,
                ..ortho(20., 200.)
            },
            PixelView {
                clip_from_local: Mat4::perspective_rh(1., 1., 0.1, 1000.)
                    * Mat4::from_translation(Vec3::new(0., 0., 5.)),
                viewport: Vec2::splat(200.),
            },
        ] {
            let mut history = vec![2];
            targets(&m, view, 2., &mut history);
            assert_eq!(history, [0]);
        }
    }
}
