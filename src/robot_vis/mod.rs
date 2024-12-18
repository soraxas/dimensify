use bevy::app::App;
use bevy::prelude::*;

use std::collections::HashMap;

use urdf_rs::Robot;

use super::assets_loader::{self};

// use super::assets_loader::{self, rgba_from_visual};

pub mod display_options;
pub mod sync_state;
pub mod visuals;

pub fn plugin(app: &mut App) {
    app.register_type::<RobotState>()
        .add_plugins(visuals::mesh_loader_plugin)
        .add_plugins(sync_state::plugin)
        // .add_systems(Startup, test_startup)
        // .add_systems(Update, ControlRobot)
        // .add_systems(Update, update_desire_robot_state)
        ;
}

// fn bevy_transform_to_k_isometry(transform: &Transform) -> k::Isometry3<f32> {

// }

#[derive(Component, Default)]
pub struct RobotRoot;

#[derive(Component, Default)]
pub struct RobotLink {
    pub link_name: String,
    pub joint_name: String, // normally this refers to its parent joint
    pub node: Option<k::Node<f32>>,
}

impl RobotLink {
    pub fn link_name(&self) -> Option<String> {
        self.node
            .as_ref()
            .and_then(|node| node.link().as_ref().map(|link| link.name.clone()))
    }

    pub fn joint_name(&self) -> Option<String> {
        self.node.as_ref().map(|node| node.joint().name.clone())
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotLinkMeshes {
    Visual,
    Collision,
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
pub struct RobotState {
    #[reflect(ignore)]
    pub urdf_robot: Robot,
    pub end_link_names: Vec<String>,
    pub is_collision: bool,
    pub disable_texture: bool,
    #[reflect(ignore)]
    pub robot_chain: k::Chain<f32>,
    pub link_names_to_entity: HashMap<String, Entity>,
    pub joint_link_map: HashMap<String, String>,
}

impl RobotState {
    pub fn new(
        urdf_robot: Robot,
        end_link_names: Vec<String>,
        //
    ) -> Self {
        // let joint_link_map = k::urdf::joint_to_link_map(&urdf_robot);

        Self {
            joint_link_map: k::urdf::joint_to_link_map(&urdf_robot),
            robot_chain: urdf_robot.clone().into(),
            urdf_robot,
            end_link_names,
            is_collision: false,
            disable_texture: false,
            // link_joint_map: k::urdf::link_to_joint_map(&urdf_robot),
            link_names_to_entity: Default::default(),
        }
    }
}
