use bevy::prelude::*;

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
