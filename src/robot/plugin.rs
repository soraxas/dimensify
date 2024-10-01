use bevy::{prelude::*, utils::HashMap};

use crate::robot_vis::{RobotLink, RobotState};

use super::Robot;

#[derive(Resource, Default)]
struct RobotToCollisionChecker(HashMap<Entity, Robot>);

#[derive(Component, Default, Reflect)]
struct RobotLinkIsColliding;

pub fn plugin(app: &mut App) {
    app.register_type::<RobotLinkIsColliding>()
        .add_systems(Update, (on_new_robot_root, on_robot_change).chain())
        .add_systems(Update, show_colliding_link_color)
        .add_systems(Update, detect_removals)
        .insert_resource(RobotToCollisionChecker::default());
}

fn detect_removals(
    mut removals: RemovedComponents<RobotLinkIsColliding>,
    // ... (maybe Commands or a Query ?) ...
) {
    for entity in removals.read() {
        // do something with the entity
        // eprintln!("Entity {:?} had the component removed.", entity);
    }
}

fn set_material_recursive(
    entity: Entity,
    children_query: &Query<&Children>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    material_handles: &Query<&mut Handle<StandardMaterial>>,
) {
    if let Ok(m_handle) = material_handles.get(entity) {
        if let Some(material) = materials.get_mut(m_handle) {
            material.base_color = Color::srgba(1., 0., 0., 0.5);
        }
    }

    if let Ok(children) = children_query.get(entity) {
        for child in children.iter() {
            set_material_recursive(*child, children_query, materials, material_handles);
        }
    }
}

fn show_colliding_link_color(
    colliding_links: Query<
        (&RobotLink, Entity), // , (With<RobotLinkIsColliding>)
    >,
    children_query: Query<&Children>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    material_handles: Query<&mut Handle<StandardMaterial>>,
) {
    for (link, entity) in &colliding_links {
        // link.set_color(Color::RED);

        set_material_recursive(entity, &children_query, &mut materials, &material_handles);
    }

    // for m_handle in material_handles.iter() {
    //     if let Some(material) = materials.get_mut(m_handle) {
    //         material.base_color = Color::srgba(1., 0., 0., 0.5);
    //     }
    // }
}

fn on_robot_change(
    robots: Query<(&RobotState, Entity), Changed<RobotState>>,
    mut robot_to_collision_checker: ResMut<RobotToCollisionChecker>,
) {
    let robot_to_collision_checker = &mut robot_to_collision_checker.into_inner().0;
    for (robot_state, entity) in &robots {
        let robot = robot_to_collision_checker.get_mut(&entity).unwrap();

        robot.set_joints(robot_state.robot_chain.joint_positions().as_slice());

        dbg!(robot.has_collision().unwrap());
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
