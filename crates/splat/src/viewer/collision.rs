//! SuperSplat collision voxels: download beside the scene and toggle the overlay.
use std::sync::Arc;

use bevy::prelude::*;
use dimensify_camera::viewer::NavigationConfig;

use crate::SplatScene;
use crate::collision::{VoxelCollision, VoxelOverlay};

/// Whether this build can draw the overlay (it needs storage buffers).
pub(super) const SUPPORTED: bool = !cfg!(all(feature = "webgl", not(feature = "webgpu")));

const METADATA_LIMIT: usize = 1024 * 1024;
const BINARY_LIMIT: usize = 256 * 1024 * 1024;

/// Collision found beside a downloaded scene: `None` when none is published.
pub(super) type Found = Option<Result<Arc<VoxelCollision>, String>>;

pub(super) fn install(app: &mut App) {
    #[cfg(not(all(feature = "webgl", not(feature = "webgpu"))))]
    app.add_plugins(crate::collision::VoxelOverlayPlugin);
    app.init_resource::<VoxelOverlay>().add_systems(
        Update,
        sync.before(dimensify_camera::viewer::NavigationSystems),
    );
}

/// Apply the settings toggle and `V` shortcut, following the visible splat root.
fn sync(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<NavigationConfig>,
    scene: Res<SplatScene>,
    mut settings: ResMut<super::settings::ViewerSettings>,
    mut overlay: ResMut<VoxelOverlay>,
) {
    if !config.blocked && keys.just_pressed(KeyCode::KeyV) && overlay.collision.is_some() {
        settings.collision = !settings.collision;
    }
    let available = overlay.collision.is_some();
    if settings.collision_available != available {
        settings.collision_available = available;
    }
    let enabled = SUPPORTED && settings.collision && available;
    let root = scene.loaded.as_ref().map(|loaded| loaded.entity);
    if overlay.enabled != enabled || overlay.root != root {
        overlay.enabled = enabled;
        overlay.root = root;
    }
}

fn binary_url(metadata: &url::Url) -> url::Url {
    let mut binary = metadata.clone();
    binary.set_path(&metadata.path().replace(".voxel.json", ".voxel.bin"));
    binary
}

fn decode(
    metadata: Result<super::remote::Download, String>,
    binary: impl FnOnce() -> Result<super::remote::Download, String>,
) -> Found {
    if super::remote::missing(&metadata) {
        return None;
    }
    let result = metadata.and_then(|metadata| {
        let binary = binary()?;
        VoxelCollision::parse(&metadata.bytes, &binary.bytes)
    });
    Some(result.map(Arc::new))
}

/// Download collision published next to resolved scene metadata.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn fetch(scene: &url::Url) -> Found {
    use super::remote::fetch_url;
    let metadata = super::remote::supersplat_collision(scene)?;
    decode(fetch_url(&metadata, Some(METADATA_LIMIT)), || {
        fetch_url(&binary_url(&metadata), Some(BINARY_LIMIT))
    })
}

/// Download collision published next to resolved scene metadata.
#[cfg(target_arch = "wasm32")]
pub(super) async fn fetch(scene: &url::Url) -> Found {
    use super::remote::fetch_url;
    let metadata = super::remote::supersplat_collision(scene)?;
    let json = fetch_url(&metadata, Some(METADATA_LIMIT)).await;
    let binary = match &json {
        Ok(_) => Some(fetch_url(&binary_url(&metadata), Some(BINARY_LIMIT)).await),
        Err(_) => None,
    };
    decode(json, || binary.expect("fetched after metadata"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_sits_beside_metadata_and_missing_files_mean_no_collision() {
        let metadata =
            url::Url::parse("https://d28zzqy0iyovbz.cloudfront.net/8429e5e2/v1/scene.voxel.json")
                .unwrap();
        assert_eq!(
            binary_url(&metadata).as_str(),
            "https://d28zzqy0iyovbz.cloudfront.net/8429e5e2/v1/scene.voxel.bin"
        );
        assert!(decode(Err("HTTP 403".into()), || unreachable!()).is_none());
        assert!(matches!(
            decode(Err("HTTP 500".into()), || unreachable!()),
            Some(Err(_))
        ));
    }
}
