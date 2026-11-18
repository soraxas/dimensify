//! Pivot inspect gestures on the splat under the pointer.
//!
//! Picking reads the rendered depth (see `dimensify_camera::viewer::DepthPickPlugin`).
//! Sorted splats blend without writing depth, so on frames with a pick request
//! the renderer redraws them depth-only for that camera
//! (`bevy_gaussian_splatting::render::depth_pass`). No splat data is copied to
//! the CPU; ordinary frames draw nothing extra.
use bevy::{
    core_pipeline::schedule::Core3d,
    prelude::*,
    render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems, sync_world::MainEntity},
};
use bevy_gaussian_splatting::render::depth_pass::{
    DEPTH_PASS_ALPHA, GaussianDepthPass, GaussianDepthPassSystems,
};
use dimensify_camera::{
    InspectEnvironment, InspectPick,
    viewer::{DepthPickFrame, DepthPickPlugin, DepthPickSystems, NavigationConfig, NavigationPick},
};

pub(super) fn install(app: &mut App) {
    install_picking(app);
    app.init_resource::<crate::effects::LoadEffectTarget>()
        .add_systems(
            Update,
            (
                sync_toggle.before(dimensify_camera::viewer::NavigationSystems),
                pick_effect_target.after(dimensify_camera::viewer::NavigationSystems),
            ),
        );
}

/// Answer a load effect's look-at request with the splat under the view
/// centre. The depth pass draws the rest pose, so held (hidden) splats are
/// still found. With picking off, the miss selects the bounds estimate.
fn pick_effect_target(
    target: Option<ResMut<crate::effects::LoadEffectTarget>>,
    mut pick: ResMut<NavigationPick>,
    cameras: Query<(&Camera, &GlobalTransform)>,
) {
    let Some(mut target) = target else {
        return;
    };
    if !target.wanted || target.answer.is_some() {
        return;
    }
    let view = pick.camera().and_then(|entity| cameras.get(entity).ok());
    let Some((camera, transform, centre)) = view.and_then(|(camera, transform)| {
        let centre = camera.logical_viewport_rect()?.center();
        Some((camera, transform, centre))
    }) else {
        target.answer = Some(None);
        return;
    };
    match pick.pick_hit(camera, transform, centre) {
        InspectPick::Hit(hit) => target.answer = Some(Some(hit.point)),
        InspectPick::Miss => target.answer = Some(None),
        InspectPick::Pending => {}
    }
}

/// Depth picking plus the splat depth pass, without viewer UI.
fn install_picking(app: &mut App) {
    app.add_plugins(DepthPickPlugin);
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app
        .configure_sets(
            Core3d,
            GaussianDepthPassSystems.in_set(DepthPickSystems::Contribute),
        )
        .add_systems(ExtractSchedule, extract_enabled)
        .add_systems(Render, request_views.before(RenderSystems::Queue));
}

/// Apply the settings toggle and `O` shortcut to navigation picking.
fn sync_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<NavigationConfig>,
    mut settings: ResMut<super::settings::ViewerSettings>,
    mut pick: ResMut<NavigationPick>,
) {
    if !config.blocked && keys.just_pressed(KeyCode::KeyO) {
        settings.pivot = !settings.pivot;
    }
    if pick.enabled != settings.pivot {
        pick.enabled = settings.pivot;
    }
}

/// Keep depth-pass pipelines warm only while picking is enabled, and apply the
/// viewer's opacity threshold. The WebGL2 renderer has no depth pass; its picks
/// miss and the camera pivots on the focus plane.
fn extract_enabled(
    pick: Extract<Option<Res<NavigationPick>>>,
    settings: Extract<Option<Res<super::settings::ViewerSettings>>>,
    pass: Option<ResMut<GaussianDepthPass>>,
) {
    let Some(mut pass) = pass else {
        return;
    };
    let enabled = pick.as_ref().is_some_and(|pick| pick.enabled);
    if pass.enabled != enabled {
        pass.enabled = enabled;
    }
    let alpha = settings.as_ref().map_or(DEPTH_PASS_ALPHA, |settings| {
        settings.pivot_opacity as f32 / 100.
    });
    if pass.alpha != alpha {
        pass.alpha = alpha;
    }
}

/// Draw splat depth only for the camera whose depth is copied this frame.
fn request_views(frame: Res<DepthPickFrame>, pass: Option<ResMut<GaussianDepthPass>>) {
    let Some(mut pass) = pass else {
        return;
    };
    pass.views.clear();
    pass.views.extend(frame.0.map(MainEntity::from));
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::{
        renderer::readiness::Snapshot,
        streaming::runtime::raster_handoff_tests::{
            capture, coloured_pixels, render_app_configured, until,
        },
    };
    use bevy::camera::primitives::Aabb;
    use bevy_gaussian_splatting::{
        CloudSettings, Gaussian3d, GaussianMode, GaussianTransparency, PlanarGaussian3d,
        PlanarGaussian3dHandle,
    };
    use dimensify_camera::{InspectEnvironment, InspectPick, viewer::ViewerCamera};

    #[test]
    #[ignore = "native offscreen GPU; run explicitly with --ignored --nocapture"]
    fn sorted_splats_are_pickable_without_changing_colour() {
        let hit = pick_sorted_splat(None, false).expect("sorted splat was not picked");
        println!("Splat pick hit {hit:?}");
        assert!(hit.truncate().length() < 0.1, "{hit:?}");
        assert!((hit.z + 2.).abs() < 0.1, "{hit:?}");
    }

    #[test]
    #[ignore = "native offscreen GPU; run explicitly with --ignored --nocapture"]
    fn pivot_opacity_threshold_above_the_splat_misses() {
        // The splat peaks at 90% opacity, so a 95% threshold writes no depth.
        assert_eq!(pick_sorted_splat(Some(95), false), None);
    }

    #[test]
    #[ignore = "native offscreen GPU; run explicitly with --ignored --nocapture"]
    fn picks_find_the_rest_surface_while_a_load_effect_hides_splats() {
        // A load-effect target is picked while the effect holds splats hidden.
        let hit = pick_sorted_splat(None, true).expect("hidden splat was not picked");
        assert!(hit.truncate().length() < 0.1, "{hit:?}");
        assert!((hit.z + 2.).abs() < 0.1, "{hit:?}");
    }

    /// Render one 90%-opaque sorted splat at z = -2 and pick the view centre.
    /// `opacity` sets the viewer threshold before the pipeline warms up.
    /// `hidden` holds an explosion at its first frame (no visible splats).
    fn pick_sorted_splat(opacity: Option<u32>, hidden: bool) -> Option<Vec3> {
        let (mut app, target, camera) = render_app_configured(32, install_picking);
        if let Some(opacity) = opacity {
            let mut settings = super::super::settings::ViewerSettings::default();
            settings.pivot_opacity = opacity;
            app.insert_resource(settings);
        }
        app.world_mut().entity_mut(camera).insert(ViewerCamera);
        app.world_mut()
            .resource_mut::<NavigationPick>()
            .set_camera(camera);

        let mut splat = Gaussian3d {
            position_visibility: [0., 0., -2., 1.].into(),
            rotation: [1., 0., 0., 0.].into(),
            scale_opacity: [0.3, 0.3, 0.3, 0.9].into(),
            ..default()
        };
        splat.spherical_harmonic.coefficients[0] = 1.5;
        let mut rows = vec![Gaussian3d::default(); 32];
        rows[0] = splat;
        let cloud = app
            .world_mut()
            .resource_mut::<Assets<PlanarGaussian3d>>()
            .add(rows);
        let entity = app
            .world_mut()
            .spawn((
                Snapshot,
                PlanarGaussian3dHandle(cloud),
                CloudSettings {
                    gaussian_mode: GaussianMode::Gaussian3d,
                    transparency: GaussianTransparency::Sorted,
                    ..default()
                },
                Transform::default(),
                Aabb::from_min_max(Vec3::splat(-4.), Vec3::splat(4.)),
            ))
            .id();
        until(&mut app, "splat renderer ready", |app| {
            app.world()
                .resource::<crate::renderer::readiness::ReadySnapshots>()
                .0
                .lock()
                .unwrap()
                .contains(&entity)
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let before = loop {
            let pixels = capture(&mut app, &target);
            if coloured_pixels(&pixels, 0) > 10 {
                break pixels;
            }
            assert!(std::time::Instant::now() < deadline, "splat must render");
        };
        let before = if hidden {
            app.world_mut()
                .get_mut::<CloudSettings>(entity)
                .unwrap()
                .effects = bevy_gaussian_splatting::SplatEffects {
                enabled: true,
                explosion: true,
                ..default()
            };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            loop {
                let pixels = capture(&mut app, &target);
                if coloured_pixels(&pixels, 0) == 0 {
                    break pixels;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "effect must hide splats"
                );
            }
        } else {
            before
        };

        let pick = |app: &mut App| {
            let world = app.world_mut();
            let (view, transform) = world
                .query::<(&Camera, &GlobalTransform)>()
                .get(world, camera)
                .map(|(c, t)| (c.clone(), *t))
                .unwrap();
            world
                .resource_mut::<NavigationPick>()
                .pick_hit(&view, &transform, Vec2::splat(32.))
        };
        assert!(matches!(pick(&mut app), InspectPick::Pending));
        // The depth pass runs on this captured frame; colour writes are masked.
        let during = capture(&mut app, &target);
        assert_eq!(before, during, "depth pass must not change colour");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match pick(&mut app) {
                InspectPick::Pending => app.update(),
                InspectPick::Miss => return None,
                InspectPick::Hit(hit) => return Some(hit.point),
            }
            assert!(std::time::Instant::now() < deadline, "pick never resolved");
        }
    }
}
