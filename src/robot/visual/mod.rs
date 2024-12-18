use bevy::prelude::*;

pub(crate) mod collision_colour;
pub(crate) mod display_options;
pub(crate) mod show_colliding_link;

pub fn plugin(app: &mut App) {
    app
        // colloding colour
        .add_plugins(collision_colour::plugin)
        // colliding link point visualisation
        .add_plugins(show_colliding_link::plugin);
}
