use bevy::prelude::*;
use k::{InverseKinematicsSolver, JacobianIkSolver};

use crate::{
    camera::window_camera::{build_camera_to_egui_img_texture, FloatingCamera},
    coordinate_system::prelude::*,
    robot_vis::RobotState,
};
use bevy_egui::EguiUserTextures;

pub fn plugin(app: &mut App) {
    app.register_type::<DesireRobotState>()
        .add_systems(Update, control_robot)
        .add_systems(Update, update_desire_robot_state);
}

fn control_robot(
    mut commands: Commands,
    mut q_robot_state: Query<(Entity, &mut RobotState)>,
    // mut q_robot_state: Query<(&mut RobotState, &mut DesireRobotState)>,
    mut marker: Query<(&Transform, &mut EndEffectorTarget), Without<FloatingCamera>>,
    mut gizmos: Gizmos,
    mut cam: Query<&mut Transform, With<FloatingCamera>>,
    mut images: ResMut<Assets<Image>>,
    mut egui_user_textures: ResMut<EguiUserTextures>,
) {
    if marker.iter().count() == 0 {
        return;
    }

    let (marker, mut ee_target) = marker.iter_mut().last().unwrap();

    // if none of the options are enabled, return
    if ee_target.translation.is_none() && ee_target.rotation.is_none() {
        return;
    }

    if cam.iter().count() == 0 {
        // auto add
        // FIXME use better way to add camera

        let (image_handle, camera) = build_camera_to_egui_img_texture(
            512,
            512,
            images.as_mut(),
            egui_user_textures.as_mut(),
        );

        commands
            .spawn(FloatingCamera {
                img_handle: image_handle,
            })
            // .insert(TransformBundle::default())
            .insert(Camera3dBundle {
                camera,
                transform: Transform::from_translation(Vec3::new(0.0, 0.0, 15.0))
                    .looking_at(Vec3::default(), Vec3::Y),
                ..default()
            });
    }

    let show_gizmo = false;
    // let show_gizmo = true;

    for (entity, mut robot_state) in q_robot_state.iter_mut() {
        // for (mut robot_state, mut desire_robot_state) in q_robot_state.iter_mut() {

        // println!("robot_state: {:?}", &robot_state.robot_chain);

        // for n in robot_state.robot_chain.iter() {
        //     error!("{:#?} > {:#?}", &n.joint().name, "joint");
        //     error!("{:#?} > {:#?}", n.link().clone().map(|l|l.name), "link");
        // }

        let mut solver = JacobianIkSolver::default();
        solver.allowable_target_distance = ee_target.allowable_target_distance;
        solver.allowable_target_angle = ee_target.allowable_target_angle;

        let constraints = k::Constraints {
            rotation_x: ee_target.rotation.is_some(),
            rotation_y: ee_target.rotation.is_some(),
            rotation_z: ee_target.rotation.is_some(),

            position_x: ee_target.translation.is_some(),
            position_y: ee_target.translation.is_some(),
            position_z: ee_target.translation.is_some(),

            // ignored_joint_names: opt.ignored_joint_names.clone(),
            ..Default::default()
        };

        // let target: k::Isometry3<f32> =
        //     k::Isometry3::<f32>::from_bevy_with_swap_yz_axis_and_flip_hand(transform);

        let nodes: Vec<_> = robot_state.robot_chain.iter().collect();

        // if let Some(arm) = k::SerialChain::try_new(robot_state.robot_chain.clone()) {
        let real_serial_link = k::SerialChain::from_end(nodes.last().unwrap());
        // the following one is a detatched one (otherwise it will be a reference)
        let arm = real_serial_link.clone();
        let arm_ee_parent = k::SerialChain::from_end(nodes[nodes.len() - 2]);
        let arm_ee_transform = arm.end_transform().to_bevy();
        let arm_ee_parent_transform = arm_ee_parent.end_transform().to_bevy();

        // let joints = real_serial_link.iter_joints().collect::<Vec<_>>();
        // real_serial_link.set_joint_positions(&[0.0; 7]);

        // arm.joint_positions();

        let mut nodes_ = nodes.last().unwrap().iter_ancestors().collect::<Vec<_>>();
        nodes_.reverse();

        for mut cam_transform in cam.iter_mut() {
            // gizmos.set_camera(cam);
            *cam_transform = arm_ee_transform;
            // it seems like the camera is should be looking 90 degree along x-axis (at least for panda robot)
            cam_transform.rotate_local_y(-std::f32::consts::FRAC_PI_2);
            cam_transform.rotate_local_z(-std::f32::consts::FRAC_PI_2);
        }

        error!("{:#?}", ee_target);

        // we need to set transform default to the actual ee transfrom.
        // otherewise, e.g., if we are just using 0,0,0 and controlling
        // orientation, the ik solver dislike it. The above is a lie, it doesn't matter.
        // the orientation keep on getting reset and i dont know why
        let mut target_transform = arm_ee_transform;

        if let Some(translation) = ee_target.translation {
            target_transform.translation = match ee_target.translation_mode {
                EndEffectorMode::Absolute => translation,
                EndEffectorMode::ApplyAsDelta => {
                    // remove value
                    ee_target.translation.take();
                    arm_ee_transform.translation + translation

                    // let mut tr = arm_ee_transform;
                    // tr.rotation = tr.rotation.swap_yz_axis_and_flip_hand();
                    // tr.transform_point(translation)

                    // ee_target.translation.take();

                    // let relative_translation = arm_ee_transform.transform_point(translation);

                    // arm_ee_transform.translation + relative_translation
                    // relative_translation
                }
            }
        }
        if let Some(rotation) = ee_target.rotation {
            target_transform.rotation = match ee_target.rotation_mode {
                EndEffectorMode::Absolute => {
                    let up = Dir3::new_unchecked(rotation * Vec3::Y);
                    let forward = -Dir3::new_unchecked(rotation * Vec3::X);

                    let out = arm_ee_transform.looking_to(forward, up);
                    // rotation
                    out.rotation
                }
                // EndEffectorMode::Absolute => {
                //     ee_target.rotation;
                //     rotation * arm_ee_parent_transform.rotation
                //     arm_ee_parent_transform.translate_around(point, rotation);
                // },
                EndEffectorMode::ApplyAsDelta => {
                    // remove value
                    ee_target.rotation.take();
                    rotation * arm_ee_transform.rotation
                }
            }
        }

        if show_gizmo {
            gizmos.axes(target_transform, 2.);
            gizmos.axes(arm_ee_transform, 4.);
        }
        error!("{:#?}", target_transform.rotation);

        let target: k::Isometry3<f32> = k::Isometry3::<f32>::from_bevy(&target_transform)
            // FIXME should we flip hand here??
            // .swap_yz_axis_and_flip_hand()
            .flip_hand();

        // even though arm is a reference, it is still mutable (due to the internal implementation)
        match solver.solve_with_constraints(&arm, &target, &constraints) {
            Ok(_) => {
                // robot_state.set_changed();
                // desire_robot_state.set_target(arm.joint_positions().to_vec());

                commands.entity(entity).insert(DesireRobotState::new(
                    real_serial_link.unwrap(), // unwrap is safe here to get inner value
                    Some(arm.joint_positions().to_vec()),
                ));
            }
            Err(err) => {
                println!("Err: {err}");
            }
        }
    }
}

#[derive(Default, Debug)]
pub enum EndEffectorMode {
    #[default]
    Absolute,
    // if it is apply as diff, the ee controller will
    // remove its value from option after applying the ik
    ApplyAsDelta,
}

impl Default for EndEffectorTarget {
    fn default() -> Self {
        Self {
            queued_translation: None,
            translation: None,
            rotation: None,
            translation_mode: EndEffectorMode::Absolute,
            rotation_mode: EndEffectorMode::Absolute,
            allowable_target_distance: 0.1,
            allowable_target_angle: 0.08,
        }
    }
}

#[derive(Component, Debug)]
pub struct EndEffectorTarget {
    pub queued_translation: Option<Vec3>,
    pub translation: Option<Vec3>,
    pub rotation: Option<Quat>,
    pub translation_mode: EndEffectorMode,
    pub rotation_mode: EndEffectorMode,

    pub allowable_target_distance: f32,
    pub allowable_target_angle: f32,
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

fn update_desire_robot_state(
    commands: Commands,
    mut q_desire_robot_state: Query<(&mut RobotState, &mut DesireRobotState)>,
    time: Res<Time>,
) {
    for (mut robot_state, mut desire_robot_state) in q_desire_robot_state.iter_mut() {
        // Update the desired robot state
        desire_robot_state.update(time.delta_seconds());
        robot_state.set_changed();
    }
}
