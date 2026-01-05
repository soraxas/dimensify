use bevy::prelude::*;
use bevy_gaussian_splatting::{
    CloudSettings, GaussianCamera, GaussianMode, GaussianSplattingPlugin, PlanarGaussian3dHandle,
};

use crate::camera::main_camera;

pub fn plugin(app: &mut App) {
    app.add_plugins(GaussianSplattingPlugin)
        .add_event::<GaussianSplattingSceneLoadRequest>()
        .add_systems(
            PreUpdate,
            load_gaussian_splatting_scene_handler
                .run_if(on_event::<GaussianSplattingSceneLoadRequest>),
        )
        // spawn the gaussian camera
        .add_systems(
            // NOTE: this only works for the first camera
            // DOES NOT supports dynamic camera spawning
            PostStartup,
            spawn_gaussian_camera,
        );
}

fn spawn_gaussian_camera(
    mut commands: Commands,
    q_main_camera: MainCameraWithoutGaussianCameraQuery,
) {
    for entity in q_main_camera.iter() {
        commands.entity(entity).insert(GaussianCamera::default());
    }
}
#[derive(Event, Debug, Default, Message)]
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
type MainCameraWithoutGaussianCameraQuery<'a, 'b> = Query<
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
    mut q_main_camera: MainCameraWithoutGaussianCameraQuery,
) {
    for event in reader.read() {
        commands.spawn((
            PlanarGaussian3dHandle(asset_server.load(&event.path)),
            CloudSettings {
                gaussian_mode: GaussianMode::Gaussian3d,
                ..default()
            },
            event.transform,
            Name::new("GaussianCloud"),
        ));

        // do we need the following?
        // #[cfg(feature = "gsplat")]
        // tonemapping: Tonemapping::None,
    }
}
