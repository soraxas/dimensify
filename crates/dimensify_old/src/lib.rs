#![allow(clippy::too_many_arguments)]
extern crate nalgebra as na;

pub use crate::dimensify::{Dimensify, DimensifyApp, DimensifyGraphics, DimensifyState};
pub use crate::graphics::{BevyMaterial, GraphicsManager};
pub use crate::harness::plugin::HarnessPlugin;
pub use crate::physics::PhysicsState;
pub use bevy::prelude::KeyCode;
pub use plugins::DimensifyPlugin;

mod camera3d;
pub mod constants;
mod dimensify;
pub(crate) mod graphics;
pub mod harness;
mod mouse;
pub mod physics;
pub mod plugins;
pub mod scene;
pub mod scene_graphics;
mod ui;
pub mod utils;

pub mod math {
    pub type Isometry<N> = na::Isometry3<N>;
    pub type Vector<N> = na::Vector3<N>;
    pub type Point<N> = na::Point3<N>;
    pub type Translation<N> = na::Translation3<N>;
}
