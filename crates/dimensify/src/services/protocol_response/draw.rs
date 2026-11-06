use super::apply_new_commands;
use bevy::prelude::*;

pub(crate) struct GizmoDrawPlugin;

impl Plugin for GizmoDrawPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, draw.after(apply_new_commands));
    }
}
fn draw(mut gizmos: Gizmos, draw_commands: Query<&DrawCommand>) {
    for command in &draw_commands {
        match command {
            DrawCommand::Line3d { points, color } => {
                for window in points.windows(2) {
                    gizmos.line(window[0], window[1], *color);
                }
            }
            DrawCommand::Line2d { points, color } => {
                for window in points.windows(2) {
                    gizmos.line_2d(window[0], window[1], *color);
                }
            }
            DrawCommand::Rect2d {
                position,
                size,
                rotation,
                color,
            } => {
                let isometry = Isometry2d::new(*position, Rot2::radians(*rotation));
                gizmos.rect_2d(isometry, *size, *color);
            }
        }
    }
}

#[derive(Component, Clone)]
pub enum DrawCommand {
    Line3d {
        points: Vec<Vec3>,
        color: Color,
    },
    Line2d {
        points: Vec<Vec2>,
        color: Color,
    },
    Rect2d {
        position: Vec2,
        size: Vec2,
        rotation: f32,
        color: Color,
    },
}
