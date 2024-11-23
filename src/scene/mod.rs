use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use crate::collision_checker;

pub mod collidable;
pub mod objects;

pub fn plugin(app: &mut App) {
    let mut config = RapierConfiguration::new(1.0); // default is 1.0 by default in 3D
                                                    // default disable the physics pipeline step simulation. We will manually call it when needed.
    config.physics_pipeline_active = false;

    app //
        .insert_resource(config)
        .add_plugins(RapierPhysicsPlugin::<collidable::IgnoredCollidersFilter>::default())
        .add_plugins(collision_checker::plugin)
        .register_type::<collidable::IgnoredColliders>();
}
