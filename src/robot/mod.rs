use bevy::prelude::*;
use bevy::utils::{HashMap, HashSet};

pub mod visual;

pub fn plugin(app: &mut App) {
    app.add_plugins(visual::plugin)
        .register_type::<RobotLinkIsColliding>()
        .register_type::<HashSet<Entity>>()
        // .add_systems(Update, on_new_robot_root)


        ;
}

#[derive(Component, Reflect)]
pub struct RobotLinkIsColliding {
    pub entities: HashSet<Entity>,
}

// #[derive(Resource, Default)]
// struct RobotToCollisionChecker(HashMap<Entity, Robot>);
