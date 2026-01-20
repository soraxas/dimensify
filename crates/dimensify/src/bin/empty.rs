use bevy::prelude::*;
use dimensify::DimensifyPlugin;
use eyre::Result;

use dimensify::util;

fn main() -> Result<()> {
    util::initialise()?;
    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins(DimensifyPlugin::default());

    app.run();

    Ok(())
}
