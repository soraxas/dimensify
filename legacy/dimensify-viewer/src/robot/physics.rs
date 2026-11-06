use bevy_rapier3d::prelude::{Collider, ComputedColliderShape, RigidBody};
use std::collections::VecDeque;

use bevy::prelude::*;

use crate::util;

use super::{
    RobotLink, RobotLinkMeshesType, RobotRoot,
    urdf_loader::{RobotLinkPart, RobotLinkPartContainer},
};

pub fn process_rapier_component(
    mut command: Commands,
    meshes: Res<Assets<Mesh>>,
    q_robots: Populated<(Entity, &Children), (With<RobotRoot>, Without<RigidBody>)>,
    q_robot_links: Query<&Children, (With<RobotLink>, Without<Collider>)>,
    q_robot_link_meshes: Query<(&Children, &RobotLinkMeshesType)>,
    q_robot_link_part_containers: Query<(&Children, &Transform), With<RobotLinkPartContainer>>,
    q_robot_link_parts: Query<(&Mesh3d, &Transform), With<RobotLinkPart>>,
) {
    // TODO: make this configurable?
    // This represents the mesh that we will use for collision detection
    const COLLIDER_USE_MESH: RobotLinkMeshesType = RobotLinkMeshesType::Collision;

    // FOR EACH ROBOT
    for (robot_entity, robot_links) in q_robots.iter() {
        command.entity(robot_entity).insert(RigidBody::Dynamic);
        // FOR EACH LINK
        for robot_link_entity in robot_links {
            if let Ok(robot_link_meshes) = q_robot_links.get(*robot_link_entity) {
                // FOR EACH LINK MESH TYPE
                for (robot_link_part_containers, link_mesh_type) in robot_link_meshes
                    .iter()
                    .filter_map(|e| q_robot_link_meshes.get(*e).ok())
                {
                    if COLLIDER_USE_MESH != *link_mesh_type {
                        continue;
                    }

                    let mut collidable_containers = VecDeque::new();

                    // FOR EACH LINK PART CONTAINER
                    for (robot_link_parts, link_parts_transform) in robot_link_part_containers
                        .iter()
                        .filter_map(|e| q_robot_link_part_containers.get(*e).ok())
                    {
                        // FOR EACH LINK PART
                        for (link_part_mesh, entity_transform) in robot_link_parts
                            .iter()
                            .filter_map(|e| q_robot_link_parts.get(*e).ok())
                        {
                            if let Some(mesh) = meshes.get(Mesh3d(link_part_mesh.0.clone_weak())) {
                                let mesh = util::mesh_tools::mesh_with_transform(
                                    mesh,
                                    &link_parts_transform.mul_transform(*entity_transform),
                                )
                                .expect("Failed to create mesh with transform");

                                collidable_containers.push_back(mesh);
                            }
                        }
                    }

                    // if there is some collider meshes, we will merge all the meshes into one
                    if let Some(mut first) = collidable_containers.pop_front() {
                        // merge with all remaining meshes
                        while let Some(next) = collidable_containers.pop_front() {
                            // merge with next mesh
                            util::mesh_tools::mesh_append(&mut first, &next)
                                .expect("failed to append mesh");
                        }

                        match Collider::from_bevy_mesh(&first, &ComputedColliderShape::default()) {
                            Some(collider) => {
                                command.entity(*robot_link_entity).insert(collider);
                            }
                            None => {
                                error!("Failed to create collider for mesh");
                            }
                        }
                    }
                }
            }
        }
        // return child;
    }
}

// fn rsient(
//     q_robot: Query<(Entity, &UrdfLoadRequestParams), With<RobotRoot>>,
//     q_floor: Option<Single<Entity, With<FloorMarker>>>,
// ) -> Result<()> {
//     // now that we are done with creating all of the entities, we will deal with ignoring links that are next to each other,
//     // as well as user specified link pairs that allows to collide.

//     let mut link_name_to_collidable: HashMap<&str, Vec<Entity>> = HashMap::default();

//     let mut ignored_colliders: HashMap<&str, IgnoredColliders> = HashMap::new();
//     // add any user-provided ignored link pairs
//     for (a, b) in params.ignored_linkpair_collision.iter() {
//         // pair-wise ignore
//         let ignored = ignored_colliders.entry(a.as_str()).or_default();
//         {
//             if let Some(entities) = link_name_to_collidable.get(b.as_str()) {
//                 entities.iter().for_each(|e| ignored.add(*e))
//             } else {
//                 Err(UrdfAssetLoadingError::InvalidLinkPairToIgnore(a.clone()))?;
//             }
//         }

//         {
//             let ignored = ignored_colliders.entry(b.as_str()).or_default();
//             if let Some(entities) = link_name_to_collidable.get(a.as_str()) {
//                 entities.iter().for_each(|e| ignored.add(*e))
//             } else {
//                 Err(UrdfAssetLoadingError::InvalidLinkPairToIgnore(b.clone()))?;
//             }
//         }
//     }

//     // deal with fixed based
//     if params.fixed_base {
//         if let Some(floor_entity) = get_entity_by_name(&entities, SCENE_FLOOR_NAME) {
//             // find root link. Root link is one where it contains no parent.
//             let root_link_name = urdf_robot
//                 .links
//                 .iter()
//                 .filter_map(|l| {
//                     let name = &l.name;
//                     match mapping_child_to_parent.get(name.as_str()) {
//                         None => Some(name),
//                         _ => None,
//                     }
//                 })
//                 .next()
//                 .expect("cannot find root link");

//             // ignore floor collision for the fixed base
//             ignored_colliders
//                 .entry(root_link_name.as_str())
//                 .or_default()
//                 .add(floor_entity);

//             ignored_colliders
//                 .entry(root_link_name)
//                 .or_default()
//                 .add(floor_entity);
//         }
//     }

//     //
//     for link in urdf_robot.links.iter() {
//         let mut ignored = ignored_colliders
//             .remove(link.name.as_str())
//             .unwrap_or_default();
//         // let mut ignored = IgnoredColliders::default();

//         // ignore all link parent links by default
//         if let Some(parent_name) = mapping_child_to_parent.get(link.name.as_str()) {
//             link_name_to_collidable[parent_name]
//                 .iter()
//                 .for_each(|e| ignored.add(*e));
//         }
//         // ignore all link child links by default
//         if let Some(child_name) = mapping_parent_to_child.get(link.name.as_str()) {
//             link_name_to_collidable[child_name]
//                 .iter()
//                 .for_each(|e| ignored.add(*e));
//         }

//         // all collidable that belongs to this link should ignore all these mappings
//         for entity in &link_name_to_collidable[link.name.as_str()] {
//             let mut ignored = ignored.clone();

//             // also ignore every other entity-part within this link that is not this entity
//             link_name_to_collidable[link.name.as_str()]
//                 .iter()
//                 .filter(|e| e != &entity)
//                 .for_each(|e| ignored.add(*e));

//             commands
//                 .entity(*entity)
//                 // ignore these entities during collision detection
//                 .insert(ignored.clone())
//                 // add this flag to enable filter contact
//                 .insert(ActiveHooks::FILTER_CONTACT_PAIRS);
//         }
//     }
// }
