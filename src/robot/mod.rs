use bevy::prelude::*;
use bevy::utils::HashSet;
use std::collections::HashMap as StdHashMap;
use urdf_rs::Robot as UrdfRobot;

mod sync_state;

pub mod control;
pub mod urdf_loader;
pub mod visual;

pub fn plugin(app: &mut App) {
    app.register_type::<RobotState>()
        .register_type::<RobotLinkIsColliding>()
        .register_type::<RobotRoot>()
        .register_type::<RobotLink>()
        .register_type::<RobotLinkIsColliding>()
        .register_type::<RobotLinkMeshes>()
        .register_type::<HashSet<Entity>>()
        // .add_systems(Update, on_new_robot_root)
        .add_plugins(urdf_loader::plugin)
        .add_plugins(visual::plugin)
        .add_plugins(control::plugin)
        .add_plugins(sync_state::plugin);
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
pub struct RobotState {
    #[reflect(ignore)]
    pub urdf_robot: UrdfRobot,
    pub end_link_names: Vec<String>,
    pub is_collision: bool,
    pub disable_texture: bool,
    #[reflect(ignore)]
    pub robot_chain: k::Chain<f32>,
    pub link_names_to_entity: StdHashMap<String, Entity>,
    pub joint_link_map: StdHashMap<String, String>,
}

#[derive(Component, Default, Reflect)]
pub struct RobotRoot;

#[derive(Component, Default, Reflect)]
pub struct RobotLink {
    #[reflect(ignore)]
    node: Option<k::Node<f32>>,
}

#[derive(Component, Reflect)]
pub struct RobotLinkIsColliding {
    pub entities: HashSet<Entity>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Reflect)]
pub enum RobotLinkMeshes {
    Visual,
    Collision,
}

impl RobotState {
    pub fn new(
        urdf_robot: UrdfRobot,
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

impl RobotLink {
    pub fn new(node: Option<k::Node<f32>>) -> Self {
        match node {
            Some(node) => Self { node: Some(node) },
            None => Self { node: None },
        }
    }

    pub fn link_name(&self) -> Option<String> {
        self.node
            .as_ref()
            .and_then(|node| node.link().as_ref().map(|link| link.name.clone()))
    }

    /// This usually refers to the link's parent joint
    pub fn joint_name(&self) -> Option<String> {
        self.node.as_ref().map(|node| node.joint().name.clone())
    }
}

// #[derive(Resource, Default)]
// struct RobotToCollisionChecker(HashMap<Entity, Robot>);
