//! Core Dimensify viewer plugins and compatibility types.
//!
//! The default plugin group provides the viewer camera. Optional robot viewing
//! is feature-gated; legacy simulation, Rapier, egui editor, and infinite-grid
//! integrations remain inactive.

use bevy::{app::PluginGroupBuilder, prelude::*};

/// Optional standalone robot viewer; separate from the default plugin group.
#[cfg(feature = "robot")]
pub mod robosim;

/// Shared viewer camera types and setup.
pub mod camera {
    /// Primary viewer camera compatibility module.
    pub mod main_camera {
        use bevy::{anti_alias::fxaa::Fxaa, prelude::*};
        use dimensify_ui::WorldSpaceCamera;

        /// Marker for the viewer's primary 3D camera.
        #[derive(Component)]
        pub struct MainCamera;

        /// Spawns the primary 3D camera with a default pose and FXAA.
        pub struct CameraPlugin;

        impl Plugin for CameraPlugin {
            fn build(&self, app: &mut App) {
                app.add_systems(Startup, setup);
            }
        }

        fn setup(mut commands: Commands) {
            commands.spawn((
                Camera3d::default(),
                Transform::from_translation(Vec3::splat(20.0)).looking_at(Vec3::ZERO, Vec3::Y),
                MainCamera,
                WorldSpaceCamera,
                Fxaa::default(),
            ));
        }
    }
}

/// Small initialization shim retained by the native entry point.
pub mod util {
    use eyre::Result;

    /// Install color-eyre's diagnostic hooks.
    pub fn initialise() -> Result<()> {
        color_eyre::install()
    }
}

/// Core runtime plugins retained from the former Dimensify group.
pub struct DimensifyPlugins {
    /// Retained for configuration compatibility; infinite-grid rendering is no
    /// longer installed by this plugin group.
    pub infinite_grid: bool,
    /// Retained for configuration compatibility.
    pub with_sun: bool,
    /// Retained for configuration compatibility.
    pub with_ambient_light: bool,
    /// Retained for configuration compatibility.
    pub with_ui: bool,
}

impl PluginGroup for DimensifyPlugins {
    fn build(self) -> PluginGroupBuilder {
        let _ = (
            self.infinite_grid,
            self.with_sun,
            self.with_ambient_light,
            self.with_ui,
        );
        PluginGroupBuilder::start::<Self>().add(camera::main_camera::CameraPlugin)
    }
}

/// Empty compatibility group for former editor-only plugins.
pub struct SimDevPlugin;

impl PluginGroup for SimDevPlugin {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
    }
}

/// Empty compatibility group for former showcase-only plugins.
pub struct SimShowcasePlugin;

impl PluginGroup for SimShowcasePlugin {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::camera::main_camera::{CameraPlugin, MainCamera};
    use bevy::prelude::*;
    use dimensify_ui::WorldSpaceCamera;

    #[test]
    fn camera_plugin_spawns_the_product_camera_markers() {
        let mut app = App::new();
        app.add_plugins(CameraPlugin);
        app.update();

        let count = app
            .world_mut()
            .query_filtered::<Entity, (With<MainCamera>, With<WorldSpaceCamera>)>()
            .iter(app.world())
            .count();
        assert_eq!(count, 1);
    }
}

/// Optional Gaussian-splat loading and rendering.
#[cfg(feature = "splat")]
pub use dimensify_splat as splat;

/// Native/WASM example shell over shared navigation and Feathers controls.
#[cfg(feature = "playground")]
pub mod playground;
