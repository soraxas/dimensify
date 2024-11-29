use bevy::prelude::*;
use dimensify::SimPlugin;
use eyre::Result;

mod web_demo;

use dimensify::test_scene;
use dimensify::util;

fn main() -> Result<()> {
    util::initialise()?;

    let mut app = App::new();
    app.add_plugins(SimPlugin)
        .add_plugins(test_scene::plugin)
        .run();

    Ok(())
}
