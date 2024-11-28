use bevy::{app::PluginGroupBuilder, log::LogPlugin, prelude::*};
use bevy_egui::EguiPlugin;
use bevy_web_asset::WebAssetPlugin;
use rapier3d::parry::simba::scalar::SupersetOf;

pub mod assets_loader;
pub mod camera;
pub mod collision_checker;
pub mod constants;
pub mod graphics;
pub mod robot;
pub mod robot_vis;
pub mod scene;
pub mod sketching;
pub mod test_scene;
pub mod ui;
pub mod util;
// pub mod camera3d;

pub struct SimPlugin;

impl PluginGroup for SimPlugin {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        #[cfg(target_arch = "wasm32")]
        let primary_window = Some(Window {
            // fit_canvas_to_parent: true,
            canvas: Some("#bevy".to_string()),
            mode: bevy::window::WindowMode::Windowed,
            prevent_default_event_handling: true,
            title: "RobotSim".to_string(),

            #[cfg(feature = "perftest")]
            present_mode: bevy::window::PresentMode::AutoNoVsync,
            #[cfg(not(feature = "perftest"))]
            present_mode: bevy::window::PresentMode::AutoVsync,

            ..default()
        });

        #[cfg(not(target_arch = "wasm32"))]
        let primary_window = Some(Window {
            mode: bevy::window::WindowMode::Windowed,
            prevent_default_event_handling: false,
            // resolution: (config.width, config.height).into(),
            resizable: true,
            // cursor_visible: true,
            // present_mode: PresentMode::AutoVsync,
            // This will spawn an invisible window
            fit_canvas_to_parent: true, // no more need to handle this myself with wasm binding: https://github.com/bevyengine/bevy/commit/fed93a0edce9d66586dc70c1207a2092694b9a7d

            title: "RobotSim".to_string(),
            present_mode: bevy::window::PresentMode::AutoVsync,
            ..default()
        });

        group = group
            .add(WebAssetPlugin {
                cache_resource: true,
            })
            .add_group(
                DefaultPlugins
                    .set(WindowPlugin {
                        primary_window,
                        ..default()
                    })
                    .set(LogPlugin {
                        filter: "bevy_render=info,bevy_ecs=trace,bevy=info".to_string(),
                        ..default()
                    }),
            )
            // .add_plugins(web_demo::plugin)
            .add(graphics::plugin)
            .add(robot::plugin::plugin)
            .add(ui::plugin);

        #[cfg(feature = "gspat")]
        {
            // use scene::gaussian_splatting::plugin;
            group = group.add(scene::gaussian_splatting::plugin);
        }

        // if !group.is_in_subset::<EguiPlugin>() {
        //     group = group.add(EguiPlugin);
        // }

        group = group
            // .add_plugins(EguiPlugin)
            .add(camera::plugin) // camera needs egui to be added first
            .add(scene::plugin)
            .add(robot_vis::plugin)
            .add(sketching::plugin);

        group
    }
}
