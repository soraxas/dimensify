use bevy::{
    ecs::{entity, system::SystemParam},
    gizmos::gizmos,
    prelude::*,
};
use bevy_editor_pls::{
    editor_window::{EditorWindow, EditorWindowContext},
    AddEditorWindow,
};
use bevy_rapier3d::prelude::*;
use egui::CollapsingHeader;

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

// #[derive(SystemParam)]
// struct SameUserDataFilter<'w, 's> {
//     tags: Query<'w, 's, &'static CustomFilterTag>,
// }

// impl BevyPhysicsHooks for SameUserDataFilter<'_, '_> {
//     fn filter_contact_pair(&self, context: PairFilterContextView) -> Option<SolverFlags> {
//         if self.tags.get(context.collider1()).ok().copied()
//             == self.tags.get(context.collider2()).ok().copied()
//         {
//             Some(SolverFlags::COMPUTE_IMPULSES)
//         } else {
//             None
//         }
//     }
// }

pub(super) fn plugin(app: &mut App) {
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_plugins(RapierDebugRenderPlugin::default().disabled())
        .add_systems(Update, insert_colliding_marker)
        .add_editor_window::<RapierDebugEditorWindow>()
        .add_systems(Update, display_collider_contract_points);
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

fn display_collider_contract_points(
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
            // The contact pair has active contacts, meaning that it
            // contains contacts for which contact forces were computed.
        }

        let name1 = get_parent_times_n(
            &contact_pair.collider1(),
            &parents,
            LINK_COLLIDER_TO_PARENT_LINK,
        )
        .and_then(|entity| names.get(entity).ok());

        let name2 = get_parent_times_n(
            &contact_pair.collider1(),
            &parents,
            LINK_COLLIDER_TO_PARENT_LINK,
        )
        .and_then(|entity| names.get(entity).ok());

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
                return;
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
