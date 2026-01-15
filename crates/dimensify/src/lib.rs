use bevy::{app::PluginGroupBuilder, prelude::*};
// use bevy_web_asset::WebAssetPlugin;

#[cfg(feature = "robot")]
pub mod urdf_assets_loader;

pub mod camera;

pub mod constants;
pub mod coordinate_system;
pub mod graphics;
pub mod plugins;
pub mod services;

#[cfg(feature = "physics")]
pub mod collision_checker;
#[cfg(feature = "physics")]
pub mod physics;
#[cfg(feature = "robot")]
pub mod robot;

pub mod scene;
pub mod sim;
#[cfg(feature = "protocol")]
pub mod stream;
#[cfg(feature = "protocol")]
pub mod telemetry;
pub mod test_scene;
pub mod ui;
pub mod util;
// pub mod pointcloud;

// pub use bevy_web_asset::WebAssetPlugin;

pub mod reexport {
    pub use bevy_egui;
    pub use bevy_inspector_egui;
}

pub struct SimPlugin;

#[cfg(feature = "gsplat")]
use bevy_gaussian_splatting::{CloudSettings, PlanarGaussian3dHandle};

impl PluginGroup for SimPlugin {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group
            // .add(WebAssetPlugin {
            //     cache_resource: true,
            // })
            .add(graphics::plugin)
            .add(ui::plugin)
            .add(services::plugin);

        #[cfg(feature = "protocol")]
        {
            group = group.add(telemetry::plugin).add(stream::plugin);
        }

        #[cfg(feature = "transport")]
        {
            group = group.add(dimensify_transport::TransportRuntimePlugin::default());
        }

        #[cfg(feature = "robot")]
        {
            group = group.add(robot::plugin);
        }

        // if !group.is_in_subset::<EguiPlugin>() {
        //     group = group.add(EguiPlugin);
        // }
        // .add_plugins(EguiPlugin)

        group = group
            .add(camera::plugin) // camera needs egui to be added first
            .add(scene::plugin)
            // .add(sketching::plugin)
            ;
        #[cfg(feature = "physics")]
        {
            group = group.add(physics::plugin);
        }

        group
    }
}

/// Plugin group for the editor development environment.
pub struct SimDevPlugin;

impl PluginGroup for SimDevPlugin {
    fn build(self) -> PluginGroupBuilder {
        let group = PluginGroupBuilder::start::<Self>();

        // group = group.add(EditorPlugin::new()).add(|app: &mut App| {
        //     app.insert_resource(default_editor_controls());
        // });

        #[cfg(feature = "robot")]
        {
            group = group
                .add(crate::robot::editor_ui::plugin)
                .add(crate::robot::control::editor_ui::plugin)
        }

        #[cfg(feature = "physics")]
        {
            group = group.add(crate::ui::rapier_debug_render::plugin);
        }

        group
    }
}

/// Plugin group for showcasing functionality.
pub struct SimShowcasePlugin;

impl PluginGroup for SimShowcasePlugin {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group
            .add(crate::ui::showcase_window::plugin)
            // .add(crate::camera::floating_cam_editor_ui::plugin)
            ;

        group
    }
}
