use std::ops::DerefMut;

use crate::define_config_state;
use bevy::asset::Handle;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter, IntoEnumIterator, IntoStaticStr};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use crate::robot_vis::visuals::UrdfLinkMaterial;
use crate::robot_vis::RobotLinkMeshes;

define_config_state!(ConfRobotLinkForceUseLinkMaterial);

#[derive(Default, Debug, Hash, Eq, PartialEq, Clone, bevy::prelude::States, EnumIter, AsRefStr)]
pub enum RobotDisplayMeshType {
    #[default]
    Visual,
    Collision,
    None,
}

/// Show or hide the robot's collision meshes.
pub fn update_robot_link_meshes_visibilities(
    conf: Res<State<RobotDisplayMeshType>>,
    mut query: Query<(&RobotLinkMeshes, &mut Visibility)>,
) {
    let (desire_visual_mesh_visibility, desire_collider_mesh_visibility) = match conf.get() {
        RobotDisplayMeshType::Visual => (Visibility::Inherited, Visibility::Hidden),
        RobotDisplayMeshType::Collision => (Visibility::Hidden, Visibility::Inherited),
        RobotDisplayMeshType::None => (Visibility::Hidden, Visibility::Hidden),
    };

    for (mesh, mut visible) in query.iter_mut() {
        match mesh {
            RobotLinkMeshes::Visual => {
                *visible = desire_visual_mesh_visibility;
            }
            RobotLinkMeshes::Collision => {
                *visible = desire_collider_mesh_visibility;
            }
        }
    }
}

/// Force the robot's links to use the material specified in the URDF link file.
/// (sometimes there are meshes that have their own material, and we prioritize that by default)
pub fn update_robot_link_materials(
    conf: Res<State<ConfRobotLinkForceUseLinkMaterial>>,
    mut query: Query<(&UrdfLinkMaterial, &mut Handle<StandardMaterial>)>,
) {
    for (link_material_container, mut handle) in query.iter_mut() {
        match (
            conf.get(),
            &link_material_container.from_inline_tag,
            &link_material_container.from_mesh_component,
        ) {
            (ConfRobotLinkForceUseLinkMaterial::On, Some(inline_material), _) => {
                *handle = inline_material.clone_weak();
            }
            (_, _, Some(mesh_material)) => {
                *handle = mesh_material.clone_weak();
            }
            (_, Some(inline_material), _) => {
                *handle = inline_material.clone_weak();
            }
            (_, None, None) => { /* do nothing */ }
        }
    }
}
