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

pub struct DimensifyPlugin {
    pub infinite_grid: bool,
    pub with_sun: bool,
    pub with_ambient_light: bool,
    pub with_ui: bool,
    #[cfg(feature = "gsplat")]
    pub gaussian_splatting: bool,
    #[cfg(feature = "robot")]
    pub robot: bool,
    #[cfg(feature = "physics")]
    pub physics: bool,
    #[cfg(feature = "transport")]
    pub transport: bool,
}

impl Default for DimensifyPlugin {
    fn default() -> Self {
        Self {
            infinite_grid: true,
            with_sun: true,
            with_ambient_light: false,
            with_ui: true,
            #[cfg(feature = "gsplat")]
            gaussian_splatting: true,
            #[cfg(feature = "robot")]
            robot: true,
            #[cfg(feature = "physics")]
            physics: true,
            #[cfg(feature = "transport")]
            transport: true,
        }
    }
}

#[cfg(feature = "gsplat")]
use bevy_gaussian_splatting::{CloudSettings, PlanarGaussian3dHandle};

impl PluginGroup for DimensifyPlugin {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group.add(graphics::plugin);
        group = group
        .add(camera::CameraPlugin {
            with_ambient_light: self.with_ambient_light,
            with_sun: self.with_sun,
        }) // camera needs egui to be added first
        // .add(scene::plugin)
        // .add(sketching::plugin)
        ;

        if self.infinite_grid {
            group = group.add(graphics::infinite_grid_plugin);
        }
        if self.with_ui {
            // group = group.add(ui::plugin);
            group = group.add(dimensify_ui::setup_ui);
        }

        #[cfg(feature = "transport")]
        {
            if self.transport {
                group = group.add(services::plugin);
            }
        }

        #[cfg(feature = "protocol")]
        {
            group = group.add(telemetry::plugin).add(stream::plugin);
        }

        // #[cfg(feature = "robot")]
        // {
        //     group = group.add(robot::RobotPlugin::default());
        // }

        // if !group.is_in_subset::<EguiPlugin>() {
        //     group = group.add(EguiPlugin);
        // }
        // .add_plugins(EguiPlugin)

        #[cfg(feature = "physics")]
        {
            group = group.add(physics::plugin);
        }

        #[cfg(feature = "gsplat")]
        {
            group = group.add(scene::gaussian_splatting::plugin);
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

        // #[cfg(feature = "robot")]
        // {
        //     group = group
        //         .add(crate::robot::editor_ui::plugin)
        //         .add(crate::robot::control::editor_ui::plugin)
        // }

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
            .add(crate::scene::showcase_window::plugin)
            // .add(crate::camera::floating_cam_editor_ui::plugin)
            ;

        group
    }
}
