use bevy::prelude::*;


use robotsim::{ robot};


mod web_demo;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Window {
                    title: "RobotSim".to_string(),
                    // title: "Bevy Rust Experiments".to_string(),
                    resizable: true,
                    // cursor_visible: true,
                    // present_mode: PresentMode::AutoVsync,
                    // This will spawn an invisible window
                    fit_canvas_to_parent: true, // no more need to handle this myself with wasm binding: https://github.com/bevyengine/bevy/commit/fed93a0edce9d66586dc70c1207a2092694b9a7d
                    canvas: Some("#bevy".to_string()),

                    // The window will be made visible in the make_visible() system after 3 frames.
                    // This is useful when you want to avoid the white window that shows up before the GPU is ready to render the app.
                    // visible: false,
                    ..default()
                }
                .into(),
                ..default()
            }),
        )
        .add_plugins(web_demo::plugin)
        .add_plugins(robot::plugin)
        .run();
}