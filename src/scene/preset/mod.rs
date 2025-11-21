use bevy::{light::CascadeShadowConfigBuilder, prelude::*};

#[cfg(feature = "physics")]
use bevy_rapier3d::prelude::Collider;

#[cfg(feature = "physics")]
use rapier3d::{math::Vector, prelude::SharedShape};

use bevy::pbr::OpaqueRendererMethod;

use crate::constants::SCENE_FLOOR_NAME;

#[derive(Debug, Default, Component)]
pub struct FloorMarker;

/// add a sun to the scene
pub fn add_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 15_000.,
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            maximum_distance: 100.0,
            ..default()
        }
        .build(),
        Transform::from_rotation(Quat::from_euler(
            EulerRot::ZYX,
            0.0,
            0.0,
            -std::f32::consts::FRAC_PI_4,
        )),
    ));
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

    let _entity = commands.spawn((
        Name::new(SCENE_FLOOR_NAME),
        FloorMarker,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        // mesh: meshes.add(Plane3d::default().mesh().size(50.0, 50.0)),
        MeshMaterial3d(forward_mat_h.clone()),
    ));
    #[cfg(feature = "physics")]
    {
        let plane = SharedShape::halfspace(Vector::y_axis());

        entity.insert(Collider::from(plane));
    }
}
