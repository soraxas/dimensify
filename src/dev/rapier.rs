use crate::util::error;
use bevy::prelude::*;
use bevy_editor_pls::{editor::Editor, editor_window::EditorWindow, AddEditorWindow, EditorPlugin};
use bevy_egui::egui;
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use bevy_rapier3d::render::DebugRenderContext;
use eyre::{OptionExt, Result};
use serde::{Deserialize, Serialize};

use super::dev_editor::DevEditorWindow;

fn handle_debug_render(
    state: Res<Editor>,
    mut last_enabled: Local<bool>,
    // mut config_store: ResMut<GizmoConfigStore>,
    mut query: ResMut<DebugRenderContext>,
) -> Result<()> {
    let current_enabled = state
        .window_state::<DevEditorWindow>()
        .ok_or_eyre("Failed to read dev window state")?
        .collider_render_enabled;
    if current_enabled != *last_enabled {
        *last_enabled = current_enabled;

        query.enabled = current_enabled;
    }

    // let config = config_store.config_mut::<PhysicsGizmos>().0;
    // config.enabled = current_enabled;

    Ok(())
}

pub fn rapier_debug_plugin(app: &mut App) {
    app.add_plugins(EditorPlugin::new())
        .add_plugins((bevy_rapier3d::render::RapierDebugRenderPlugin::default(),))
        .add_systems(
            Update,
            (
                handle_debug_render.pipe(error),
                // set_cursor_grab_mode
            ),
        );
}
