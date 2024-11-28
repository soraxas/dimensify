use bevy::prelude::*;
use bevy_gaussian_splatting::{
    GaussianCloudSettings, GaussianMode, GaussianSplattingBundle, GaussianSplattingPlugin,
};

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

fn load_gaussian_splatting_scene_handler(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut reader: EventReader<GaussianSplattingSceneLoadRequest>,
    // mut gaussian_assets: ResMut<Assets<GaussianCloud>>,
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
    }
}
