use bevy::{app::PluginGroupBuilder, prelude::*};
// use bevy_web_asset::WebAssetPlugin;

pub mod assets_loader;
pub mod camera;
pub mod collision_checker;
pub mod constants;
pub mod coordinate_system;
pub mod graphics;
pub mod preset;
pub mod robot;
pub mod scene;
pub mod sketching;
pub mod test_scene;
pub mod ui;
pub mod util;
// pub mod camera3d;

pub use bevy_web_asset::WebAssetPlugin;

pub struct SimPlugin;

impl PluginGroup for SimPlugin {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group
            // .add(WebAssetPlugin {
            //     cache_resource: true,
            // })
            // .add_plugins(web_demo::plugin)
            .add(graphics::plugin)
            .add(robot::plugin)
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
            // .add(sketching::plugin)
            ;

        group
    }
}
