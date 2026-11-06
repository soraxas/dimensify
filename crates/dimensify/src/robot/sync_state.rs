#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.
/// Sync the robot state to the physical simulation.
use bevy::prelude::*;

use super::{RobotLink, RobotLinkTargetJointValue, RobotRoot, RobotState};

use k;

pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, sync_control_component_to_robot_state)
        .add_systems(
            PreUpdate,
            update_requested_joint_values.after(sync_control_component_to_robot_state),
        )
        .add_systems(
            Update,
            sync_robot_state_to_control_component.before(update_robot_meshes),
        )
        .add_systems(Update, update_robot_meshes);
}

fn update_requested_joint_values(
    mut commands: Commands,
    mut q_robots: Query<(&mut RobotState), With<RobotRoot>>,
    mut q_links: Populated<(Entity, &ChildOf, &mut RobotLink, &RobotLinkTargetJointValue)>,
) -> Result<()> {
    for (entity, child_of, mut link, target_joint_value) in &mut q_links {
        // for link_entity in links_entities.iter() {
        // println!("update_requested_joint_values: {:?}", link_entity);

        // let (entity, child_of, mut link, target_joint_value) =

        match q_robots.get_mut(child_of.parent()) {
            Ok(mut robot_state) => {
                println!("update_requested_joint_values: {:?}", target_joint_value.0);
                if let Some(node) = &mut link.node {
                    if node.set_joint_position(target_joint_value.0).is_err() {
                        error!("Failed to set joint position for: {:?}", node.joint().name);
                    }
                }
                // remove this after the joint is applied
                commands
                    .entity(entity)
                    .remove::<RobotLinkTargetJointValue>();

                robot_state.set_changed();
            }
            Err(_) => {
                error!(
                    "Failed to get parent robot state for: {:?}",
                    child_of.parent()
                );
            }
        }
    }
    // }
    Ok(())
}

type ChangedRobotState<'a, 'b, 'c> =
    Query<'a, 'b, &'c RobotState, (Changed<RobotState>, With<Children>, With<RobotRoot>)>;

/// Update the mesh of the robot based on the current state of the robot.
fn update_robot_meshes(
    mut robots: ChangedRobotState,
    mut transform_query: Query<&mut Transform, With<RobotLink>>,
) {
    for robot_state in &mut robots {
        let kinematic: &k::Chain<f32> = &robot_state.robot_chain;

        kinematic.update_transforms();
        println!("update_robot_meshes: {:?}", kinematic.joint_positions());
        for link in kinematic.iter() {
            println!("update_robot_meshes: {:?}", link.joint().name);
            let trans = link.world_transform().unwrap();
            let joint_name = &link.joint().name;
            let link_name = robot_state.joint_link_map.get(joint_name).unwrap();

            if let Some(id) = robot_state.link_names_to_entity.get(link_name) {
                if let Ok(mut transform) = transform_query.get_mut(*id) {
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
                }
            }
        }
    }
}

use bevy::platform::collections::HashMap;

type ChangedRobotStateWithRemote<'a, 'b, 'c> = Query<
    'a,
    'b,
    (&'c RobotState, &'c mut RemoteRobotState),
    (Changed<RobotState>, With<Children>, With<RobotRoot>),
>;

// a bevy ecs setter/getter for the robot state
#[derive(Component, Default, Reflect)]
pub struct RemoteRobotState(Vec<(String, f32)>);

fn sync_robot_state_to_control_component(mut robots: ChangedRobotStateWithRemote) -> Result<()> {
    for (robot_state, mut remote_robot_state) in &mut robots {
        // we bypass the change detection here to avoid infinite recursion
        let remote_robot_state = remote_robot_state.bypass_change_detection();

        let kinematic: &k::Chain<f32> = &robot_state.robot_chain;
        println!(
            "sync_robot_state_to_control_component: {:?}",
            kinematic.joint_positions()
        );
        // kinematic.

        if remote_robot_state.0.len() > 0 {
            continue;
        }

        for joint in kinematic.iter_joints() {
            let joint_name = &joint.name;

            match joint.joint_position() {
                Some(joint_position) => {
                    let joint_position = 5.;
                    remote_robot_state
                        .0
                        .push((joint_name.clone(), joint_position));
                    // .insert(joint_name.clone(), joint_position);
                }
                None => {
                    error!("Failed to get joint position for: {:?}", joint_name);
                }
            }
        }
    }
    Ok(())
}

use thiserror::Error;
#[derive(Error, Debug)]
pub enum SyncControlComponentToRobotStateError {
    #[error("Failed to map joint name '{0}' to link entity in joint_link_map")]
    FailedToMapJointNameToLinkEntity(String),
    #[error("Failed to get link entity for link name '{0}'")]
    FailedToGetLinkEntity(String),
    #[error("Failed to get joint position for joint name '{0}'")]
    FailedToGetJointPosition(String),
}

fn sync_control_component_to_robot_state(
    mut commands: Commands,
    mut q_robots: Query<
        (&RobotState, &Children, &mut RemoteRobotState),
        (Changed<RemoteRobotState>, With<Children>, With<RobotRoot>),
    >,
    mut q_links: Populated<Entity, With<RobotLink>>,
) -> Result<()> {
    for (robot_state, links_entities, remote_robot_state) in &mut q_robots {
        // go through the remote robot state
        for (joint_name, joint_position) in remote_robot_state.0.iter() {
            // find the link entity for the joint

            dbg!(joint_name, joint_position);

            let link_name = robot_state.joint_link_map.get(joint_name).ok_or_else(|| {
                SyncControlComponentToRobotStateError::FailedToMapJointNameToLinkEntity(
                    joint_name.clone(),
                )
            })?;
            let link_entity = robot_state
                .link_names_to_entity
                .get(link_name)
                .ok_or_else(|| {
                    SyncControlComponentToRobotStateError::FailedToGetLinkEntity(link_name.clone())
                })?;

            let link_entity = q_links.get(*link_entity).map_err(|_| {
                SyncControlComponentToRobotStateError::FailedToGetLinkEntity(link_name.clone())
            })?;

            commands
                .entity(link_entity)
                .insert(RobotLinkTargetJointValue(*joint_position));

            println!(
                "sync_control_component_to_robot_state: {:?}: {:?}",
                link_entity, joint_position
            );
        }
    }

    Ok(())
}
