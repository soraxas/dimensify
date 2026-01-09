use bevy::{diagnostic::LogDiagnosticsPlugin, platform::collections::HashSet, prelude::*};
use bevy_egui::EguiPrimaryContextPass;
// use bevy_egui_notify::EguiToastsPlugin;

#[cfg(feature = "physics")]
pub(crate) mod rapier_debug_render;
pub(crate) mod showcase_window;
pub mod widgets;
use widgets::{
    WidgetCommandQueue, WidgetPanel, WidgetRegistry, apply_widget_commands, register_demo_widgets,
    widget_registry_demo_ui,
};

#[cfg(feature = "protocol")]
use widgets::WidgetStreamSettings;
// use bevy_editor_pls::EditorPlugin;

/// Plugin with debugging utility intended for use during development only.
/// Don't include this in a release build.
pub fn plugin(app: &mut App) {
    app.add_plugins((
        // FrameTimeDiagnosticsPlugin,
        LogDiagnosticsPlugin::filtered(HashSet::new()),
        // EguiToastsPlugin::default(),
        // bevy_rapier3d::render::RapierDebugRenderPlugin::default(),
    ))
    .init_resource::<WidgetRegistry>()
    .init_resource::<WidgetCommandQueue>()
    .init_resource::<WidgetPanel>()
    .add_systems(Startup, register_demo_widgets)
    .add_systems(Update, apply_widget_commands)
    .add_systems(EguiPrimaryContextPass, widget_registry_demo_ui)
    // TODO: add back in when bevy_editor_pls is updated to use
    // newer bevy_egui version
    // .add_plugins(bevy_egui::EguiPlugin::default())
    ;

    #[cfg(feature = "protocol")]
    #[cfg(not(target_arch = "wasm32"))]
    {
        app.init_resource::<WidgetStreamSettings>();
        app.add_systems(Startup, widgets::load_widget_commands_from_source);
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
