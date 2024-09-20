#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use crate::harness::Harness;
use bevy::gizmos::gizmos::Gizmos;
use bevy::prelude::*;
use bevy_egui::egui::{self, Ui};
use bevy_egui::EguiContexts;
use rapier3d::math::{Point, Real};
use rapier3d::pipeline::{
    DebugRenderBackend, DebugRenderMode, DebugRenderObject, DebugRenderPipeline,
};

use super::DimensifyPlugin;

#[derive(Default)]
pub struct DebugRenderDimensifyPlugin {}

impl DimensifyPlugin for DebugRenderDimensifyPlugin {
    fn build_bevy_plugin(&mut self, app: &mut App) {
        app.add_plugins(RapierDebugRenderPlugin::default());
        // app.add_systems(Update, self.ha);
    }
}

#[derive(Resource)]
pub struct DebugRenderPipelineResource {
    pub pipeline: DebugRenderPipeline,
    pub enabled: bool,
}

#[derive(Default)]
pub struct RapierDebugRenderPlugin {}

impl Plugin for RapierDebugRenderPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DebugRenderPipelineResource {
            pipeline: DebugRenderPipeline::new(
                Default::default(),
                !DebugRenderMode::RIGID_BODY_AXES & !DebugRenderMode::COLLIDER_AABBS,
            ),
            enabled: false,
        })
        .add_systems(Update, debug_render_scene)
        .add_systems(Update, debug_ui);
    }
}

fn debug_ui(mut ui_context: EguiContexts, mut debug_render: ResMut<DebugRenderPipelineResource>) {
    egui::Window::new("Debug Render").show(ui_context.ctx_mut(), |ui| {
        ui.checkbox(&mut debug_render.enabled, "debug render enabled");
    });
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
    mut debug_render: ResMut<DebugRenderPipelineResource>,
    harness: NonSend<Harness>,
    gizmos: Gizmos,
) {
    if debug_render.enabled {
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
}
