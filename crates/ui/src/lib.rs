//! Shared camera markers and UI compatibility types for Dimensify.

use bevy::prelude::*;

/// Marker for the camera rendering the 3D world.
#[derive(Component)]
pub struct WorldSpaceCamera;

/// Marker for a camera rendering screen-space UI.
#[derive(Component)]
pub struct UiSpaceCamera;

/// No-op compatibility plugin; applications compose their own UI separately.
#[derive(Default)]
pub struct DimensifyUiPlugin;

impl Plugin for DimensifyUiPlugin {
    fn build(&self, _app: &mut App) {}
}
