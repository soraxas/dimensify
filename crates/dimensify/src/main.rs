use bevy::{log::LogPlugin, prelude::*};
use dimensify::{SimDevPlugin, SimPlugin, SimShowcasePlugin, graphics};
use eyre::Result;

use dimensify::{test_scene, util};

use bevy::log::DEFAULT_FILTER;

// const EXTRA_LOG_FILTER: &str = "naga=warn,bevy_render=info,bevy_ecs=trace,bevy=info";
const EXTRA_LOG_FILTER: [&str; 0] = [
    // "naga=warn",
    // "bevy_render=info",
    // "bevy_ecs=trace",
    // "bevy=info",
];

fn main() -> Result<()> {
    util::initialise()?;

    #[allow(unused_mut)] // we setup something else for wasm build
    let mut primary_window = Window {
        mode: bevy::window::WindowMode::Windowed,
        prevent_default_event_handling: false,
        fit_canvas_to_parent: true,
        title: "Dimensify".to_string(),
        present_mode: bevy::window::PresentMode::AutoVsync,
        ..default()
    };

    #[cfg(target_arch = "wasm32")]
    {
        primary_window.canvas = if cfg!(not(debug_assertions)) {
            Some("#bevy".to_string())
        } else {
            None
        };
        primary_window.prevent_default_event_handling = true;
    }

    let mut app = App::new();
    let mut filter = EXTRA_LOG_FILTER.to_vec();
    filter.push(DEFAULT_FILTER);
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(primary_window),
                ..default()
            })
            .set(LogPlugin {
                filter: filter.join(","),
                ..default()
            }),
    )
    .add_plugins(graphics::infinite_grid_plugin)
    .add_plugins(SimPlugin)
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
