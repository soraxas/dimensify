//! Splat scene and modal-input adaptation to shared viewer navigation.
use super::{FrameSplat, url_panel::UrlPanel};
use crate::{SplatLoaded, SplatScene};
use bevy::prelude::*;
use dimensify_camera::viewer::NavigationConfig;
pub(super) use dimensify_camera::viewer::{CameraAction, NavigationMode, home_pose};

/// Forward scene framing and URL capture without writing a camera pose.
pub(super) fn sync(
    scene: Res<SplatScene>,
    panel: Res<UrlPanel>,
    mut loaded: MessageReader<SplatLoaded>,
    mut frames: MessageReader<FrameSplat>,
    mut actions: MessageWriter<CameraAction>,
    mut config: ResMut<NavigationConfig>,
) {
    let loaded_bounds = loaded.read().last().map(|event| event.bounds);
    config.bounds = scene
        .loaded
        .as_ref()
        .map(|scene| scene.bounds)
        .or(loaded_bounds)
        .or(config.bounds);
    config.blocked = panel.open;
    if frames.read().count() > 0 {
        actions.write(CameraAction::Frame);
    }
}
