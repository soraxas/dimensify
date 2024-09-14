use bevy::{
    app::{App, Startup},
    math::sampling::standard, state::state,
};
use eyre::Context;

use std::{collections::HashMap, default, f32::consts::*, io};

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

use super::{assets_loader::{self, rgba_from_visual}, visuals};

// use super::assets_loader::{self, rgba_from_visual};
use bevy_asset_loader::prelude::*;

use k;



use crate::robot::RobotState;
use crate::robot::RobotLink;

pub fn plugin(app: &mut App) {

    let path = "assets/panda/urdf/panda_relative.urdf";

    app
    .add_systems(Update, update_robot_visual
        .run_if(in_state(visuals::UrdfLoadState::Next))
        // .run_if(in_state(RobotLinkVisualState::Outdated))
    )
        ;
}


fn update_robot_visual(
    robot_state: Res<RobotState>,
    mut query: Query<(&RobotLink, &mut Transform)>,

) {
    if !robot_state.is_changed() {
        return;
    }
    let robot_state = robot_state.into_inner();
    // return;
    let mut kinematic: &k::Chain<f32> = &robot_state.robot_chain;

    kinematic.update_transforms();
    for link in kinematic.iter() {
       let trans = link.world_transform().unwrap();
       let joint_name = &link.joint().name;
       let link_name = robot_state.joint_link_map.get(joint_name).unwrap();
       // let link_name = &link.joint().name;
       // let trans_f32: na::Isometry3<f32> = na::Isometry3::to_superset(&trans);

       dbg!(joint_name);
       dbg!(link_name);
    //    dbg!(&robot_state.link_names_to_entity);
       // robot_state.link_names_to_entity.get(link_name).unwrap();
       if let Some(id) = robot_state.link_names_to_entity.get(link_name) {
           // query.get_mut(*id).unwrap();
           query.get_mut(*id).unwrap();
           if let Ok((link, mut transform)) = query.get_mut(*id) {
               dbg!(&id);
            //    dbg!(&query);
               *transform = Transform {
                   translation: [
                       trans.translation.vector.x,
                       trans.translation.vector.y,
                       trans.translation.vector.z,
                   ]
                   .into(),
                   rotation: Quat::from_xyzw(
                          trans.rotation.i as f32,
                          trans.rotation.j as f32,
                       trans.rotation.k as f32,
                       trans.rotation.w as f32,
                   ),
                   ..Default::default()
               };
               dbg!(&transform);

               //  * transform = trans.into();
           }
       }
    }
}

