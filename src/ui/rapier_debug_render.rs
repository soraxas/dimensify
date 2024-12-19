use bevy::prelude::*;
use bevy_editor_pls::{
    editor_window::{EditorWindow, EditorWindowContext},
    AddEditorWindow,
};
use bevy_rapier3d::prelude::*;
use egui::CollapsingHeader;

pub(crate) fn plugin(app: &mut App) {
    app.add_plugins(RapierDebugRenderPlugin::default().disabled())
        .add_editor_window::<RapierDebugEditorWindow>();
}

#[allow(unused)]
fn get_name_from_parents(
    entity: Entity,
    names: &Query<&Name>,
    parents: &Query<&Parent>,
) -> Option<String> {
    let mut parent = entity;
    while let Ok(parent_entity) = parents.get(parent) {
        if let Ok(name_component) = names.get(parent_entity.get()) {
            return Some(name_component.as_str().to_string());
        }
        parent = parent_entity.get();
    }
    None
}

pub(crate) struct RapierDebugEditorWindow;

impl EditorWindow for RapierDebugEditorWindow {
    type State = ();

    const NAME: &'static str = "Rapier Debug Render";
    // const DEFAULT_SIZE: (f32, f32) = (200., 150.);

    fn ui(world: &mut World, mut _cx: EditorWindowContext, ui: &mut egui::Ui) {
        if let Some(mut debug_context) = world.get_resource_mut::<DebugRenderContext>() {
            ui.checkbox(&mut debug_context.enabled, "Enable Debug Rendering");

            if !debug_context.enabled {
                ui.disable();
            }

            let debug_render_mode = &mut debug_context.pipeline.mode;
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
                .default_open(true)
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
}
