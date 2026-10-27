//! Select world and UI cameras independently in a headless application.
use bevy::prelude::*;
use dimensify_ui::{DimensifyUiPlugin, UiSpaceCamera, WorldSpaceCamera};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(DimensifyUiPlugin);
    // Markers express ownership; the application supplies rendering components.
    app.world_mut()
        .spawn((WorldSpaceCamera, Name::new("scene")));
    app.world_mut().spawn((UiSpaceCamera, Name::new("overlay")));
    app.update();
    let world = app.world_mut();
    for name in world
        .query_filtered::<&Name, With<WorldSpaceCamera>>()
        .iter(world)
    {
        println!("World camera: {name}");
    }
    for name in world
        .query_filtered::<&Name, With<UiSpaceCamera>>()
        .iter(world)
    {
        println!("UI camera: {name}");
    }
}
