use bevy::prelude::*;

pub mod end_effector;
pub mod linear_joint_control;

pub fn plugin(app: &mut App) {
    app.register_type::<DesireRobotState>()
        .add_plugins(linear_joint_control::plugin)
        .add_plugins(end_effector::plugin);
}

#[derive(Component, Debug, Reflect)]
#[reflect(from_reflect = false)]
/// A component that represents a desired robot state.
/// This component can be used to interpolate the robot's
/// joint positions to a target state.
pub struct DesireRobotState {
    #[reflect(ignore)]
    pub ref_robot_state: k::Chain<f32>,
    pub target_robot_state: Option<Vec<f32>>,
    pub interpolation_speed: f32, // radians per second
    reached: bool,
}

impl DesireRobotState {
    pub fn new(ref_robot_state: k::Chain<f32>, target_robot_state: Option<Vec<f32>>) -> Self {
        Self {
            ref_robot_state,
            target_robot_state,
            interpolation_speed: std::f32::consts::FRAC_PI_2, // 2 PI radians per second
            // interpolation_speed: std::f32::consts::PI, // 2 PI radians per second
            reached: false,
        }
    }

    // pub fn from(robot_state: &RobotState) -> Self {
    //     Self::new(robot_state.robot_chain.iter().co, None)
    // }

    pub fn set_target(&mut self, target_robot_state: Vec<f32>) {
        // Set the target joint positions
        self.target_robot_state = Some(target_robot_state);
        self.reached = false;
    }

    pub fn update(&mut self, delta_seconds: f32) {
        if self.reached {
            return;
        }

        if let Some(target_robot_state) = &self.target_robot_state {
            // Get the current joint positions of the robot
            let mut current_positions = self.ref_robot_state.joint_positions();

            let mut all_reached = true;

            // Interpolate each joint position
            for (current_pos, target_pos) in
                current_positions.iter_mut().zip(target_robot_state.iter())
            {
                // Calculate the distance to the target
                let distance = target_pos - *current_pos;

                // If the distance is significant (greater than epsilon), interpolate
                if distance.abs() > f32::EPSILON {
                    // Calculate the interpolation step based on the distance
                    let step = distance.signum() * self.interpolation_speed * delta_seconds;

                    // Ensure we don't overshoot by limiting the step
                    if step.abs() > distance.abs() {
                        *current_pos = *target_pos; // Directly set to target if the step would overshoot
                    } else {
                        *current_pos += step; // Otherwise, apply the interpolation step
                        all_reached = false; // At least one joint hasn't reached the target yet
                    }
                }
            }
            // Set the updated joint positions back into the robot state
            self.ref_robot_state
                .set_joint_positions(&current_positions)
                .expect("error: set_joint_positions");

            self.reached = all_reached;
        }
    }
}
