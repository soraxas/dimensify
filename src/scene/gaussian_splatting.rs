use bevy::prelude::*;
use bevy_gaussian_splatting::{
    GaussianCamera, GaussianCloudSettings, GaussianMode, GaussianSplattingBundle,
    GaussianSplattingPlugin,
};

use crate::camera::main_camera;

pub fn plugin(app: &mut App) {
    app.add_plugins(GaussianSplattingPlugin)
        .add_event::<GaussianSplattingSceneLoadRequest>()
        .add_systems(
            Update,
            load_gaussian_splatting_scene_handler
                .run_if(on_event::<GaussianSplattingSceneLoadRequest>()),
        );
}

#[derive(Event, Debug, Default)]
pub struct GaussianSplattingSceneLoadRequest {
    pub path: String,
    pub transform: Transform,
}

impl GaussianSplattingSceneLoadRequest {
    pub fn new(path: String) -> Self {
        Self {
            path,
            transform: default(),
        }
    }
}

/// Query for the main camera entity that does not ALREADY have a GaussianCamera
type MainCameraWithougGaussianCameraQuery<'a, 'b> = Query<
    'a,
    'b,
    Entity,
    (
        With<main_camera::MainCamera>,
        With<Camera>,
        Without<GaussianCamera>,
    ),
>;

fn load_gaussian_splatting_scene_handler(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut reader: EventReader<GaussianSplattingSceneLoadRequest>,
    // mut gaussian_assets: ResMut<Assets<GaussianCloud>>,
    mut q_main_camera: MainCameraWithougGaussianCameraQuery,
) {
    for event in reader.read() {
        commands.spawn((
            GaussianSplattingBundle {
                cloud: asset_server.load(&event.path),
                settings: GaussianCloudSettings {
                    gaussian_mode: GaussianMode::Gaussian3d,
                    transform: event.transform,
                    ..default()
                },
                ..default()
            },
            Name::new("GaussianCloud"),
        ));

        // do we need the following?
        // #[cfg(feature = "gspat")]
        // tonemapping: Tonemapping::None,

        for entity in q_main_camera.iter_mut() {
            // Ondemand we will insert the GaussianCamera
            commands.entity(entity).insert(GaussianCamera::default());
        }
    }
}
