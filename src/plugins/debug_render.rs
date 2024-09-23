#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use crate::harness::Harness;
use crate::ui::main_ui::MainUiPainter;
use bevy::gizmos::gizmos::Gizmos;
use bevy::prelude::*;
use bevy_egui::egui::Ui;
use bevy_trait_query::RegisterExt;
use rapier3d::math::{Point, Real};
use rapier3d::pipeline::{
    DebugRenderBackend, DebugRenderMode, DebugRenderObject, DebugRenderPipeline,
};

use super::DimensifyPlugin;

#[derive(Default)]
pub struct DebugRenderDimensifyPlugin {}

impl DimensifyPlugin for DebugRenderDimensifyPlugin {
    fn build_bevy_plugin(&mut self, app: &mut App) {
        app.add_plugins(plugin);
    }
}

#[derive(Component)]
struct DebugRenderData {
    pub pipeline: DebugRenderPipeline,
    pub enabled: bool,
}

impl MainUiPainter for DebugRenderData {
    fn draw(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.enabled, "debug render enabled");
    }
}

fn plugin(app: &mut App) {
    app
        // register the component to be query by the main painter
        .register_component_as::<dyn MainUiPainter, DebugRenderData>()
        .add_systems(Startup, |mut commands: Commands| {
            // insert the settings component
            commands.spawn((
                Name::new("MainUI:DebugRenderData"),
                DebugRenderData {
                    pipeline: DebugRenderPipeline::new(
                        Default::default(),
                        !DebugRenderMode::RIGID_BODY_AXES & !DebugRenderMode::COLLIDER_AABBS,
                    ),
                    enabled: false,
                },
            ));
        })
        .add_systems(
            Update,
            debug_render_scene
                .run_if(|debug_render: Query<&DebugRenderData>| debug_render.single().enabled),
        );
}

struct BevyLinesRenderBackend<'w, 's> {
    gizmos: Gizmos<'w, 's>,
}

impl<'w, 's> DebugRenderBackend for BevyLinesRenderBackend<'w, 's> {
    fn draw_line(&mut self, _: DebugRenderObject, a: Point<Real>, b: Point<Real>, color: [f32; 4]) {
        self.gizmos.line(
            [a.x as f32, a.y as f32, a.z as f32].into(),
            [b.x as f32, b.y as f32, b.z as f32].into(),
            Color::hsla(color[0], color[1], color[2], color[3]),
        )
    }
}

fn debug_render_scene(
    mut debug_render: Query<&mut DebugRenderData>,
    harness: Res<Harness>,
    gizmos: Gizmos,
) {
    let mut debug_render = debug_render.single_mut();
    let mut backend = BevyLinesRenderBackend { gizmos };
    debug_render.pipeline.render(
        &mut backend,
        &harness.physics.bodies,
        &harness.physics.colliders,
        &harness.physics.impulse_joints,
        &harness.physics.multibody_joints,
        &harness.physics.narrow_phase,
    );
}
