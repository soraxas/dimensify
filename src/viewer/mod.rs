use crate::protocol::Command;
use crate::stream::{CommandLog, StreamSet};
use bevy::math::primitives::Cuboid;
use bevy::math::{Isometry2d, Rot2};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use std::collections::HashMap;

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
            _ => ViewerMode::ThreeD,
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
        .init_resource::<LineStore>()
        .init_resource::<LineStore2d>()
        .init_resource::<RectStore2d>()
        .init_resource::<CommandCursor>()
        .add_systems(Startup, validate_command_log.after(StreamSet::Load))
        .add_systems(Update, apply_new_commands)
        .add_systems(Update, (draw_lines_3d, draw_lines_2d, draw_rects_2d));
}

fn validate_command_log(settings: Res<ViewerSettings>, command_log: Res<CommandLog>) {
    let mut unsupported = 0usize;
    for command in &command_log.commands {
        let is_3d = matches!(
            command,
            Command::Line3d { .. }
                | Command::Text3d { .. }
                | Command::Mesh3d { .. }
                | Command::Transform { .. }
        );
        let is_2d = matches!(
            command,
            Command::Line2d { .. } | Command::Text2d { .. } | Command::Rect2d { .. }
        );
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
pub(crate) struct LineStore {
    pub(crate) lines: Vec<LineSpec>,
}

#[derive(Resource, Default)]
pub(crate) struct LineStore2d {
    pub(crate) lines: Vec<Line2dSpec>,
}

#[derive(Resource, Default)]
pub(crate) struct RectStore2d {
    pub(crate) rects: Vec<Rect2dSpec>,
}

#[derive(Clone)]
pub(crate) struct LineSpec {
    points: Vec<Vec3>,
    color: Color,
    width: f32,
}

#[derive(Clone)]
pub(crate) struct Line2dSpec {
    points: Vec<Vec2>,
    color: Color,
    width: f32,
}

#[derive(Clone)]
pub(crate) struct Rect2dSpec {
    position: Vec2,
    size: Vec2,
    rotation: f32,
    color: Color,
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
    mut line_store: ResMut<LineStore>,
    mut line_store_2d: ResMut<LineStore2d>,
    mut rect_store_2d: ResMut<RectStore2d>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let total = command_log.commands.len();
    if cursor.index >= total {
        return;
    }
    let new_commands = &command_log.commands[cursor.index..];
    cursor.index = total;

    for command in new_commands {
        match command {
            Command::Line3d {
                points,
                color,
                width,
            } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                if *width != 1.0 {
                    bevy::log::warn!("Line3d width is not supported yet; using 1.0");
                }
                line_store.lines.push(LineSpec {
                    points: points.iter().map(|p| Vec3::new(p[0], p[1], p[2])).collect(),
                    color: Color::srgba(color[0], color[1], color[2], color[3]),
                    width: *width,
                });
            }
            Command::Line2d {
                points,
                color,
                width,
            } => {
                if settings.mode == ViewerMode::ThreeD {
                    continue;
                }
                if *width != 1.0 {
                    bevy::log::warn!("Line2d width is not supported yet; using 1.0");
                }
                line_store_2d.lines.push(Line2dSpec {
                    points: points.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
                    color: Color::srgba(color[0], color[1], color[2], color[3]),
                    width: *width,
                });
            }
            Command::Text3d { .. } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                bevy::log::warn!("Text3d is not implemented yet; ignoring command");
            }
            Command::Text2d { .. } => {
                if settings.mode == ViewerMode::ThreeD {
                    continue;
                }
                bevy::log::warn!("Text2d is not implemented yet; ignoring command");
            }
            Command::Mesh3d {
                name,
                position,
                scale,
            } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0).mesh());
                let material = materials.add(StandardMaterial::default());
                let entity = commands
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform::from_translation(Vec3::new(
                            position[0],
                            position[1],
                            position[2],
                        ))
                        .with_scale(Vec3::new(scale[0], scale[1], scale[2])),
                        Name::new(name.clone()),
                    ))
                    .id();
                entities.map.insert(name.clone(), entity);
            }
            Command::Rect2d {
                position,
                size,
                rotation,
                color,
            } => {
                if settings.mode == ViewerMode::ThreeD {
                    continue;
                }
                rect_store_2d.rects.push(Rect2dSpec {
                    position: Vec2::new(position[0], position[1]),
                    size: Vec2::new(size[0], size[1]),
                    rotation: *rotation,
                    color: Color::srgba(color[0], color[1], color[2], color[3]),
                });
            }
            Command::Transform {
                entity,
                position,
                rotation,
                scale,
            } => {
                if settings.mode == ViewerMode::TwoD {
                    continue;
                }
                if let Some(target) = entities.map.get(entity).copied() {
                    commands.entity(target).insert(Transform {
                        translation: Vec3::new(position[0], position[1], position[2]),
                        rotation: Quat::from_xyzw(
                            rotation[0],
                            rotation[1],
                            rotation[2],
                            rotation[3],
                        ),
                        scale: Vec3::new(scale[0], scale[1], scale[2]),
                    });
                } else {
                    bevy::log::warn!("Transform refers to unknown entity '{}'", entity);
                }
            }
            Command::Binary { .. } => {
                bevy::log::warn!("Binary command payloads are not handled yet");
            }
        }
    }
}

fn draw_lines_3d(mut gizmos: Gizmos, line_store: Res<LineStore>) {
    for line in &line_store.lines {
        for points in line.points.windows(2) {
            gizmos.line(points[0], points[1], line.color);
        }
    }
}

fn draw_lines_2d(mut gizmos: Gizmos, line_store: Res<LineStore2d>) {
    for line in &line_store.lines {
        for points in line.points.windows(2) {
            gizmos.line_2d(points[0], points[1], line.color);
        }
    }
}

fn draw_rects_2d(mut gizmos: Gizmos, rect_store: Res<RectStore2d>) {
    for rect in &rect_store.rects {
        let isometry = Isometry2d::new(rect.position, Rot2::radians(rect.rotation));
        gizmos.rect_2d(isometry, rect.size, rect.color);
    }
}
