use crate::{constants::SCENE_FLOOR_NAME, rigidbody::add_floor};
use bevy::prelude::*;
use bevy_rapier3d::prelude::Collider;
use rapier3d::{math::Vector, prelude::SharedShape};

use std::f32::consts::*;

use bevy::{
    core_pipeline::prepass::{DeferredPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass},
    pbr::{CascadeShadowConfigBuilder, DefaultOpaqueRendererMethod, OpaqueRendererMethod},
};

#[derive(Resource)]
struct RotateSun(bool);

pub fn plugin(app: &mut App) {
    app
        .insert_resource(RotateSun(true))
        .add_systems(Startup, (setup, add_floor))
        .add_systems(Update, (animate_light_direction, switch_mode))
        // setup more distance for shadow map
        ;
}

fn setup(mut commands: Commands) {
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 15_000.,
            shadows_enabled: true,
            ..default()
        },
        cascade_shadow_config: CascadeShadowConfigBuilder {
            num_cascades: 3,
            maximum_distance: 100.0,
            ..default()
        }
        .into(),
        transform: Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.0, -FRAC_PI_4)),
        ..default()
    });

    // Example instructions
    commands.spawn(
        TextBundle::from_section("", TextStyle::default()).with_style(Style {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        }),
    );
}

fn animate_light_direction(
    time: Res<Time>,
    mut query: Query<&mut Transform, With<DirectionalLight>>,
    rotate_sun: Res<RotateSun>,
) {
    if !rotate_sun.0 {
        return;
    }
    for mut transform in &mut query {
        transform.rotate_y(time.delta_seconds() * PI / 5.0);
    }
}

fn switch_mode(
    mut text: Query<&mut Text>,
    keys: Res<ButtonInput<KeyCode>>,
    mut rotate_sun: ResMut<RotateSun>,
) {
    let mut text = text.single_mut();
    let text = &mut text.sections[0].value;

    text.clear();

    if keys.just_pressed(KeyCode::Space) {
        rotate_sun.0 = !rotate_sun.0;
    }
}
