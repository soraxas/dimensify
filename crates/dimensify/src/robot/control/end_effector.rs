use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};
use k::{InverseKinematicsSolver, JacobianIkSolver};

use crate::{
    camera::window_camera::{FloatingCamera, build_camera_to_egui_img_texture},
    coordinate_system::prelude::*,
    robot::{RobotLink, RobotState, control::DesireRobotState},
    util::{exponential_iterator::ExponentialIterator, math_trait_ext::BevyQuatDistanceTrait},
};
use bevy_egui::EguiUserTextures;

#[cfg(feature = "physics")]
use crate::robot::urdf_loader::{RobotLinkInitOption, RobotLinkInitOptions};

pub fn plugin(app: &mut App) {
    app.register_type::<EndEffectorMode>()
        .register_type::<EndEffectorTarget>()
        .register_type::<EndEffectorUserMarker>()
        .add_systems(
            Update,
            // the timer avoid too frequent updates to the ik target
            ee_target_to_target_joint_state.run_if(on_timer(Duration::from_millis(150))),
        )
        // .add_systems(Startup, spawn_user_ee_marker)
        .add_systems(Update, (draw_ee_absolute_marker, ee_absolute_marker_sync));

    #[cfg(feature = "physics")]
    app.add_systems(Update, set_robot_link_init_options);
}

/// A marker for user to control the end effector target
pub fn spawn_user_ee_marker(mut commands: Commands) {
    commands.spawn((
        EndEffectorUserMarker::default(),
        Transform::default(),
        Name::new("User ee marker"),
    ));
}

#[cfg(feature = "physics")]
/// automatically insert end effector target for the robot link with some specified name
fn set_robot_link_init_options(
    mut commands: Commands,
    mut q_robot_links: Populated<(&RobotLink, Entity, &mut RobotLinkInitOptions)>,
    mut images: ResMut<Assets<Image>>,
    mut egui_user_textures: ResMut<EguiUserTextures>,
) {
    for (new_robot_link, entity, mut init_options) in q_robot_links.iter_mut() {
        // remove the init options
        commands.entity(entity).remove::<RobotLinkInitOptions>();

        for option in init_options.0.drain(..) {
            match option {
                RobotLinkInitOption::AsEndEffectorTarget(ee_target) => {
                    commands.entity(entity).insert(ee_target.clone());
                }
                RobotLinkInitOption::WithAttachedCamera {
                    camera_origin,
                    image_width,
                    image_height,
                } => {
                    let (image_handle, camera) = build_camera_to_egui_img_texture(
                        image_width,
                        image_height,
                        images.as_mut(),
                        egui_user_textures.as_mut(),
                    );
                    let joint_name = new_robot_link
                        .joint_name()
                        .unwrap_or("unknown joint name".to_string());

                    // spawn a camera inside this link
                    commands.entity(entity).with_children(|child_builder| {
                        // insert floating camera
                        child_builder
                            .spawn(FloatingCamera {
                                img_handle: image_handle,
                            })
                            .insert((
                                Name::new(format!("ee camera @ {}", joint_name)),
                                Camera3d::default(),
                                camera,
                            RenderTarget::Image(ImageRenderTarget::from(image_handle.clone())),

                                camera_origin,
                            ));
                    });
                }
            }
        }
    }
}

/// A component that can be inserted into a robot part,
/// which will tell the robot arm to move to the position of this component
#[derive(Component, Debug, Reflect)]
pub struct EndEffectorUserMarker {
    pub translation_mode: bool,
    pub rotation_mode: bool,
}

impl Default for EndEffectorUserMarker {
    fn default() -> Self {
        Self {
            translation_mode: true,
            rotation_mode: true,
        }
    }
}

fn draw_ee_absolute_marker(
    marker: Query<
        &Transform,
        // Or<(
        // With<EndEffectorUserMarker>,
        With<EndEffectorTarget>, // )>
    >,
    mut gizmos: Gizmos,
) {
    for marker_transform in marker.iter() {
        gizmos.axes(*marker_transform, 0.5);

        // we flip and swap again here as k kinematics uses a different coordinate system
        // gizmos.axes(marker_transform.flip_hand(), 0.8);

        gizmos.sphere(marker_transform.translation, 0.07, Color::BLACK);
    }
}

/// A system that set the end effector target to the marker's position
fn ee_absolute_marker_sync(
    marker: Single<(&Transform, &EndEffectorUserMarker), Changed<Transform>>,
    mut end_eff_target: Query<&mut EndEffectorTarget, Without<EndEffectorUserMarker>>,
) {
    let (marker_transform, marker) = *marker;

    for mut ee_target in end_eff_target.iter_mut() {
        ee_target.clear();
        if marker.translation_mode {
            ee_target.translation = Some(marker_transform.translation);
        }
        if marker.rotation_mode {
            ee_target.rotation = Some(marker_transform.rotation);
        }
    }
}

#[derive(Default, Debug, Reflect, Clone)]
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
            allowable_target_distance: 0.05,
            allowable_target_angle: 0.08,
        }
    }
}

/// A component that represents the end effector target
/// This component set configs for the end effector target
#[derive(Component, Debug, Reflect, Clone)]
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
/// A struct that contains the parameters generating
/// progressively larger values for the target distance and angle
pub struct ProgressiveIkParameters {
    /// If the distance is smaller than this value, it is reached.
    pub allowable_target_distance_initial: f32,
    pub allowable_target_distance_max: f32,
    /// If the angle distance is smaller than this value, it is reached.
    pub allowable_target_angle_initial: f32,
    pub allowable_target_angle_max: f32,
    pub max_steps: usize,
}

impl Default for ProgressiveIkParameters {
    fn default() -> Self {
        Self {
            allowable_target_distance_initial: 0.001,
            allowable_target_distance_max: 0.1,
            allowable_target_angle_initial: 0.005,
            // allowable_target_angle_max: 0.1,
            allowable_target_angle_max: 0.4,
            max_steps: 8,
        }
    }
}

impl ProgressiveIkParameters {
    pub fn get_translation_iterator(&self) -> ExponentialIterator {
        ExponentialIterator::new(
            self.allowable_target_distance_initial,
            self.allowable_target_distance_max,
            self.max_steps,
        )
    }

    pub fn get_rotation_iterator(&self) -> ExponentialIterator {
        ExponentialIterator::new(
            self.allowable_target_angle_initial,
            self.allowable_target_angle_max,
            self.max_steps,
        )
    }
}

impl EndEffectorTarget {
    pub fn clear(&mut self) {
        self.queued_translation = None;
        self.translation = None;
        self.rotation = None;
    }

    /// Calculate the distance between the current target and the given transform
    /// This is used to determine if the target has changed significantly
    pub fn distance(&self, transform: Transform) -> f32 {
        let translation_dist = self
            .translation
            .map(|t| transform.translation.distance(t))
            .unwrap_or(0.0);

        let rotation_dist = self
            .rotation
            .map(|r| transform.rotation.distance(r))
            .unwrap_or(0.0);

        // perhaps we should have some scaling factors here
        translation_dist + rotation_dist
    }
}

/// A system that updates the desired robot state, based on the target joint positions
fn ee_target_to_target_joint_state(
    mut commands: Commands,
    mut q_robot_state: Query<(Entity, &mut RobotState)>,
    // mut q_robot_state: Query<(&mut RobotState, &mut DesireRobotState)>,
    mut ee_target: Query<&mut EndEffectorTarget, Changed<EndEffectorTarget>>,
    mut gizmos: Gizmos,
) {
    if ee_target.iter().count() == 0 {
        return;
    }

    let mut ee_target = ee_target.iter_mut().last().unwrap();

    // if none of the options are enabled, return
    if ee_target.translation.is_none() && ee_target.rotation.is_none() {
        return;
    }

    // let show_gizmo = false;
    let show_gizmo = true;

    for (entity, robot_state) in q_robot_state.iter_mut() {
        // for (mut robot_state, mut desire_robot_state) in q_robot_state.iter_mut() {

        // println!("robot_state: {:?}", &robot_state.robot_chain);

        // for n in robot_state.robot_chain.iter() {
        //     error!("{:#?} > {:#?}", &n.joint().name, "joint");
        //     error!("{:#?} > {:#?}", n.link().clone().map(|l|l.name), "link");
        // }

        // let mut solver = JacobianIkSolver::new(
        //     // default:
        //     0.001,
        //     0.005,
        //     0.5,
        //     35,
        // );
        // solver.allowable_target_distance = ee_target.allowable_target_distance;
        // solver.allowable_target_angle = ee_target.allowable_target_angle;

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
        // the following one is a detached one (otherwise it will be a reference)
        let arm = real_serial_link.clone();
        let arm_ee_transform = arm.end_transform().to_bevy();

        // let joints = real_serial_link.iter_joints().collect::<Vec<_>>();
        // real_serial_link.set_joint_positions(&[0.0; 7]);

        // arm.joint_positions();

        // let mut nodes_ = nodes.last().unwrap().iter_ancestors().collect::<Vec<_>>();
        // nodes_.reverse();

        // error!("{:#?}", ee_target);

        // we need to set transform default to the actual ee transform.
        // otherwise, e.g., if we are just using 0,0,0 and controlling
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
        // error!("{:#?}", target_transform.rotation);

        let target: k::Isometry3<f32> = k::Isometry3::<f32>::from_bevy(&target_transform)
            // FIXME should we flip hand here??
            // .swap_yz_axis_and_flip_hand()
            // .flip_hand()
            // .swap_yz_axis_and_flip_hand()
            ;

        let ik_progressive_params = ProgressiveIkParameters::default();

        let translation_iter = ik_progressive_params.get_translation_iterator();
        let rotation_iter = ik_progressive_params.get_rotation_iterator();

        for (target_dist, target_rot) in translation_iter.zip(rotation_iter) {
            let solver = JacobianIkSolver::new(
                // default:
                target_dist,
                target_rot,
                0.5,
                35,
            );

            // even though arm is a reference, it is still mutable (due to the internal implementation)
            match solver.solve_with_constraints(&arm, &target, &constraints) {
                Ok(_) => {
                    // robot_state.set_changed();
                    // desire_robot_state.set_target(arm.joint_positions().to_vec());

                    // let's do a final sanity check to see if the final result is true better than the previous one
                    // let original_dist = ee_target.distance(arm_ee_transform);
                    // let ik_solution_dist = ee_target.distance(arm.end_transform().to_bevy());

                    // if ik_solution_dist < original_dist {
                    // dbg!("dist", original_dist, ik_solution_dist);

                    // if the ik solution is better than the original one, we will update the robot state
                    commands.entity(entity).insert(DesireRobotState::new(
                        real_serial_link.unwrap(), // unwrap is safe here to get inner value
                        Some(arm.joint_positions().to_vec()),
                    ));
                    break;
                    // }
                }
                Err(err) => {
                    println!("Err: {err}");
                }
            }
        }
    }
}
