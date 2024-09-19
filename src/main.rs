use bevy::prelude::*;
use eyre::Result;
use robotsim::SimPlugin;

mod web_demo;

use robotsim::util;

fn main() -> Result<()> {
    util::initialise()?;

    let mut app = App::new();
    app.add_plugins(SimPlugin).run();

    Ok(())
}
