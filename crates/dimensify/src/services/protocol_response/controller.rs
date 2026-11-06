use super::{draw::DrawCommand, pending_response::PendingApplyCommand};
use crate::stream::CommandLog;
use anyhow::Context;
use dimensify_transport::ProtoResponse;

use bevy::{
    pbr::{MeshMaterial3d, StandardMaterial},
    prelude::*,
};
use dimensify_protocol::prelude::{
    InsertionResult, Material as ProtoMaterial, ProtoComponent, ProtoComponentIntoBevy, Shape3d,
    WorldCommand,
};

use lightyear::prelude::MessageSender;

pub fn plugin(app: &mut App) {
    app.init_resource::<PendingMeshInsertion>()
        .init_resource::<PendingMaterialInsertion>()
        .add_systems(Update, apply_new_commands)
        // always insert meshes and materials after new commands are applied (and hence they would create new pending insertion)
        .add_systems(
            Update,
            (insert_meshes, insert_materials).after(apply_new_commands),
        )
        .add_systems(
            Update,
            handle_apply_command_response.after(apply_new_commands),
        );
}

/// outgoing responses to the client
///
/// This is a system that is responsible for sending responses to the client,
/// after the command is applied.
fn handle_apply_command_response(
    mut commands: Commands,
    mut receivers: Populated<
        (Entity, &mut MessageSender<ProtoResponse>, &ProtoResponse),
        With<PendingApplyCommand>,
    >,
) {
    for (sender_entity, mut sender, response) in &mut receivers {
        info!("Sending response to client: {:?}", sender_entity);
        let _ = sender.send::<dimensify_transport::StreamReliable>(response.clone());
        // clean up the components
        commands
            .entity(sender_entity)
            .remove::<PendingApplyCommand>()
            .remove::<ProtoResponse>();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewerMode {
    TwoD,
    ThreeD,
}

#[derive(Resource, Clone, Debug)]
pub struct ViewerSettings {
    pub mode: ViewerMode,
}

impl Default for ViewerSettings {
    fn default() -> Self {
        let mode = match std::env::var("DIMENSIFY_VIEWER_MODE")
            .unwrap_or_else(|_| "3d".to_string())
            .as_str()
        {
            "2d" => ViewerMode::TwoD,
            "3d" => ViewerMode::ThreeD,
            mode => {
                bevy::log::error!("Invalid viewer mode: {}. Falling back to 3D.", mode);
                ViewerMode::ThreeD
            }
        };
        Self { mode }
    }
}

#[derive(Resource)]
pub struct ViewerState;

impl Default for ViewerState {
    fn default() -> Self {
        Self
    }
}

#[derive(Resource, Default)]
pub(crate) struct CommandCursor {
    index: usize,
}

#[derive(Resource, Default)]
pub(crate) struct PendingMeshInsertion {
    pub(crate) items: Vec<(Entity, Shape3d)>,
}

#[derive(Resource, Default)]
pub(crate) struct PendingMaterialInsertion {
    pub(crate) items: Vec<(Entity, ProtoMaterial)>,
}

/// Insert meshes into the scene on-demand.
fn insert_meshes(
    mut commands: Commands,
    mut pending_mesh_insertion: ResMut<PendingMeshInsertion>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, shape) in pending_mesh_insertion.items.drain(..) {
        let mesh = meshes.add(shape.mesh());
        commands.entity(entity).insert(Mesh3d(mesh));
    }
}

/// Insert materials into the scene on-demand.
fn insert_materials(
    mut commands: Commands,
    mut pending_material_insertion: ResMut<PendingMaterialInsertion>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, material) in pending_material_insertion.items.drain(..) {
        let material = materials.add(material.material());
        commands.entity(entity).insert(MeshMaterial3d(material));
    }
}

pub fn apply_new_commands(
    command_log: Res<CommandLog>,
    mut cursor: ResMut<CommandCursor>,
    // mut meshes: ResMut<Assets<Mesh>>,
    // mut materials: ResMut<Assets<StandardMaterial>>,
    mut pending_mesh_insertion: ResMut<PendingMeshInsertion>,
    mut pending_material_insertion: ResMut<PendingMaterialInsertion>,
    mut commands: Commands,
    draw_commands: Query<Entity, With<DrawCommand>>,
) {
    let total = command_log.commands.len();
    if cursor.index >= total {
        return;
    }
    let new_commands = &command_log.commands[cursor.index..];
    cursor.index = total;

    for (sender_entity, command) in new_commands {
        use bevy::log::info;
        info!("Running command: {:?}", command);

        let command_result = match command {
            WorldCommand::Spawn { components } => {
                let mut entity = commands.spawn_empty();
                apply_components(
                    &mut entity,
                    components,
                    &mut pending_mesh_insertion,
                    &mut pending_material_insertion,
                );
                Ok(entity.id())
            }
            WorldCommand::Insert { entity, components } => commands
                .get_entity(*entity)
                .and_then(|mut target| {
                    apply_components(
                        &mut target,
                        components,
                        &mut pending_mesh_insertion,
                        &mut pending_material_insertion,
                    );
                    Ok(target.id())
                })
                .context(format!("Insert refers to unknown entity '{}'", entity)),

            WorldCommand::Update { entity, component } => {
                // TODO: implement targeted updates; currently all updates are rejected.
                todo!("Update is not implemented yet");
                commands
                    .get_entity(*entity)
                    .and_then(|mut target| {
                        apply_components(
                            &mut target,
                            std::slice::from_ref(component),
                            &mut pending_mesh_insertion,
                            &mut pending_material_insertion,
                        );
                        Ok(target.id())
                    })
                    .context(format!("Update refers to unknown entity '{}'", entity))
                // if !has_component(&entity_ref, component) {
                //     bevy::log::warn!("Update refers to missing component on entity '{}'", entity);
                //     continue;
                // }
            }
            WorldCommand::Remove { entity, component } => commands
                .get_entity(*entity)
                .and_then(|mut target| {
                    remove_component(&mut target, *component);
                    Ok(target.id())
                })
                .context(format!("Remove refers to unknown entity '{}'", entity)),
            WorldCommand::Despawn { entity } => commands
                .get_entity(*entity)
                .and_then(|mut target| {
                    target.despawn();
                    Ok(target.id())
                })
                .context(format!("Despawn refers to unknown entity '{}'", entity)),
            WorldCommand::Clear => {
                // TODO: implement clear; currently unused by clients.
                unimplemented!()
                // entities.map.clear();
                // for entity in &draw_commands {
                //     commands.entity(entity).despawn();
                // }
                // for entity in &mesh_entities {
                //     commands.entity(entity).despawn();
                // }
            }
        };
        match command_result {
            Ok(entity) => {
                commands
                    .entity(*sender_entity)
                    .insert(ProtoResponse::CommandResponseEntity(entity));
            }
            Err(e) => {
                bevy::log::warn!("Failed to apply command: {:?}", e);
                commands
                    .entity(*sender_entity)
                    .insert(ProtoResponse::Error {
                        message: e.to_string(),
                    });
            }
        }
    }
}

fn apply_components(
    entity_cmd: &mut EntityCommands<'_>,
    components: &[ProtoComponent],
    pending_mesh_insertion: &mut PendingMeshInsertion,
    pending_material_insertion: &mut PendingMaterialInsertion,
) {
    for component in components {
        // TODO: investigate if cloning is necessary. Do we really need the log?
        let insertion_result = component.clone().insert_into(entity_cmd);
        match insertion_result {
            InsertionResult::Trivial(_) => (),
            InsertionResult::RequireResMesh(shape) => {
                pending_mesh_insertion.items.push((entity_cmd.id(), shape))
            }
            InsertionResult::RequireResMaterial(material) => pending_material_insertion
                .items
                .push((entity_cmd.id(), material)),
        }
    }
}

/// Remove a component from an entity.
///
/// It will panic if the component id is not found.
fn remove_component(
    entity_cmd: &mut EntityCommands<'_>,
    component: dimensify_protocol::ComponentId,
) {
    let id = bevy::ecs::component::ComponentId::from(component);
    entity_cmd.remove_by_id(id);
}
