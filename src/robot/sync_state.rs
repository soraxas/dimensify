#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::prelude::*;

use super::{RobotLink, RobotLinkTargetJointValue, RobotRoot, RobotState};

use k;

pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, update_requested_joint_values)
        .add_systems(Update, update_robot_meshes);
}

fn update_requested_joint_values(
    mut commands: Commands,
    mut q_robots: Query<(&mut RobotState, &Children), With<RobotRoot>>,
    mut q_links: Populated<(Entity, &mut RobotLink, &RobotLinkTargetJointValue)>,
) {
    for (mut robot_state, links_entities) in &mut q_robots {
        for link_entity in links_entities.iter() {
            if let Ok((entity, mut link, target_joint_value)) = q_links.get_mut(*link_entity) {
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
        }
    }
}

type ChanedRobotState<'a, 'b, 'c> =
    Query<'a, 'b, &'c RobotState, (Changed<RobotState>, With<Children>, With<RobotRoot>)>;

/// Update the mesh of the robot based on the current state of the robot.
fn update_robot_meshes(
    mut robots: ChanedRobotState,
    mut transform_query: Query<&mut Transform, With<RobotLink>>,
) {
    for robot_state in &mut robots {
        let kinematic: &k::Chain<f32> = &robot_state.robot_chain;

        kinematic.update_transforms();
        for link in kinematic.iter() {
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
