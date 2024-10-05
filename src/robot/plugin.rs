use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

use crate::robot_vis::{RobotLink, RobotState};

use super::Robot;

#[derive(Resource, Default)]
struct RobotToCollisionChecker(HashMap<Entity, Robot>);

const COLLIDING_LINK_COLOR_1: Color = Color::srgba(1., 0.5, 0., 0.5);
const COLLIDING_LINK_COLOR_2: Color = Color::srgba(1., 0., 0.5, 0.5);

#[derive(Component, Reflect)]
pub struct RobotLinkIsColliding {
    pub entities: HashSet<Entity>,
}

#[derive(Resource, Default, Reflect)]
struct LinkIsCollidingPreviousColor(HashMap<Handle<StandardMaterial>, Color>);

pub fn plugin(app: &mut App) {
    app.register_type::<RobotLinkIsColliding>()
        .register_type::<HashSet<Entity>>()
        .add_systems(Update, (on_new_robot_root, on_robot_change).chain())
        .add_systems(Update, show_colliding_link_color)
        // This observer will react to the removal of the component.
        .observe(detect_removals)
        .insert_resource(RobotToCollisionChecker::default())
        .insert_resource(LinkIsCollidingPreviousColor::default());
}

fn removal_recursive(
    entity: Entity,
    children_query: &Query<&Children>,
    previous_colors: &mut ResMut<LinkIsCollidingPreviousColor>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_handles: &Query<&mut Handle<StandardMaterial>>,
) {
    if let Ok(m_handle) = material_handles.get(entity) {
        if let Some(material) = materials.get_mut(m_handle) {
            if let Some(previous_color) = previous_colors.0.remove(m_handle) {
                material.base_color = previous_color;
            }
        }
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            removal_recursive(
                *child,
                children_query,
                previous_colors,
                materials,
                material_handles,
            );
        }
    }
}

fn detect_removals(
    removals: Trigger<OnRemove, RobotLinkIsColliding>,
    children_query: Query<&Children>,
    mut previous_colors: ResMut<LinkIsCollidingPreviousColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    material_handles: Query<&mut Handle<StandardMaterial>>,
) {
    removal_recursive(
        removals.entity(),
        &children_query,
        &mut previous_colors,
        &mut materials,
        &material_handles,
    );
}

fn set_material_recursive(
    entity: Entity,
    children_query: &Query<&Children>,
    color: &Color,
    previous_colors: &mut ResMut<LinkIsCollidingPreviousColor>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_handles: &Query<&mut Handle<StandardMaterial>>,
) {
    if let Ok(m_handle) = material_handles.get(entity) {
        if let Some(material) = materials.get_mut(m_handle) {
            let original_color = std::mem::replace(&mut material.base_color, *color);

            // only insert the original color into hashmap if its empty
            if let bevy::utils::hashbrown::hash_map::Entry::Vacant(vacant_entry) =
                previous_colors.0.entry(m_handle.clone_weak())
            {
                vacant_entry.insert(original_color);
            }
        }
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            set_material_recursive(
                *child,
                children_query,
                color,
                previous_colors,
                materials,
                material_handles,
            );
        }
    }
}

fn show_colliding_link_color(
    colliding_links: Query<(Entity, &RobotLinkIsColliding), Changed<RobotLinkIsColliding>>,
    children_query: Query<&Children>,
    mut previous_colors: ResMut<LinkIsCollidingPreviousColor>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    material_handles: Query<&mut Handle<StandardMaterial>>,
) {
    // let shown_entity = HashSet::new();
    for (entity, is_colliding) in &colliding_links {
        // if !shown_entity.contains(&entity) {
        // let color = match is_colliding {
        //     RobotLinkIsColliding::Contact1 { .. } => COLLIDING_LINK_COLOR_1,
        //     RobotLinkIsColliding::Contact2 { .. } => COLLIDING_LINK_COLOR_2,
        // };

        dbg!(&is_colliding.entities);

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
                &COLLIDING_LINK_COLOR_2,
                &mut previous_colors,
                &mut materials,
                &material_handles,
            );
        }

        // }
    }
}

fn on_robot_change(
    robots: Query<(&RobotState, Entity), Changed<RobotState>>,
    mut robot_to_collision_checker: ResMut<RobotToCollisionChecker>,
) {
    let robot_to_collision_checker = &mut robot_to_collision_checker.into_inner().0;
    for (robot_state, entity) in &robots {
        let robot = robot_to_collision_checker.get_mut(&entity).unwrap();

        robot.set_joints(robot_state.robot_chain.joint_positions().as_slice());

        // dbg!(robot.has_collision().unwrap());
    }
}

fn on_new_robot_root(
    robots: Query<(&RobotState, Entity), Added<RobotState>>,
    mut robot_to_collision_checker: ResMut<RobotToCollisionChecker>,
) {
    for (robot_state, entity) in &robots {
        if !robot_to_collision_checker.0.contains_key(&entity) {
            robot_to_collision_checker.0.insert(
                entity,
                Robot::from_urdf_robot(robot_state.urdf_robot.clone(), None).unwrap(), // TODO make urd_robot contains the base_dir
            );
        }

        // let kinematic: &k::Chain<f32> = &robot_state.robot_chain;

        // kinematic.update_transforms();
        // for link in kinematic.iter() {}
    }
}
