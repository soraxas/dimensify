use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use strum::{AsRefStr, EnumIter};

use crate::collision_checker;

pub mod collidable;

#[derive(Default, Debug, Hash, Eq, PartialEq, Clone, bevy::prelude::States, EnumIter, AsRefStr)]
pub enum PhysicsState {
    #[default]
    None,
    Dynamic,
}

pub fn plugin(app: &mut App) {
    // let mut config = RapierConfiguration::new(1.0); // default is 1.0 by default in 3D
    //                                                 // default disable the physics pipeline step simulation. We will manually call it when needed.
    // config.physics_pipeline_active = false;

    // FIXME the collidable is not working at the moment
    app //
        // .insert_resource(config)
        .init_state::<PhysicsState>()
        .add_plugins(collision_checker::plugin)
        .add_plugins(RapierPhysicsPlugin::<collidable::IgnoredCollidersFilter>::default())
        // .add_systems(
        //     PreUpdate,
        //     remove_physics_system.run_if(on_event::<StateTransitionEvent<PhysicsState>>),
        // )
        .register_type::<collidable::IgnoredColliders>();
}

// hard to sync up the physics state with the rapier physics plugin.
// not doing it for now.
fn remove_physics_system(
    mut command: Commands,
    physics_states: Res<State<PhysicsState>>,
    q_colliders: Query<Entity, With<Collider>>,
    q_rigidbodies: Query<Entity, With<RigidBody>>,
) {
    if *physics_states == PhysicsState::None {
        for entity in q_colliders.iter() {
            // command.entity(entity).remove::<Collider>();
            command
                .entity(entity)
                .remove::<RapierContextEntityLink>()
                .remove::<RapierColliderHandle>();
        }
        for entity in q_rigidbodies.iter() {
            command
                .entity(entity)
                .remove::<RapierContextEntityLink>()
                .remove::<RapierRigidBodyHandle>()
                .remove::<RigidBody>();
        }
    }
}
