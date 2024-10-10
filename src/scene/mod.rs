use bevy::{ecs::system::SystemParam, log::tracing_subscriber::filter, prelude::*, utils::HashSet};
use bevy_editor_pls::{
    editor_window::{EditorWindow, EditorWindowContext},
    AddEditorWindow,
};
use bevy_rapier3d::prelude::*;

pub mod collidable;

pub fn plugin(app: &mut App) {
    app.add_plugins(RapierPhysicsPlugin::<collidable::IgnoredCollidersFilter>::default())
        .register_type::<collidable::IgnoredColliders>();
}
