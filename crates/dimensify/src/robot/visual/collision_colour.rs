use bevy::{
    platform::collections::{HashMap, hash_table::Entry},
    prelude::*,
};

use crate::robot::RobotLinkIsColliding;

/// The color of the link when it is colliding, which is red with 50% transparency
const COLLIDING_LINK_COLOR: Color = Color::srgba(1., 0.1, 0.1, 0.5);

pub fn plugin(app: &mut App) {
    app.add_systems(Update, show_colliding_link_color)
        .insert_resource(LinkIsCollidingPreviousColor::default())
        .add_observer(detect_removals)
        // This observer will react to the removal of the component.
        // .insert_resource(RobotToCollisionChecker::default())
        ;
}

#[derive(Resource, Default, Reflect)]
struct LinkIsCollidingPreviousColor(HashMap<Handle<StandardMaterial>, Color>);

/// Show the colliding link color when component is added
fn show_colliding_link_color(
    colliding_links: Query<(Entity, &RobotLinkIsColliding), Changed<RobotLinkIsColliding>>,
    children_query: Query<&Children>,
    mut previous_colors: ResMut<LinkIsCollidingPreviousColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    material_handles: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    // let shown_entity = HashSet::new();
    for (entity, is_colliding) in &colliding_links {
        if is_colliding.entities.is_empty() {
            removal_recursive(
                entity,
                &children_query,
                &mut previous_colors,
                &mut materials,
                &material_handles,
            );
        } else {
            set_material_recursive(
                entity,
                &children_query,
                &COLLIDING_LINK_COLOR,
                &mut previous_colors,
                &mut materials,
                &material_handles,
            );
        }

        // }
    }
}

/// Recursively remove the color from the entity and all its children
fn removal_recursive(
    entity: Entity,
    children_query: &Query<&Children>,
    previous_colors: &mut ResMut<LinkIsCollidingPreviousColor>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_handles: &Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    if let Ok(m_handle) = material_handles.get(entity) {
        if let Some(material) = materials.get_mut(m_handle) {
            if let Some(previous_color) = previous_colors.0.remove(&m_handle.0) {
                material.base_color = previous_color;
            }
        }
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            removal_recursive(
                child,
                children_query,
                previous_colors,
                materials,
                material_handles,
            );
        }
    }
}

fn set_material_recursive(
    entity: Entity,
    children_query: &Query<&Children>,
    color: &Color,
    previous_colors: &mut ResMut<LinkIsCollidingPreviousColor>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_handles: &Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    if let Ok(m_handle) = material_handles.get(entity) {
        if let Some(material) = materials.get_mut(m_handle) {
            let original_color = std::mem::replace(&mut material.base_color, *color);

            // only insert the original color into hashmap if its empty
            // FIXME: should we store the outer mesh3d-material wrapper, or only the inner standard material?
            previous_colors
                .0
                .try_insert(m_handle.0.clone(), original_color);
        }
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            set_material_recursive(
                child,
                children_query,
                color,
                previous_colors,
                materials,
                material_handles,
            );
        }
    }
}

/// Detect the removal of the component and remove the color
fn detect_removals(
    removals: On<Remove, RobotLinkIsColliding>,
    children_query: Query<&Children>,
    mut previous_colors: ResMut<LinkIsCollidingPreviousColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    material_handles: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    removal_recursive(
        removals.entity,
        &children_query,
        &mut previous_colors,
        &mut materials,
        &material_handles,
    );
}
