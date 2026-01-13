use bevy::{log::LogPlugin, prelude::*};
use dimensify::{SimDevPlugin, SimPlugin, SimShowcasePlugin, graphics};
use eyre::Result;

mod web_demo;

use dimensify::{test_scene, util};

fn main() -> Result<()> {
    util::initialise()?;

    #[cfg(target_arch = "wasm32")]
    let primary_window = Some(Window {
        fit_canvas_to_parent: true,
        canvas: if cfg!(not(debug_assertions)) {
            Some("#bevy".to_string())
        } else {
            None
        },
        mode: bevy::window::WindowMode::Windowed,
        prevent_default_event_handling: true,
        title: "Dimensify".to_string(),
        present_mode: bevy::window::PresentMode::AutoVsync,

        ..default()
    });

    #[cfg(not(target_arch = "wasm32"))]
    let primary_window = Some(Window {
        mode: bevy::window::WindowMode::Windowed,
        prevent_default_event_handling: false,
        // resolution: (config.width, config.height).into(),
        resizable: true,
        // cursor_visible: true,
        // present_mode: PresentMode::AutoVsync,
        // This will spawn an invisible window
        fit_canvas_to_parent: true, // no more need to handle this myself with wasm binding: https://github.com/bevyengine/bevy/commit/fed93a0edce9d66586dc70c1207a2092694b9a7d

        title: "Dimensify".to_string(),
        present_mode: bevy::window::PresentMode::AutoVsync,
        ..default()
    });

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window,
                ..default()
            })
            .set(LogPlugin {
                filter: "naga=warn,wgpu_hal=warn,bevy_render=info,bevy_ecs=trace,bevy=info"
                    .to_string(),
                ..default()
            }),
    )
    .add_plugins(graphics::infinite_grid_plugin)
    .add_plugins(SimPlugin) // HEYYY this is making editor pls
    // plugin to disappear??
    .add_plugins(SimDevPlugin)
    .add_plugins(SimShowcasePlugin)
    .add_plugins(test_scene::plugin);

    #[cfg(feature = "dev")]
    {
        app.add_plugins(dimensify_ui::setup_ui);
    }

    app.run();

    Ok(())
}
