use crate::{
    draw_command::DrawCommand,
    stream::{CommandLog, StreamSet},
    viewer::gizmo::GizmoDrawPlugin,
};
use bevy::{
    math::primitives::Cuboid,
    pbr::{MeshMaterial3d, StandardMaterial},
    prelude::*,
};
use dimensify_protocol::{Component, ComponentKind, WorldCommand};
use std::collections::HashMap;

mod gizmo;

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

pub fn plugin(app: &mut App) {
    app.init_resource::<ViewerSettings>()
        .init_resource::<ViewerState>()
        .init_resource::<SceneEntities>()
        .init_resource::<CommandCursor>()
        .add_systems(Startup, validate_command_log.after(StreamSet::Load))
        .add_systems(Update, apply_new_commands)
        .add_plugins(GizmoDrawPlugin);
}

fn validate_command_log(settings: Res<ViewerSettings>, command_log: Res<CommandLog>) {
    let mut unsupported = 0usize;
    for command in &command_log.commands {
        let (is_3d, is_2d) = command_dimension_flags(command);
        if settings.mode == ViewerMode::TwoD && is_3d {
            unsupported += 1;
        }
        if settings.mode == ViewerMode::ThreeD && is_2d {
            unsupported += 1;
        }
    }

    if unsupported > 0 {
        let mode = match settings.mode {
            ViewerMode::TwoD => "2D",
            ViewerMode::ThreeD => "3D",
        };
        bevy::log::error!(
            "Viewer mode is {}, but {} incompatible commands were loaded.",
            mode,
            unsupported
        );
    }
}

#[derive(Resource, Default)]
pub(crate) struct SceneEntities {
    pub(crate) map: HashMap<String, Entity>,
}

#[derive(Resource, Default)]
struct CommandCursor {
    index: usize,
}

fn apply_new_commands(
    settings: Res<ViewerSettings>,
    command_log: Res<CommandLog>,
    mut cursor: ResMut<CommandCursor>,
    mut entities: ResMut<SceneEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    entity_refs: Query<EntityRef>,
    draw_commands: Query<Entity, With<DrawCommand>>,
    mesh_entities: Query<Entity, With<Mesh3d>>,
) {
    let total = command_log.commands.len();
    if cursor.index >= total {
        return;
    }
    let new_commands = &command_log.commands[cursor.index..];
    cursor.index = total;

    for command in new_commands {
        match command {
            WorldCommand::Spawn { components } => {
                let mut entity = commands.spawn_empty();
                apply_components(
                    settings.as_ref(),
                    &mut entities,
                    &mut meshes,
                    &mut materials,
                    &mut entity,
                    components,
                );
            }
            WorldCommand::Insert { entity, components } => {
                if let Some(target) = entities.map.get(entity).copied() {
                    let mut target = commands.entity(target);
                    apply_components(
                        settings.as_ref(),
                        &mut entities,
                        &mut meshes,
                        &mut materials,
                        &mut target,
                        components,
                    );
                } else {
                    bevy::log::warn!("Insert refers to unknown entity '{}'", entity);
                }
            }
            WorldCommand::Update { entity, component } => {
                let Some(target) = entities.map.get(entity).copied() else {
                    bevy::log::warn!("Update refers to unknown entity '{}'", entity);
                    continue;
                };
                let Ok(entity_ref) = entity_refs.get(target) else {
                    bevy::log::warn!("Update refers to missing entity '{}'", entity);
                    continue;
                };
                if !has_component(&entity_ref, component) {
                    bevy::log::warn!("Update refers to missing component on entity '{}'", entity);
                    continue;
                }
                let mut target = commands.entity(target);
                apply_components(
                    settings.as_ref(),
                    &mut entities,
                    &mut meshes,
                    &mut materials,
                    &mut target,
                    std::slice::from_ref(component),
                );
            }
            WorldCommand::Remove { entity, component } => {
                let Some(target) = entities.map.get(entity).copied() else {
                    bevy::log::warn!("Remove refers to unknown entity '{}'", entity);
                    continue;
                };
                remove_component(&mut commands, &mut entities, target, component, entity);
            }
            WorldCommand::Despawn { entity } => {
                if let Some(target) = entities.map.remove(entity) {
                    commands.entity(target).despawn();
                } else {
                    bevy::log::warn!("Despawn refers to unknown entity '{}'", entity);
                }
            }
            WorldCommand::Clear => {
                entities.map.clear();
                for entity in &draw_commands {
                    commands.entity(entity).despawn();
                }
                for entity in &mesh_entities {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

fn command_dimension_flags(command: &WorldCommand) -> (bool, bool) {
    let mut is_3d = false;
    let mut is_2d = false;
    let mut check_component = |component: &Component| match component {
        Component::Line3d { .. }
        | Component::Text3d { .. }
        | Component::Mesh3d { .. }
        | Component::Transform3d { .. } => is_3d = true,
        Component::Line2d { .. } | Component::Text2d { .. } | Component::Rect2d { .. } => {
            is_2d = true
        }
        _ => {}
    };
    match command {
        WorldCommand::Spawn { components } | WorldCommand::Insert { components, .. } => {
            for component in components {
                check_component(component);
            }
        }
        WorldCommand::Update { component, .. } => {
            check_component(component);
        }
        _ => {}
    }
    (is_3d, is_2d)
}

fn apply_components(
    settings: &ViewerSettings,
    entities: &mut SceneEntities,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    entity: &mut EntityCommands<'_>,
    components: &[Component],
) {
    let name = effective_name(components);
    if let Some(name) = name {
        entities.map.insert(name.clone(), entity.id());
        entity.insert(Name::new(name));
    }

    for component in components {
        match component {
            Component::Name { .. } => {}
            Component::Line3d {
                points,
                color,
                width,
                ..
            } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                if *width != 1.0 {
                    bevy::log::warn!("Line3d width is not supported yet; using 1.0");
                }
                entity.insert(DrawCommand::Line3d {
                    points: points.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect(),
                    color: Color::srgba(color[0], color[1], color[2], color[3]),
                });
            }
            Component::Line2d {
                points,
                color,
                width,
                ..
            } => {
                if settings.mode == ViewerMode::ThreeD {
                    continue;
                }
                if *width != 1.0 {
                    bevy::log::warn!("Line2d width is not supported yet; using 1.0");
                }
                entity.insert(DrawCommand::Line2d {
                    points: points.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
                    color: Color::srgba(color[0], color[1], color[2], color[3]),
                });
            }
            Component::Text3d { .. } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                bevy::log::warn!("Text3d is not implemented yet; ignoring component");
            }
            Component::Text2d { .. } => {
                if settings.mode == ViewerMode::ThreeD {
                    continue;
                }
                bevy::log::warn!("Text2d is not implemented yet; ignoring component");
            }
            Component::Mesh3d {
                position, scale, ..
            } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0).mesh());
                let material = materials.add(StandardMaterial::default());
                entity.insert((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::from_translation(Vec3::new(position[0], position[1], position[2]))
                        .with_scale(Vec3::new(scale[0], scale[1], scale[2])),
                ));
            }
            Component::Rect2d {
                position,
                size,
                rotation,
                color,
                ..
            } => {
                if settings.mode == ViewerMode::ThreeD {
                    continue;
                }
                entity.insert(DrawCommand::Rect2d {
                    position: Vec2::new(position[0], position[1]),
                    size: Vec2::new(size[0], size[1]),
                    rotation: *rotation,
                    color: Color::srgba(color[0], color[1], color[2], color[3]),
                });
            }
            Component::Transform3d {
                position,
                rotation,
                scale,
            } => {
                if settings.mode == ViewerMode::TwoD {
                    bevy::log::warn!("Transform is not supported in 2D mode; ignoring component");
                    continue;
                }
                entity.insert(Transform {
                    translation: Vec3::new(position[0], position[1], position[2]),
                    rotation: Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]),
                    scale: Vec3::new(scale[0], scale[1], scale[2]),
                });
            }
            Component::Binary { .. } => {
                bevy::log::warn!("Binary component payloads are not handled yet");
            }
        }
    }
}

fn effective_name(components: &[Component]) -> Option<String> {
    for component in components {
        if let Component::Name { value } = component {
            return Some(value.clone());
        }
    }
    for component in components {
        let name = match component {
            Component::Line3d { name, .. }
            | Component::Line2d { name, .. }
            | Component::Text3d { name, .. }
            | Component::Text2d { name, .. }
            | Component::Mesh3d { name, .. }
            | Component::Rect2d { name, .. } => name.clone(),
            _ => None,
        };
        if name.is_some() {
            return name;
        }
    }
    None
}

fn has_component(entity_ref: &EntityRef, component: &Component) -> bool {
    match component {
        Component::Name { .. } => entity_ref.contains::<Name>(),
        Component::Line3d { .. } | Component::Line2d { .. } | Component::Rect2d { .. } => {
            entity_ref.contains::<DrawCommand>()
        }
        Component::Text3d { .. } | Component::Text2d { .. } => false,
        Component::Mesh3d { .. } => entity_ref.contains::<Mesh3d>(),
        Component::Transform3d { .. } => entity_ref.contains::<Transform>(),
        Component::Binary { .. } => false,
    }
}

fn remove_component(
    commands: &mut Commands,
    entities: &mut SceneEntities,
    entity: Entity,
    component: &ComponentKind,
    entity_name: &str,
) {
    let mut target = commands.entity(entity);
    match component {
        ComponentKind::Name => {
            target.remove::<Name>();
            entities.map.remove(entity_name);
        }
        ComponentKind::Line3d | ComponentKind::Line2d | ComponentKind::Rect2d => {
            target.remove::<DrawCommand>();
        }
        ComponentKind::Text3d | ComponentKind::Text2d => {}
        ComponentKind::Mesh3d => {
            target.remove::<Mesh3d>();
            target.remove::<MeshMaterial3d<StandardMaterial>>();
        }
        ComponentKind::Transform3d => {
            target.remove::<Transform>();
        }
        ComponentKind::Binary => {}
    }
}
