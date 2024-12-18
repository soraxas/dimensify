use crate::collision_checker::checker::{CollisionChecker, CollisionDetectorFromBevyRapierContext};
use crate::robot::urdf_loader::UrdfLinkPart;
use crate::robot::RobotLinkIsColliding;
use bevy::app::Update;
use bevy::color::Color;
use bevy::hierarchy::Parent;
use bevy::math::Quat;
use bevy::prelude::*;
use bevy::utils::{HashMap, HashSet};
use bevy_rapier3d::pipeline::CollisionEvent;
use bevy_rapier3d::plugin::RapierContext;

use crate::define_config_state;

pub fn plugin(app: &mut bevy::app::App) {
    app.init_state::<ConfCollidingContactPoints>()
        .init_state::<ConfCollidingObjects>()
        .add_systems(
            Update,
            (
                // update collision checking pipeline, if needed
                (|mut collision_checker: CollisionDetectorFromBevyRapierContext| {
                    collision_checker.update_detect();
                })
                .run_if(
                    // only run this update if the collision pipeline is needed
                    in_state(ConfCollidingObjects::On)
                        .or_else(in_state(ConfCollidingContactPoints::On)),
                ),
                // the collision pipeline update step is done before any other systems that
                // depend on the collision pipeline
                (
                    insert_colliding_marker.run_if(in_state(ConfCollidingObjects::On)),
                    display_collider_contact_points
                        .run_if(in_state(ConfCollidingContactPoints::On)),
                ),
                // create a system that remove all markers on exit
            )
                .chain(),
        )
        .add_systems(
            OnEnter(ConfCollidingObjects::Off),
            remove_all_colliding_marker,
        );
}

define_config_state!(ConfCollidingContactPoints);

define_config_state!(ConfCollidingObjects);

pub const LINK_COLLIDER_TO_PARENT_LINK: usize = 3;

/// a helper function that, if the entity is a urdf part, we get the grand parent link entity.
fn get_parent_times_n(entity: &Entity, parents: &Query<&Parent>, num: usize) -> Option<Entity> {
    let mut parent = *entity;
    for _ in 0..num {
        parent = parents.get(parent).ok()?.get();
    }
    Some(parent)
}

/// a helper function that, if the entity is a urdf part, we get the grand parent link entity.
/// Otherwise, we return the entity itself.
fn get_correct_entity(
    entity: &Entity,
    parents: &Query<&Parent>,
    urdf_parts: &Query<&UrdfLinkPart>,
) -> Option<Entity> {
    match urdf_parts.get(*entity) {
        Ok(_) => get_parent_times_n(entity, parents, LINK_COLLIDER_TO_PARENT_LINK),
        Err(_) => Some(*entity),
    }
}

/// System that inserts the colliding marker to the robot link that is colliding.
pub fn insert_colliding_marker(
    mut commands: Commands,
    parents: Query<&Parent>,
    urdf_parts: Query<&UrdfLinkPart>,
    mut collision_checker: CollisionDetectorFromBevyRapierContext,
    mut is_colliding: Query<(Entity, &mut RobotLinkIsColliding)>,
    // mut previous_colliding_entities: Local<HashMap<Entity, Vec<Entity>>>,
) {
    // run through all colliding entities and insert them some set
    let mut all_colliding_entities: HashMap<Entity, Vec<Entity>> = HashMap::default();
    // ensure that we have all the entries that has the marker component
    // this is to ensure that we can properly trigger links that are no longer colliding
    for (entity, _) in is_colliding.iter() {
        all_colliding_entities.entry(entity).or_default();
    }

    /* Find the contact pair, if it exists, between two colliders. */
    // collision_checker.update_detect();
    collision_checker
        .get_colliding_entity()
        .drain(..)
        .for_each(|(entity1, entity2)| {
            // The contact pair exists meaning that the broad-phase identified a potential contact.
            let entity1 = get_correct_entity(&entity1, &parents, &urdf_parts).unwrap_or(entity1);
            let entity2 = get_correct_entity(&entity2, &parents, &urdf_parts).unwrap_or(entity2);

            for (entity, other_entity) in [(entity1, entity2), (entity2, entity1)].iter() {
                all_colliding_entities
                    .entry(*entity)
                    .or_default()
                    .push(*other_entity);
            }
        });

    // if *previous_colliding_entities != all_colliding_entities {
    // actually perform the update, to properly trigger the change event
    for (entity, colliding_entities) in all_colliding_entities.iter() {
        if colliding_entities.is_empty() {
            commands.entity(*entity).remove::<RobotLinkIsColliding>();
        } else {
            match is_colliding.get_mut(*entity) {
                Ok((_, mut component)) => {
                    // insert this colliding entity to the existing component
                    component.entities.clear();
                    colliding_entities.iter().for_each(|e| {
                        component.entities.insert(*e);
                    });
                }
                Err(_) => {
                    // insert new component
                    let mut entities = HashSet::default();
                    colliding_entities.iter().for_each(|e| {
                        entities.insert(*e);
                    });
                    commands
                        .entity(*entity)
                        .insert(RobotLinkIsColliding { entities });
                }
            }
        }
    }
    // }

    // *previous_colliding_entities = all_colliding_entities;
}

/// System that inserts the colliding marker to the robot link that is colliding.
pub fn remove_all_colliding_marker(
    mut commands: Commands,
    parents: Query<&Parent>,
    urdf_parts: Query<&UrdfLinkPart>,
    is_colliding: Query<Entity, With<RobotLinkIsColliding>>,
) {
    for entity in is_colliding.iter() {
        let entity = get_correct_entity(&entity, &parents, &urdf_parts).unwrap_or(entity);

        commands.entity(entity).remove::<RobotLinkIsColliding>();
    }
}

fn display_collider_contact_points(
    collision_checker: CollisionDetectorFromBevyRapierContext,
    mut gizmos: Gizmos,
) {
    const CONTACT_DIST_SCALE: f32 = 10.;

    /* Find the contact pair, if it exists, between two colliders. */

    collision_checker
        .get_colliding_interactions()
        .for_each(|contact_pair| {
            // We may also read the contact manifolds to access the contact geometry.
            for manifold in &contact_pair.manifolds {
                // Read the solver contacts.
                for solver_contact in &manifold.data.solver_contacts {
                    let dist = solver_contact.dist;
                    if dist >= 0. {
                        continue;
                    }

                    let point = solver_contact.point;

                    let bevy_point = point.into();

                    gizmos.arrow(
                        bevy_point,
                        (point + manifold.data.normal * (CONTACT_DIST_SCALE * dist)).into(),
                        Color::srgb(1., 0., 0.),
                    );

                    gizmos.sphere(bevy_point, Quat::IDENTITY, dist * 5., Color::BLACK);
                }
            }
        });
}

#[deprecated]
#[allow(unused)]
fn insert_colliding_marker_using_rapier_context(
    mut commands: Commands,
    parents: Query<&Parent>,
    urdf_parts: Query<&UrdfLinkPart>,
    rapier_context: Res<RapierContext>,
    mut is_colliding: Query<(Entity, &mut RobotLinkIsColliding)>,
    // mut contact_force_events: EventReader<ContactForceEvent>,
    mut previous_colliding_entities: Local<HashMap<Entity, Vec<Entity>>>,
) {
    /// a helper function that, if the entity is a urdf part, we get the grand parent link entity.
    /// Otherwise, we return the entity itself.
    fn get_correct_entity(
        entity: &Entity,
        parents: &Query<&Parent>,
        urdf_parts: &Query<&UrdfLinkPart>,
    ) -> Option<Entity> {
        match urdf_parts.get(*entity) {
            Ok(_) => get_parent_times_n(entity, parents, LINK_COLLIDER_TO_PARENT_LINK),
            Err(_) => Some(*entity),
        }
    }

    // run through all colliding entities and insert them some set
    let mut all_colliding_entities: HashMap<Entity, Vec<Entity>> = HashMap::default();
    // ensure that we have all the entries in the previous_colliding_entities
    // this is to ensure that we can properly trigger links that are no longer colliding
    for entity in previous_colliding_entities.keys() {
        all_colliding_entities.entry(*entity).or_default();
    }

    /* Find the contact pair, if it exists, between two colliders. */
    rapier_context.contact_pairs().for_each(|contact_pair| {
        // The contact pair exists meaning that the broad-phase identified a potential contact.
        if contact_pair.has_any_active_contact() {
            let (entity1, entity2) = (contact_pair.collider1(), contact_pair.collider2());

            let entity1 = get_correct_entity(&entity1, &parents, &urdf_parts).unwrap_or(entity1);
            let entity2 = get_correct_entity(&entity2, &parents, &urdf_parts).unwrap_or(entity2);

            for (entity, other_entity) in [(entity1, entity2), (entity2, entity1)].iter() {
                all_colliding_entities
                    .entry(*entity)
                    .or_default()
                    .push(*other_entity);
            }
        }
    });

    if *previous_colliding_entities != all_colliding_entities {
        // actually perform the update, to properly trigger the change event
        for (entity, colliding_entities) in all_colliding_entities.iter() {
            if colliding_entities.is_empty() {
                commands.entity(*entity).remove::<RobotLinkIsColliding>();
            } else {
                match is_colliding.get_mut(*entity) {
                    Ok((_, mut component)) => {
                        // insert this colliding entity to the existing component
                        component.entities.clear();
                        colliding_entities.iter().for_each(|e| {
                            component.entities.insert(*e);
                        });
                    }
                    Err(_) => {
                        // insert new component
                        let mut entities = HashSet::default();
                        colliding_entities.iter().for_each(|e| {
                            entities.insert(*e);
                        });
                        commands
                            .entity(*entity)
                            .insert(RobotLinkIsColliding { entities });
                    }
                }
            }
        }
    }

    *previous_colliding_entities = all_colliding_entities;
}

#[deprecated]
#[allow(unused)]
fn insert_colliding_marker_using_event(
    mut commands: Commands,
    mut collision_events: EventReader<CollisionEvent>,
    parents: Query<&Parent>,
    urdf_parts: Query<&UrdfLinkPart>,
    mut is_colliding: Query<&mut RobotLinkIsColliding>,
    // mut contact_force_events: EventReader<ContactForceEvent>,
) {
    /// a helper function that, if the entity is a urdf part, we get the grand parent link entity.
    /// Otherwise, we return the entity itself.
    fn get_correct_entity(
        entity: &Entity,
        parents: &Query<&Parent>,
        urdf_parts: &Query<&UrdfLinkPart>,
    ) -> Option<Entity> {
        match urdf_parts.get(*entity) {
            Ok(_) => get_parent_times_n(entity, parents, LINK_COLLIDER_TO_PARENT_LINK),
            Err(_) => Some(*entity),
        }
    }

    for collision_event in collision_events.read() {
        match collision_event {
            CollisionEvent::Started(entity1, entity2, _collision_event_flags) => {
                let entity1 =
                    get_correct_entity(entity1, &parents, &urdf_parts).unwrap_or(*entity1);
                let entity2 =
                    get_correct_entity(entity2, &parents, &urdf_parts).unwrap_or(*entity2);

                for entity in [entity1, entity2].iter() {
                    let other_entity = if *entity == entity1 { entity2 } else { entity1 };

                    match is_colliding.get_mut(*entity) {
                        Ok(mut component) => {
                            // insert this colliding entity to the existing component
                            component.entities.insert(other_entity);
                        }
                        Err(_) => {
                            // insert new component
                            commands.entity(*entity).insert(RobotLinkIsColliding {
                                entities: [other_entity].into(),
                            });
                        }
                    }
                }
            }
            CollisionEvent::Stopped(entity1, entity2, _collision_event_flags) => {
                let entity1 =
                    get_correct_entity(entity1, &parents, &urdf_parts).unwrap_or(*entity1);
                let entity2 =
                    get_correct_entity(entity2, &parents, &urdf_parts).unwrap_or(*entity2);

                for entity in [entity1, entity2].iter() {
                    let other_entity = if *entity == entity1 { entity2 } else { entity1 };
                    // commands.entity(*entity).remove::<RobotLinkIsColliding>();
                    if let Ok(mut component) = is_colliding.get_mut(*entity) {
                        component.entities.remove(&other_entity);

                        if component.entities.is_empty() {
                            commands.entity(*entity).remove::<RobotLinkIsColliding>();
                        }
                    }
                }
            }
        }
    }
    // for contact_force_event in contact_force_events.read() {
    //     println!("Received contact force event: {:?}", contact_force_event);
    // }
}
