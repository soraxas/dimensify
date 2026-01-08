use bevy::{prelude::*, utils::HashSet};
use bevy_editor_pls::EditorPlugin;
use std::collections::HashMap as StdHashMap;
use urdf_rs::Robot as UrdfRobot;

#[cfg(feature = "physics")]
use crate::physics::PhysicsState;
#[cfg(feature = "physics")]
pub mod physics;

mod sync_state;

pub mod control;
pub(crate) mod editor_ui;

pub mod ui;
pub mod visual;

/// urdf loader is currently dependent on rapier3d's shape
/// (which isn't necessarily but for convenience)
/// So cannot use urdf loader without physics
#[cfg(feature = "physics")]
pub mod urdf_loader;

pub fn plugin(app: &mut App) {
    app.register_type::<RobotState>()
        .register_type::<RobotRoot>()
        .register_type::<RobotLink>()
        .register_type::<RobotLinkIsColliding>()
        .register_type::<RobotLinkMeshesType>()
        .register_type::<HashSet<Entity>>()
        // .add_systems(Update, on_new_robot_root)
        .add_plugins(visual::plugin)
        .add_plugins(control::plugin)
        .add_plugins(sync_state::plugin);

    #[cfg(feature = "physics")]
    {
        app.add_plugins(urdf_loader::plugin);

        app.add_systems(
            PreUpdate,
            physics::process_rapier_component.run_if(in_state(PhysicsState::Dynamic)),
        );
    }

    // app.add_systems(PostStartup, physics::spawn_rapier_component);
    // app.add_systems(PostStartup, physics::spawn_rapier_component);

    // TimestepMode::Fixed {
    //     dt: A * 10.,
    //     substeps: B * 10,
    // };

    // app.insert_resource(TimestepMode::Variable {
    //     max_dt: 1.0 / 60.0,
    //     time_scale: 1.0,
    //     substeps: 10,
    // });

    if app.is_plugin_added::<EditorPlugin>() {
        app.add_plugins(editor_ui::plugin);
    }
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
pub struct RobotState {
    #[reflect(ignore)]
    pub urdf_robot: UrdfRobot,
    pub end_link_names: Vec<String>,
    #[reflect(ignore)]
    pub robot_chain: k::Chain<f32>,
    pub link_names_to_entity: StdHashMap<String, Entity>,
    pub child_to_parent: StdHashMap<String, String>,
    pub joint_link_map: StdHashMap<String, String>,
}

#[derive(Component, Default, Reflect)]
#[require(Transform, Visibility)]
pub struct RobotRoot;

#[derive(Component, Default, Reflect)]
#[require(Transform, Visibility)]
pub struct RobotLink {
    #[reflect(ignore)]
    pub node: Option<k::Node<f32>>,
}

impl RobotLink {
    pub fn joint_limit(&self) -> Option<(f32, f32)> {
        self.node.as_ref().and_then(|node| {
            node.joint()
                .limits
                .as_ref()
                .map(|limits| (limits.min, limits.max))
        })
    }

    pub fn joint_value(&self) -> Option<f32> {
        self.node
            .as_ref()
            .map(|node| node.joint_position())
            .unwrap_or_default()
    }
}

#[derive(Component, Default, Reflect)]
pub struct RobotLinkTargetJointValue(pub f32);

#[derive(Component, Reflect)]
pub struct RobotLinkIsColliding {
    pub entities: HashSet<Entity>,
}

#[derive(Component, Debug, PartialEq, Eq, Hash, Copy, Clone, Reflect)]
#[require(Transform, Visibility)]
pub enum RobotLinkMeshesType {
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
            // link_joint_map: k::urdf::link_to_joint_map(&urdf_robot),
            link_names_to_entity: Default::default(),
            child_to_parent: Default::default(),
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
