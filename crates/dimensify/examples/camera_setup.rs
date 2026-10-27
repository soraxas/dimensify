//! Create a Dimensify camera without a window or GPU.
use bevy::prelude::*;
use dimensify::{DimensifyPlugins, camera::main_camera::MainCamera};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(DimensifyPlugins {
            infinite_grid: false,
            with_sun: false,
            with_ambient_light: false,
            with_ui: false,
        });
    app.update();
    let world = app.world_mut();
    let pose = world
        .query_filtered::<&Transform, With<MainCamera>>()
        .single(world)
        .expect("Dimensify supplies one primary camera");
    println!(
        "Primary camera at {:?}, facing {:?}",
        pose.translation,
        pose.forward()
    );
    println!("For two connected viewers, see docs/examples.md.");
}
