//! Real deployed-engine observations, independent of our allocator/helpers.
//! Regenerate with scripts/sog-camera-reference-fixture.ts from the complete
//! Preserved reference-camera fixture from the baseline capture.

use super::*;
use bevy::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Capture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    camera_world: [f32; 16],
    world_from_local: [f32; 16],
    horizontal_fov_degrees: f32,
    aspect: f32,
    budget: usize,
    expected: Vec<usize>,
    displayed: Vec<usize>,
    distances: Vec<f64>,
    coverages: Vec<f64>,
}

fn capture() -> Capture {
    serde_json::from_slice(include_bytes!(
        "../fixtures/playcanvas-deployed-camera-lod.json"
    ))
    .expect("verified deployed-engine capture")
}

fn view(case: &Case) -> View<'_> {
    let camera_world = Mat4::from_cols_array(&case.camera_world);
    let local_from_world = Mat4::from_cols_array(&case.world_from_local).inverse();
    let tan_h = (case.horizontal_fov_degrees.to_radians() * 0.5).tan();
    View {
        eye: local_from_world.transform_point3(camera_world.w_axis.truncate()),
        forward: local_from_world
            .transform_vector3(-camera_world.z_axis.truncate())
            .normalize(),
        fov_scale: tan_h.min(tan_h / case.aspect) / std::f32::consts::FRAC_PI_8.tan(),
        clip_from_local: None,
        ..Vec3::ZERO.into()
    }
}

#[test]
fn moving_camera_targets_and_resident_ranges_match_deployed_viewer() {
    let manifest = Manifest::parse(include_bytes!("../fixtures/sanatorium-lod.json"))
        .expect("verified manifest");
    for case in capture().cases {
        let view = view(&case);
        let chosen = targets(&manifest, view, case.budget);
        let actual: Vec<_> = manifest
            .leaves
            .iter()
            .zip(chosen)
            .map(|(leaf, index)| leaf.lods[index.expect("all kitchen leaves have coverage")].0)
            .collect();
        assert_eq!(actual, case.expected, "{} target LODs", case.name);

        // With no missing source or admission pressure, compare the exact
        // ranges, not only counts or target indices. Normal cache ceilings
        // remain a separate stream/relocation acceptance test.
        let selected = super::super::select(&manifest, view, case.budget, usize::MAX, |_| true);
        for (id, (leaf, range)) in manifest.leaves.iter().zip(selected).enumerate() {
            let expected = leaf
                .lods
                .iter()
                .find(|(lod, _)| *lod == case.displayed[id])
                .map(|(_, range)| *range);
            assert_eq!(range, expected, "{} displayed range, leaf {id}", case.name);
        }
    }
}

#[test]
fn moving_camera_distance_and_lens_weights_match_actual_engine_observations() {
    let manifest = Manifest::parse(include_bytes!("../fixtures/sanatorium-lod.json"))
        .expect("verified manifest");
    for case in capture().cases {
        let view = view(&case);
        for (id, leaf) in manifest.leaves.iter().enumerate() {
            let distance_sq = super::super::super::distance::full_bounds_squared(
                leaf.bound,
                view.eye,
                view.forward,
            ) * f64::from(view.fov_scale).powi(2);
            let distance = distance_sq.sqrt();
            let expected = case.distances[id];
            assert!(
                (distance - expected).abs() <= (expected * 5e-5).max(1e-5),
                "{} leaf {id}: distance {distance} != {expected}",
                case.name
            );
            let coverage = (1.0 / distance_sq.max(1e-12)).max(1e-12);
            let expected = case.coverages[id];
            assert!(
                (coverage - expected).abs() <= (expected * 1e-4).max(1e-10),
                "{} leaf {id}: coverage {coverage} != {expected}",
                case.name
            );
        }
    }
}
