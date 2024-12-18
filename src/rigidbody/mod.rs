use bevy::prelude::*;
use bevy_rapier3d::prelude::Collider;
use rapier3d::{math::Vector, prelude::SharedShape};

use bevy::pbr::OpaqueRendererMethod;

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
