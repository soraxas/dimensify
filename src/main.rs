use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy_web_asset::WebAssetPlugin;
use dimensify::graphics;
use dimensify::{SimDevPlugin, SimPlugin, SimShowcasePlugin};
use eyre::Result;

mod web_demo;

use dimensify::test_scene;
use dimensify::util;

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
        title: "RobotSim".to_string(),

        #[cfg(feature = "perftest")]
        present_mode: bevy::window::PresentMode::AutoNoVsync,
        #[cfg(not(feature = "perftest"))]
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

        title: "RobotSim".to_string(),
        present_mode: bevy::window::PresentMode::AutoVsync,
        ..default()
    });

    let mut app = App::new();
    app.add_plugins(WebAssetPlugin {
        cache_resource: true,
        reject_meta_request: true,
    })
    .add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window,
                ..default()
            })
            .set(LogPlugin {
                filter: "bevy_render=info,bevy_ecs=trace,bevy=info".to_string(),
                ..default()
            }),
    )
    .add_plugins(graphics::infinite_grid_plugin)
    .add_plugins(SimPlugin)
    .add_plugins(SimDevPlugin)
    .add_plugins(SimShowcasePlugin)
    .add_plugins(test_scene::plugin)
    .run();

    Ok(())
}
