use bevy::prelude::*;

pub(crate) mod collision_colour;

#[cfg(feature = "physics")]
pub(crate) mod display_options;
#[cfg(feature = "physics")]
pub(crate) mod show_colliding_link;

pub fn plugin(_app: &mut App) {
    #[cfg(feature = "physics")]
    {
        app
            // colliding link point visualisation
            .add_plugins(collision_colour::plugin)
            // colloding colour
            .add_plugins(show_colliding_link::plugin);
    }
}
