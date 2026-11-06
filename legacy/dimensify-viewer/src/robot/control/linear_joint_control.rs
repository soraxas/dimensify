use bevy::prelude::*;

use crate::robot::RobotState;

use super::DesireRobotState;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, linear_joint_control);
}

/// A system that updates the desired robot state, based on the target joint positions
/// linearly interpolating the joint positions to the target.
pub fn linear_joint_control(
    mut q_desire_robot_state: Query<(&mut RobotState, &mut DesireRobotState)>,
    time: Res<Time>,
) {
    for (mut robot_state, mut desire_robot_state) in q_desire_robot_state.iter_mut() {
        // Update the desired robot state
        desire_robot_state.update(time.delta_secs());
        robot_state.set_changed();
    }
}
