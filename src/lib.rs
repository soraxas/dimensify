use bevy::{app::PluginGroupBuilder, prelude::*};
// use bevy_web_asset::WebAssetPlugin;

#[cfg(feature = "robot")]
pub mod urdf_assets_loader;

pub mod camera;

pub mod constants;
pub mod coordinate_system;
pub mod graphics;

#[cfg(feature = "physics")]
pub mod physics;

#[cfg(feature = "physics")]
pub mod collision_checker;

pub mod reexport;

#[cfg(feature = "robot")]
pub mod robot;
pub mod scene;
pub mod test_scene;
pub mod ui;
pub mod util;

use bevy_editor_pls::EditorPlugin;
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
            .add(ui::plugin);

        #[cfg(feature = "robot")]
        {
            group = group.add(robot::plugin);
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
        let mut group = PluginGroupBuilder::start::<Self>();

        group = group.add(EditorPlugin::new()).add(|app: &mut App| {
            app.insert_resource(default_editor_controls());
        });

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

fn default_editor_controls() -> bevy_editor_pls::controls::EditorControls {
    use bevy_editor_pls::controls::*;
    let mut editor_controls = EditorControls::default_bindings();
    editor_controls.unbind(Action::PlayPauseEditor);
    editor_controls.insert(
        Action::PlayPauseEditor,
        Binding {
            input: UserInput::Single(Button::Keyboard(KeyCode::KeyQ)),
            conditions: vec![BindingCondition::ListeningForText(false)],
        },
    );
    editor_controls
}
