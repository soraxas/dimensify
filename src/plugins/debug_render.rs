#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use crate::harness::Harness;
use crate::ui::main_ui::MainUiPainter;
use bevy::gizmos::gizmos::Gizmos;
use bevy::prelude::*;
use bevy_egui::egui::{CollapsingHeader, Ui};
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
}

impl MainUiPainter for DebugRenderData {
    fn draw(&mut self, ui: &mut Ui) {
        ui.separator();
        let debug_render_mode = &mut self.pipeline.mode;
        let mut clicked = None;
        {
            let response = ui.radio(
                !debug_render_mode.is_empty(),
                "Master Rigid-Body Physics switch",
            );
            if response.clicked() {
                clicked = Some(true);
                // if option is changed, update the debug render mode (all or nothing)
                *debug_render_mode = if debug_render_mode.is_empty() {
                    DebugRenderMode::all()
                } else {
                    DebugRenderMode::empty()
                };
            }
        }

        CollapsingHeader::new("Rigid-Body Physics Debug Rendering Option")
            .open(clicked)
            .show(ui, |content| {
                macro_rules! ui_flag_modify {
                    ($flag:expr, $desc:expr) => {
                        let mut is_on = debug_render_mode.contains($flag);
                        content.checkbox(&mut is_on, $desc);
                        debug_render_mode.set($flag, is_on);
                    };
                }

                ui_flag_modify!(DebugRenderMode::COLLIDER_SHAPES, "collider shapes");
                ui_flag_modify!(DebugRenderMode::RIGID_BODY_AXES, "rigid body axes");
                ui_flag_modify!(DebugRenderMode::MULTIBODY_JOINTS, "multibody joints");
                ui_flag_modify!(DebugRenderMode::IMPULSE_JOINTS, "impulse joints");
                ui_flag_modify!(DebugRenderMode::JOINTS, "joints");
                ui_flag_modify!(DebugRenderMode::SOLVER_CONTACTS, "solver contacts");
                ui_flag_modify!(DebugRenderMode::CONTACTS, "geometric contacts");
                ui_flag_modify!(DebugRenderMode::COLLIDER_AABBS, "collider aabbs");
            });

        ui.separator();
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
                        DebugRenderMode::empty(),
                    ),
                },
            ));
        })
        .add_systems(
            Update,
            debug_render_scene.run_if(|debug_render: Query<&DebugRenderData>| {
                !debug_render.single().pipeline.mode.is_empty()
            }),
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
