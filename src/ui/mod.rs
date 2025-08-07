use bevy::{diagnostic::LogDiagnosticsPlugin, prelude::*};
use bevy_egui_notify::EguiToastsPlugin;

#[cfg(feature = "physics")]
pub(crate) mod rapier_debug_render;
pub(crate) mod showcase_window;

/// Plugin with debugging utility intended for use during development only.
/// Don't include this in a release build.
pub fn plugin(app: &mut App) {
    app.add_plugins((
        // FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin::filtered(vec![]),
        EguiToastsPlugin::default(),
        // bevy_rapier3d::render::RapierDebugRenderPlugin::default(),
    ))
    .add_plugins(bevy_egui::EguiPlugin::default());
    // .insert_gizmo_group(
    //     PhysicsGizmos {
    //         aabb_color: Some(Color::WHITE),
    //         ..default()
    //     },
    //     GizmoConfig {
    //         enabled: false,
    //         ..default()
    //     },
    // );
}
