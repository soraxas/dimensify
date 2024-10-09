use std::ops::DerefMut;

use bevy::{
    ecs::{entity, system::SystemParam},
    log::tracing_subscriber::filter,
    prelude::*,
    utils::{hashbrown::HashMap, HashSet},
};
use bevy_editor_pls::{
    editor_window::{EditorWindow, EditorWindowContext},
    AddEditorWindow,
};
use bevy_rapier3d::prelude::*;
use egui::{CollapsingHeader, Grid};

use crate::{
    collision_checker::SimpleCollisionPipeline, robot::plugin::RobotLinkIsColliding,
    robot_vis::visuals::UrdfLinkPart,
};

use super::robot_state_setter::EditorState;

const LINK_COLLIDER_TO_PARENT_LINK: usize = 3;

fn get_parent_times_n(entity: &Entity, parents: &Query<&Parent>, num: usize) -> Option<Entity> {
    let mut parent = *entity;
    for _ in 0..num {
        parent = parents.get(parent).ok()?.get();
    }
    Some(parent)
}

fn insert_colliding_marker(
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
                        dbg!(&entities);
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

use crate::robot::collidable::IgnoredCollidersFilter;

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(RapierDebugRenderPlugin::default().disabled())
        .add_systems(Update, insert_colliding_marker)
        .add_editor_window::<RapierDebugEditorWindow>()
        .add_systems(Update, display_collider_contact_points);
}

fn get_name_from_parents(
    entity: Entity,
    names: &Query<&Name>,
    parents: &Query<&Parent>,
) -> Option<String> {
    let mut parent = entity;
    while let Ok(parent_entity) = parents.get(parent) {
        if let Ok(name_component) = names.get(parent_entity.get()) {
            return Some(name_component.as_str().to_string());
        }
        parent = parent_entity.get();
    }
    None
}

fn display_collider_contact_points(
    rapier_context: Res<RapierContext>,
    names: Query<&Name>,
    parents: Query<&Parent>,
    mut gizmos: Gizmos,
) {
    // rapier_context.intersection_pairs().for_each(|a| {
    //     dbg!(a);
    // });

    const CONTACT_DIST_SCALE: f32 = 10.;

    /* Find the contact pair, if it exists, between two colliders. */
    rapier_context.contact_pairs().for_each(|contact_pair| {
        // The contact pair exists meaning that the broad-phase identified a potential contact.
        if contact_pair.has_any_active_contact() {
            // let name1 = get_parent_times_n(
            //     &contact_pair.collider1(),
            //     &parents,
            //     LINK_COLLIDER_TO_PARENT_LINK,
            // )
            // .and_then(|entity| names.get(entity).ok());

            // let name2 = get_parent_times_n(
            //     &contact_pair.collider1(),
            //     &parents,
            //     LINK_COLLIDER_TO_PARENT_LINK,
            // )
            // .and_then(|entity| names.get(entity).ok());

            // println!("name1");
            // dbg!("{} <-> {}", name1.unwrap(), name2.unwrap());

            // We may also read the contact manifolds to access the contact geometry.
            for manifold in contact_pair.manifolds() {
                // Read the solver contacts.
                for solver_contact in &manifold.raw.data.solver_contacts {
                    let dist = solver_contact.dist;
                    if dist >= 0. {
                        continue;
                    }

                    let point = solver_contact.point.into();

                    gizmos.arrow(
                        point,
                        point + manifold.normal() * (CONTACT_DIST_SCALE * dist),
                        Color::srgb(1., 0., 0.),
                    );

                    gizmos.sphere(point, Quat::IDENTITY, dist * 5., Color::BLACK);
                }
            }
        }
    });
}

pub(crate) struct RapierDebugEditorWindow;

impl EditorWindow for RapierDebugEditorWindow {
    type State = EditorState;

    const NAME: &'static str = "Rapier Debug Render";
    // const DEFAULT_SIZE: (f32, f32) = (200., 150.);

    fn ui(world: &mut World, mut _cx: EditorWindowContext, ui: &mut egui::Ui) {
        if let Some(mut debug_context) = world.get_resource_mut::<DebugRenderContext>() {
            ui.checkbox(&mut debug_context.enabled, "Enable Debug Rendering");

            if !debug_context.enabled {
                ui.disable();
            }

            let debug_render_mode = &mut debug_context.pipeline.mode;
            let mut clicked = None;
            {
                let response = ui.radio(
                    !debug_render_mode.is_empty(),
                    "Master Rigid-Body Physics switch",
                );
                if response.clicked() {
                    clicked = Some(true);
                    // if option is changed, update the debug render mode (all or nothing)
                    *debug_render_mode = if debug_render_mode.is_empty() {
                        DebugRenderMode::all()
                    } else {
                        DebugRenderMode::empty()
                    };
                }
            }

            CollapsingHeader::new("Rigid-Body Physics Debug Rendering Option")
                .default_open(true)
                .open(clicked)
                .show(ui, |content| {
                    macro_rules! ui_flag_modify {
                        ($flag:expr, $desc:expr) => {
                            let mut is_on = debug_render_mode.contains($flag);
                            content.checkbox(&mut is_on, $desc);
                            debug_render_mode.set($flag, is_on);
                        };
                    }

                    ui_flag_modify!(DebugRenderMode::COLLIDER_SHAPES, "collider shapes");
                    ui_flag_modify!(DebugRenderMode::RIGID_BODY_AXES, "rigid body axes");
                    ui_flag_modify!(DebugRenderMode::MULTIBODY_JOINTS, "multibody joints");
                    ui_flag_modify!(DebugRenderMode::IMPULSE_JOINTS, "impulse joints");
                    ui_flag_modify!(DebugRenderMode::JOINTS, "joints");
                    ui_flag_modify!(DebugRenderMode::SOLVER_CONTACTS, "solver contacts");
                    ui_flag_modify!(DebugRenderMode::CONTACTS, "geometric contacts");
                    ui_flag_modify!(DebugRenderMode::COLLIDER_AABBS, "collider aabbs");
                });

            ui.separator();
        }
    }
}
