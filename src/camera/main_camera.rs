use bevy::{
    core_pipeline::{
        fxaa::Fxaa,
        prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass},
    },
    prelude::*,
};

use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};


#[derive(Component)]
pub struct MainCamera;


pub fn plugin(app: &mut App) {
    app.add_plugins(PanOrbitCameraPlugin)
        .add_systems(Startup, (setup,));
}


// #[derive(States)]
// #[derive(Debug, Hash, Eq, PartialEq, Clone)]
// pub enum MainCameraState {
//     Active,
//     Inactive,
// }




fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Camera3dBundle {
            camera: Camera {
                // Deferred both supports both hdr: true and hdr: false
                // hdr: false,
                ..default()
            },
            transform: Transform::from_xyz(0.7, 1.7, 1.0)
                .looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
            ..default()
        },
        MainCamera,
        PanOrbitCamera::default(),
        // FogSettings {
        //     color: Color::srgb_u8(43, 44, 47),
        //     falloff: FogFalloff::Linear {
        //         start: 1.0,
        //         end: 8.0,
        //     },
        //     ..default()
        // },
        // EnvironmentMapLight {
        //     diffuse_map: asset_server.load("environment_maps/pisa_diffuse_rgb9e5_zstd.ktx2"),
        //     specular_map: asset_server.load("environment_maps/pisa_specular_rgb9e5_zstd.ktx2"),
        //     intensity: 2000.0,
        // },
        // DepthPrepass,
        // MotionVectorPrepass,
        // DeferredPrepass,
        Fxaa::default(),
    ));
}
