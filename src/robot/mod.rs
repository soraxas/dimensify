

use bevy::{
    app::{App, Startup},
    math::sampling::standard,
};
use eyre::Context;

use std::{collections::HashMap, f32::consts::*, io};

use bevy::{
    core_pipeline::{
        fxaa::Fxaa,
        prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
    },
    ecs::{observer::TriggerTargets, system::SystemId},
    pbr::{
        CascadeShadowConfigBuilder, DefaultOpaqueRendererMethod, DirectionalLightShadowMap,
        NotShadowCaster, NotShadowReceiver, OpaqueRendererMethod,
    },
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
        texture::ImageLoaderSettings,
    },
};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use urdf_rs::{Geometry, Pose, Robot};

use crate::assets_loader::urdf::UrdfAsset;

use super::assets_loader::{self, rgba_from_visual};

// use super::assets_loader::{self, rgba_from_visual};

use k;


pub mod visuals;
pub mod sync_state;




pub fn plugin(app: &mut App) {

    let path = "assets/panda/urdf/panda_relative.urdf";

    app
    .add_plugins(visuals::mesh_loader_plugin)
    .add_plugins(sync_state::plugin)
        ;
}




#[derive(Component, Default)]
struct RobotRoot;

#[derive(Component, Default)]
struct RobotLink;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobotLinkMeshes {
    Visual,
    Collision,
}


#[derive(Resource, Debug)]
pub struct RobotState {
    pub urdf_robot: Robot,
    pub end_link_names: Vec<String>,
    pub is_collision: bool,
    pub disable_texture: bool,
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
