use bevy::{anti_alias::fxaa::Fxaa, prelude::*};

use crate::scene::preset;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};

#[derive(Component)]
pub struct MainCamera;

pub struct CameraPlugin {
    pub with_ambient_light: bool,
    pub with_sun: bool,
}

impl Default for CameraPlugin {
    fn default() -> Self {
        Self {
            with_ambient_light: false,
            with_sun: true,
        }
    }
}

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        let with_ambient_light = self.with_ambient_light;
        app.add_plugins(PanOrbitCameraPlugin)
            .add_systems(Startup, move |commands: Commands| {
                setup(commands, with_ambient_light)
            });

        if self.with_sun {
            app.add_systems(Startup, preset::add_sun);
        }
    }
}

// #[derive(States)]
// #[derive(Debug, Hash, Eq, PartialEq, Clone)]
// pub enum MainCameraState {
//     Active,
//     Inactive,
// }

fn setup(mut commands: Commands, with_ambient_light: bool) {
    let mut camera = commands.spawn((
        Camera3d::default(),
        // Transform::from_xyz(2.05, 2.0, -2.9).looking_at(Vec3::new(0.0, 0.3, 0.0), Vec3::Y),
        Transform::from_translation(Vec3::new(20., 20., 20.)).looking_at(Vec3::ZERO, Vec3::Y),
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
    if with_ambient_light {
        camera.insert(AmbientLight {
            color: Color::WHITE,
            brightness: 700.0,
            affects_lightmapped_meshes: true,
        });
    }
}
