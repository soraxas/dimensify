use bevy::{pbr::CascadeShadowConfigBuilder, prelude::*};
use bevy_rapier3d::prelude::Collider;
use rapier3d::{math::Vector, prelude::SharedShape};

use bevy::pbr::OpaqueRendererMethod;

/// add a sun to the scene
pub fn add_sun(mut commands: Commands) {
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
        transform: Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            0.0,
            -std::f32::consts::FRAC_PI_4,
        )),
        ..default()
    });
}

/// add a floor to the scene
pub fn add_floor(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let mut forward_mat: StandardMaterial = Color::srgb(0.1, 0.2, 0.1).into();
    forward_mat.opaque_render_method = OpaqueRendererMethod::Forward;
    let forward_mat_h = materials.add(forward_mat);

    let plane = SharedShape::halfspace(Vector::y_axis());

    commands
        .spawn(PbrBundle {
            mesh: meshes.add(Plane3d::default().mesh().size(50.0, 50.0)),
            // mesh: meshes.add(Plane3d::default().mesh().size(50.0, 50.0)),
            material: forward_mat_h.clone(),
            ..default()
        })
        .insert(Collider::from(plane));
}
