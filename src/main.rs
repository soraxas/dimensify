use bevy::prelude::*;
use dimensify::SimPlugin;
use eyre::Result;

mod test_scene;
mod web_demo;

use dimensify::util;

fn main() -> Result<()> {
    util::initialise()?;

    let mut app = App::new();
    app.add_plugins(SimPlugin)
        .add_plugins(test_scene::plugin)
        .run();

    Ok(())
}
