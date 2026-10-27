use bevy::{diagnostic::LogDiagnosticsPlugin, platform::collections::HashSet, prelude::*};
#[cfg(feature = "protocol")]
use bevy_egui::EguiPrimaryContextPass;
// use bevy_egui_notify::EguiToastsPlugin;

#[cfg(feature = "physics")]
pub(crate) mod rapier_debug_render;
#[cfg(feature = "protocol")]
mod telemetry_timeline;
pub mod widgets;
use widgets::{WidgetCommandQueue, WidgetPanel, WidgetRegistry};

#[cfg(feature = "protocol")]
use widgets::WidgetStreamSettings;
// use bevy_editor_pls::EditorPlugin;

/// Plugin with debugging utility intended for use during development only.
/// Don't include this in a release build.
pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<bevy_egui::EguiPlugin>() {
        app.add_plugins(bevy_egui::EguiPlugin::default());
    }

    app.add_plugins((
        // FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin::filtered(HashSet::new()),
        // EguiToastsPlugin::default(),
        // bevy_rapier3d::render::RapierDebugRenderPlugin::default(),
    ))
    .init_resource::<WidgetRegistry>()
    .init_resource::<WidgetPanel>()
    // Register the built‑in widgets using the same mechanism that users can use.
    .add_systems(Startup, widgets::register_default_widgets);

    #[cfg(feature = "protocol")]
    app.add_systems(
        EguiPrimaryContextPass,
        telemetry_timeline::telemetry_timeline_ui,
    )
    // TODO: add back in when bevy_editor_pls is updated to use
    // newer bevy_egui version
    // .add_plugins(bevy_egui::EguiPlugin::default())
    ;

    #[cfg(feature = "protocol")]
    #[cfg(not(target_arch = "wasm32"))]
    {
        app.init_resource::<WidgetStreamSettings>();
        app.init_resource::<WidgetCommandQueue>();
        app.add_systems(Startup, widgets::load_widget_commands_from_source);
        app.add_systems(Update, widgets::apply_widget_commands);
    }

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
