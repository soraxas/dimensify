use bevy::prelude::*;
use bevy::utils::hashbrown::HashMap;
use bevy_editor_pls::editor::EditorInternalState;
use bevy_editor_pls::editor_window::{open_floating_window, EditorWindowContext};
use bevy_editor_pls::{editor_window::EditorWindow, AddEditorWindow};

use crate::robot::control::end_effector::{EndEffectorMode, EndEffectorTarget};
use crate::robot::urdf_loader::{
    RobotLinkInitOption, UrdfLoadRequest, UrdfLoadRequestParams,
};
#[cfg(feature = "gspat")]
use crate::scene::gaussian_splatting::GaussianSplattingSceneLoadRequest;

pub(crate) fn plugin(app: &mut App) {
    app.add_editor_window::<ShowcaseWindow>();
}

pub(crate) struct ShowcaseWindow;

impl EditorWindow for ShowcaseWindow {
    type State = ();

    const NAME: &'static str = "Showcase";
    const DEFAULT_SIZE: (f32, f32) = (200., 150.);

    fn app_setup(app: &mut App) {
        app.add_systems(Startup, |internal_state: ResMut<EditorInternalState>| {
            open_floating_window::<Self>(internal_state.into_inner());
        });
    }

    fn ui(world: &mut World, mut _cx: EditorWindowContext, ui: &mut egui::Ui) {
        // TODO: look into file picker: https://github.com/kirjavascript/trueLMAO/blob/master/frontend/src/widgets/file.rs

        let urdf_file_root = "https://cdn.jsdelivr.net/gh/Daniella1/urdf_files_dataset@81f4cdac42c3a51ba88833180db5bf3697988c87/urdf_files/random";

        // let editor_state = &mut cx.state_mut::<Self>().unwrap();

        // ui.text_edit_singleline(&mut editor_state.robot_path);
        // if ui.button("load robot").clicked() {
        //     world.send_event(UrdfLoadRequest::from_file(editor_state.robot_path.clone()));
        // }

        if ui.button("load panda").clicked() {
            world.send_event(
                UrdfLoadRequest::from_file(
                    format!("{urdf_file_root}/robot-assets/franka_panda/panda.urdf").to_string(),
                )
                .with_params(UrdfLoadRequestParams {
                    joint_init_options: HashMap::from([(
                        "end_effector_frame_fixed_joint".to_string(),
                        vec![
                            // set this to be the end effector
                            EndEffectorTarget {
                                translation: None,
                                rotation: None,
                                translation_mode: EndEffectorMode::Absolute,
                                // rotation_mode: EndEffectorMode::ApplyAsDelta,
                                rotation_mode: EndEffectorMode::Absolute,
                                ..Default::default()
                            }
                            .into(),
                            // spawn a camera inside this link
                            RobotLinkInitOption::WithAttachedCamera {
                                camera_origin: Transform::default().with_rotation(
                                    Quat::from_euler(
                                        EulerRot::XYZ,
                                        0.0, // No rotation around the X-axis
                                        -std::f32::consts::FRAC_PI_2, // 90 degrees rotation around the Y-axis
                                        -std::f32::consts::FRAC_PI_2, // 90 degrees rotation around the Z-axis
                                    ),
                                ),
                                image_height: 512,
                                image_width: 512,
                            },
                        ]
                        .into(),
                    )]),
                    ..Default::default()
                }),
            );
        }
        if ui.button("load robot ur5").clicked() {
            world.send_event(UrdfLoadRequest::from_file(
                format!("{urdf_file_root}/robot-assets/ur5/ur5_gripper.urdf").to_string(),
            ));
        }
        if ui.button("load robot ur10").clicked() {
            world.send_event(UrdfLoadRequest::from_file(
                format!("{urdf_file_root}/robot-assets/ur10/ur10_robot.urdf").to_string(),
            ));
        }
        if ui.button("load robot kinova").clicked() {
            world.send_event(UrdfLoadRequest::from_file(
                format!("{urdf_file_root}/robot-assets/kinova/kinova.urdf").to_string(),
            ));
        }
        if ui.button("load robot spot").clicked() {
            world.send_event(UrdfLoadRequest::from_file(
                format!("{urdf_file_root}/spot_ros/spot_description/urdf/spot.urdf").to_string(),
            ));
        }
        #[cfg(feature = "gspat")]
        if ui
            .button("load gaussian splatting scene (garden)")
            .clicked()
        {
            world.send_event(GaussianSplattingSceneLoadRequest {
                path: "https://files.au-1.osf.io/v1/resources/954rg/providers/osfstorage/674592a0367509e10b078938?.ply"
                    .to_string(),
                    transform: Transform {
                        translation: Vec3::new(-0.2, 1.0, 0.3),
                        rotation: Quat::from_euler(EulerRot::XYZ, 0.5, -0.3, 3.3),
                        scale: Vec3::splat(0.3),
                        // ..default()
                    }
            },
            );
        }
    }
}
