use bevy::prelude::*;
use bevy::utils::{HashMap, HashSet};

pub mod control;
pub mod visual;

pub fn plugin(app: &mut App) {
    app.register_type::<RobotLinkIsColliding>()
        .register_type::<HashSet<Entity>>()
        // .add_systems(Update, on_new_robot_root)
        .add_plugins(visual::plugin)
        .add_plugins(control::plugin);
}

#[derive(Component, Reflect)]
pub struct RobotLinkIsColliding {
    pub entities: HashSet<Entity>,
}

// #[derive(Resource, Default)]
// struct RobotToCollisionChecker(HashMap<Entity, Robot>);
