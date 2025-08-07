use crate::define_config_state;

use crate::robot::{urdf_loader::UrdfLinkMaterial, RobotLinkMeshesType};

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use strum::{AsRefStr, EnumIter};
// use bevy_xpbd_3d::prelude::PhysicsGizmos;

define_config_state!(ConfRobotLinkForceUseLinkMaterial);

#[derive(Default, Debug, Hash, Eq, PartialEq, Clone, bevy::prelude::States, EnumIter, AsRefStr)]
pub enum RobotDisplayMeshType {
    #[default]
    Visual,
    Collision,
    None,
}

pub(crate) fn plugin(app: &mut App) {
    app.init_state::<RobotDisplayMeshType>()
        .init_state::<ConfRobotLinkForceUseLinkMaterial>()
        .add_systems(
            Update,
            update_robot_link_meshes_visibilities
                .run_if(on_event::<StateTransitionEvent<RobotDisplayMeshType>>),
        )
        .add_systems(
            Update,
            update_robot_link_materials
                .run_if(on_event::<StateTransitionEvent<ConfRobotLinkForceUseLinkMaterial>>),
        );
}

/// Show or hide the robot's collision meshes.
pub fn update_robot_link_meshes_visibilities(
    conf: Res<State<RobotDisplayMeshType>>,
    mut query: Query<(&RobotLinkMeshesType, &mut Visibility)>,
) {
    let (desire_visual_mesh_visibility, desire_collider_mesh_visibility) = match conf.get() {
        RobotDisplayMeshType::Visual => (Visibility::Inherited, Visibility::Hidden),
        RobotDisplayMeshType::Collision => (Visibility::Hidden, Visibility::Inherited),
        RobotDisplayMeshType::None => (Visibility::Hidden, Visibility::Hidden),
    };

    for (mesh, mut visible) in query.iter_mut() {
        match mesh {
            RobotLinkMeshesType::Visual => {
                *visible = desire_visual_mesh_visibility;
            }
            RobotLinkMeshesType::Collision => {
                *visible = desire_collider_mesh_visibility;
            }
        }
    }
}

/// Force the robot's links to use the material specified in the URDF link file.
/// (sometimes there are meshes that have their own material, and we prioritize that by default)
pub fn update_robot_link_materials(
    conf: Res<State<ConfRobotLinkForceUseLinkMaterial>>,
    mut query: Query<(&UrdfLinkMaterial, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    for (link_material_container, mut handle) in query.iter_mut() {
        match (
            conf.get(),
            &link_material_container.from_inline_tag,
            &link_material_container.from_mesh_component,
        ) {
            (ConfRobotLinkForceUseLinkMaterial::On, Some(inline_material), _) => {
                *handle = MeshMaterial3d(inline_material.clone_weak());
            }
            (_, _, Some(mesh_material)) => {
                *handle = MeshMaterial3d(mesh_material.clone_weak());
            }
            (_, Some(inline_material), _) => {
                *handle = MeshMaterial3d(inline_material.clone_weak());
            }
            (_, None, None) => { /* do nothing */ }
        }
    }
}
