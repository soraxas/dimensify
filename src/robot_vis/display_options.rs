use bevy::asset::Handle;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;
use crate::robot_vis::visuals::UrdfLinkMaterial;
use crate::robot_vis::RobotLinkMeshes;

#[derive(Debug, Clone, PartialEq, Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
#[derive(Default)]
pub(crate) struct RobotShowColliderMesh {
    pub(crate) enabled: bool,
}

pub fn update_robot_link_meshes_visibilities(
    conf: Res<RobotShowColliderMesh>,
    mut query: Query<(&RobotLinkMeshes, &mut Visibility)>,
) {
    if !conf.is_changed() {
        return;
    }

    let (desire_visual_mesh_visibility, desire_collider_mesh_visibility) = if conf.enabled {
        (Visibility::Hidden, Visibility::Inherited)
    } else {
        (Visibility::Inherited, Visibility::Hidden)
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

#[derive(Debug, Clone, PartialEq, Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
#[derive(Default)]
pub(crate) struct RobotLinkForceUseLinkMaterial {
    pub(crate) enabled: bool,
}

pub fn update_robot_link_materials(
    conf: Res<RobotLinkForceUseLinkMaterial>,
    mut query: Query<(&UrdfLinkMaterial, &mut Handle<StandardMaterial>)>,
) {
    if !conf.is_changed() {
        return;
    }

    for (link_material_container, mut handle) in query.iter_mut() {
        match (
            conf.enabled,
            &link_material_container.from_inline_tag,
            &link_material_container.from_mesh_component,
        ) {
            (true, Some(inline_material), _) => {
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
