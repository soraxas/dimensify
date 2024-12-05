use bevy::app::App;
use bevy::prelude::*;

use std::collections::HashMap;

use bevy::prelude::*;
use urdf_rs::Robot;

use crate::util::coordinate_transform::FromBevySwapYZandFlipHandTrait;

use super::assets_loader::{self};

// use super::assets_loader::{self, rgba_from_visual};

use k::prelude::*;
use k::{self, JacobianIkSolver};

pub mod display_options;
pub mod show_colliding_link;
pub mod sync_state;
pub mod visuals;

pub fn plugin(app: &mut App) {
    app.register_type::<RobotState>()
        .add_plugins(visuals::mesh_loader_plugin)
        .add_plugins(show_colliding_link::plugin)
        .add_plugins(sync_state::plugin)
        .add_systems(Startup, test_startup)
        .add_systems(Update, test);
}

fn test_startup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
) {
    commands
        .spawn(PbrBundle {
            mesh: meshes.add(Cuboid::new(0.5, 0.35, 0.05).mesh()),
            // // mesh: meshes.add(Cuboid::new(0.5, 0.35, 0.05).mesh()),
            // material: materials.add(StandardMaterial {
            //     base_color: Color::srgba(0.8, 0.8, 0.98, 0.8),
            //     // base_color: Color::srgba(0.3, 0.3, 0.3, 0.8),
            //     base_color_texture: Some(images.add(event.img.clone())),
            //     alpha_mode: AlphaMode::Blend,
            //     // Remove this if you want it to use the world's lighting.
            //     unlit: true,
            //     ..default()
            // }),
            ..default()
        })
        .insert(Marker);
}

#[derive(Component)]
struct Marker;

// fn bevy_transform_to_k_isometry(transform: &Transform) -> k::Isometry3<f32> {

// }

fn test(mut q_robot_state: Query<&mut RobotState>, marker: Query<&Transform, With<Marker>>) {
    for mut robot_state in q_robot_state.iter_mut() {
        // println!("robot_state: {:?}", &robot_state.robot_chain);

        let mut solver = JacobianIkSolver::default();
        solver.allowable_target_distance = 0.1;
        solver.allowable_target_angle = 0.08;

        let constraints = k::Constraints {
            // rotation_x: false,
            // rotation_y: false,
            // rotation_z: false,
            // ignored_joint_names: opt.ignored_joint_names.clone(),
            ..Default::default()
        };

        let transform = marker.single();
        let target: k::Isometry3<f32> =
            k::Isometry3::<f32>::from_bevy_with_swap_yz_axis_and_flip_hand(transform);

        let nodes: Vec<_> = robot_state.robot_chain.iter().collect();

        // if let Some(arm) = k::SerialChain::try_new(robot_state.robot_chain.clone()) {
        let arm = k::SerialChain::from_end(nodes.last().unwrap());

        dbg!(&nodes.last());

        solver
            .solve_with_constraints(&arm, &target, &constraints)
            .unwrap_or_else(|err| {
                println!("Err: {err}");
            });

        robot_state.set_changed();
    }
}

#[derive(Component, Default)]
pub struct RobotRoot;

#[derive(Component, Default)]
pub struct RobotLink;

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
