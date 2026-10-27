//! Counterfactual replay of a recorded forced eviction boundary.
//! The capture lacks orientation/projection/quality inputs. Controlled views
//! below isolate fixed-input planner stability; they are not the browser path.
use super::*;
use bevy::prelude::*;

#[derive(serde::Deserialize)]
struct Boundary {
    eye: Vec3,
    active: Vec<usize>,
    desired: Vec<usize>,
    gpu_costs: Vec<usize>,
    gpu_budget: usize,
    environment_file: usize,
    victim: usize,
    eligible_victims: Vec<usize>,
    needed_bytes: usize,
}

fn mask(files: &[usize], len: usize) -> Vec<bool> {
    (0..len).map(|file| files.contains(&file)).collect()
}

fn files(mask: &[bool]) -> Vec<usize> {
    mask.iter()
        .enumerate()
        .filter_map(|(file, used)| used.then_some(file))
        .collect()
}

fn assert_leaf_coverage(
    manifest: &Manifest,
    targets: &[Option<SplatRange>],
    actual: &[SplatRange],
) {
    assert_eq!(targets.len(), manifest.leaves.len());
    let mut expected = 0;
    for (id, (leaf, target)) in manifest.leaves.iter().zip(targets).enumerate() {
        let target = target.expect("this fixture has nonempty coarse coverage for every leaf");
        let target_index = leaf.lods.iter().position(|(_, r)| *r == target).unwrap();
        assert!(target.count > 0);
        expected += 1;
        assert_eq!(
            actual
                .iter()
                .filter(|range| leaf.lods[target_index..].iter().any(|(_, r)| r == *range))
                .count(),
            1,
            "leaf {id} must have exactly one matching target-or-coarser range"
        );
    }
    assert_eq!(actual.len(), expected, "no extra or duplicate ranges");
}

// Catches a planner that oscillates at identical inputs or accepts a
// non-improving competing set. Transport and renderer acknowledgement are
// deliberately outside this final-destination/bridge boundary test.
#[test]
fn recorded_pressure_boundary_converges_with_frozen_view_and_costs() {
    let capture: Boundary =
        serde_json::from_str(include_str!("../fixtures/pressure-boundary.json")).unwrap();
    let manifest = Manifest::parse(include_bytes!("../fixtures/sanatorium-lod.json")).unwrap();
    let len = manifest.filenames.len();
    assert_eq!(capture.environment_file, len);
    assert_eq!(capture.gpu_costs.len(), len + 1);
    assert_eq!(capture.eligible_victims, [capture.victim]);
    assert!(capture.needed_bytes > capture.gpu_budget);
    let fixed = mask(&manifest.coarse_files(), len);
    // Environment stays outside tree selection, exactly as in SogStream::tick.
    let costs = &capture.gpu_costs[..len];
    let admission = [SourceAdmission {
        costs,
        pinned: &fixed,
        capacity: capture.gpu_budget - capture.gpu_costs[len],
    }];
    let budget = 4_000_000 - 2783; // Saved kitchen environment logical count.
    for forward in [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ] {
        let up = if forward.dot(Vec3::Y).abs() > 0.9 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let clip = Mat4::perspective_infinite_reverse_rh(75_f32.to_radians(), 800. / 750., 0.01)
            * Mat4::look_to_rh(capture.eye, forward, up);
        let pixels = super::super::screen_space::PixelView {
            clip_from_local: clip,
            viewport: Vec2::new(800., 750.),
        };
        let mut quality = Vec::new();
        super::super::screen_space::targets(&manifest, pixels, 2., &mut quality);
        let distance = View {
            forward,
            ..capture.eye.into()
        };
        for view in [
            distance,
            View {
                clip_from_local: Some(clip),
                pixels: Some(pixels),
                quality: Some(&quality),
                ..distance
            },
        ] {
            for seed in [&capture.active, &capture.desired] {
                let mut preferred = mask(seed, len);
                for (used, fixed) in preferred.iter_mut().zip(&fixed) {
                    *used |= fixed;
                }
                let prepared = PreparedSelection::new(&manifest, view, budget);
                let mut history = vec![preferred.clone()];
                let mut settled = false;
                for step in 0..16 {
                    let next = plan_files(
                        &manifest,
                        view,
                        budget,
                        &admission,
                        &fixed,
                        &preferred,
                        |_| true,
                    );
                    assert!(fits(&next, &admission));
                    if next == preferred {
                        settled = true;
                        break;
                    }
                    assert!(
                        !history.contains(&next),
                        "revisited a destination at a frozen view"
                    );
                    if fits(&preferred, &admission) {
                        let (gain, changed) = prepared.delta(&next, &preferred);
                        assert!(
                            gain > 0.
                                || (gain == 0.
                                    && next.iter().zip(&preferred).all(|(new, old)| !new || *old)),
                            "replacement must improve changed detail: {gain}/{changed}"
                        );
                    }
                    let target = select_files(&manifest, view, budget, &admission, &next);
                    let pins: Vec<_> = fixed
                        .iter()
                        .zip(&preferred)
                        .map(|(a, b)| *a || *b)
                        .collect();
                    let transition = [SourceAdmission {
                        costs,
                        pinned: &pins,
                        capacity: admission[0].capacity,
                    }];
                    let bridge =
                        covered_bridge(&manifest, &target, &next, &transition, |file| pins[file])
                            .expect("pinned coarse sources cover all requested leaves");
                    assert!(bridge.iter().all(|r| pins[r.file] && next[r.file]));
                    assert_leaf_coverage(&manifest, &target, &bridge);
                    // Ready and cold-but-retryable files are both available to
                    // this final-destination planner. Repeat with accepted intent
                    // preserved; transport completion itself is not simulated.
                    eprintln!(
                        "pressure replay forward={forward:?} screen={} step={step}: {:?} -> {:?}; gain={:?}; bridge_splats={}",
                        view.quality.is_some(),
                        files(&preferred),
                        files(&next),
                        prepared.delta(&next, &preferred),
                        bridge.iter().map(|r| r.count).sum::<usize>()
                    );
                    preferred = next;
                    history.push(preferred.clone());
                }
                assert!(
                    settled,
                    "fixed-input planner failed to converge in 16 passes"
                );
                let selected = select_files(&manifest, view, budget, &admission, &preferred);
                assert_leaf_coverage(
                    &manifest,
                    &selected,
                    &selected.iter().flatten().copied().collect::<Vec<_>>(),
                );
                assert!(selected.iter().flatten().map(|r| r.count).sum::<usize>() <= budget);
            }
        }
    }
}

/// This is not a GPU reload count: it isolates changing file intent despite
/// constant inputs. Publishing each intermediate plan can cause unnecessary
/// release/acquisition work when downloads and handoffs advance between passes.
#[test]
fn fixed_view_destination_does_not_drop_then_request_the_same_file() {
    let capture: Boundary =
        serde_json::from_str(include_str!("../fixtures/pressure-boundary.json")).unwrap();
    let manifest = Manifest::parse(include_bytes!("../fixtures/sanatorium-lod.json")).unwrap();
    let len = manifest.filenames.len();
    let fixed = mask(&manifest.coarse_files(), len);
    let admission = [SourceAdmission {
        costs: &capture.gpu_costs[..len],
        pinned: &fixed,
        capacity: capture.gpu_budget - capture.gpu_costs[len],
    }];
    let clip = Mat4::perspective_infinite_reverse_rh(75_f32.to_radians(), 800. / 750., 0.01)
        * Mat4::look_to_rh(capture.eye, Vec3::Y, Vec3::Z);
    let pixels = super::super::screen_space::PixelView {
        clip_from_local: clip,
        viewport: Vec2::new(800., 750.),
    };
    let mut quality = Vec::new();
    super::super::screen_space::targets(&manifest, pixels, 2., &mut quality);
    let view = View {
        forward: Vec3::Y,
        clip_from_local: Some(clip),
        pixels: Some(pixels),
        quality: Some(&quality),
        ..capture.eye.into()
    };
    let mut preferred = mask(&capture.desired, len);
    for (used, pin) in preferred.iter_mut().zip(&fixed) {
        *used |= pin;
    }
    let mut retired = vec![false; len];
    for pass in 0..16 {
        let next = plan_files(
            &manifest,
            view,
            4_000_000 - 2783,
            &admission,
            &fixed,
            &preferred,
            |_| true,
        );
        assert!(fits(&next, &admission));
        for file in 0..len {
            assert!(
                !(retired[file] && next[file]),
                "source {file} requested again on pass {pass} despite identical view, quality, costs and availability"
            );
            retired[file] |= preferred[file] && !next[file];
        }
        if next == preferred {
            return;
        }
        preferred = next;
    }
    panic!("planner did not converge");
}
