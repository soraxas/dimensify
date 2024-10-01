#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::prelude::*;

use super::RobotRoot;

use k;

use crate::robot_vis::RobotLink;
use crate::robot_vis::RobotState;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, update_robot_visual);
}

fn update_robot_visual(
    mut robots: Query<&RobotState, (Changed<RobotState>, With<Children>, With<RobotRoot>)>,
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
