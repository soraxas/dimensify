//! Splat scene and modal-input adaptation to shared viewer navigation.
use super::{FrameSplat, url_panel::UrlPanel};
use crate::{SplatLoaded, SplatScene};
use bevy::prelude::*;
use dimensify_camera::viewer::NavigationConfig;
pub(super) use dimensify_camera::viewer::{CameraAction, NavigationMode, home_pose};

/// Forward scene framing and URL capture without writing a camera pose.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync(
    scene: Res<SplatScene>,
    panel: Res<UrlPanel>,
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    editors: Query<(), With<bevy::text::EditableText>>,
    mut loaded: MessageReader<SplatLoaded>,
    mut frames: MessageReader<FrameSplat>,
    mut actions: MessageWriter<CameraAction>,
    mut config: ResMut<NavigationConfig>,
    mut options: ResMut<super::ViewerOptions>,
) {
    let loaded_bounds = loaded.read().last().map(|event| event.bounds);
    config.bounds = scene
        .loaded
        .as_ref()
        .map(|scene| scene.bounds)
        .or(loaded_bounds)
        .or(config.bounds);
    config.blocked = panel.open
        || focus
            .as_ref()
            .and_then(|f| f.get())
            .is_some_and(|e| editors.contains(e));
    let frame_on_load = loaded_bounds.is_some() && std::mem::take(&mut options.initial_frame);
    if frames.read().count() > 0 || frame_on_load {
        actions.write(CameraAction::Frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_number_editor_captures_camera_keyboard_input() {
        let mut app = App::new();
        app.init_resource::<SplatScene>()
            .init_resource::<UrlPanel>()
            .init_resource::<NavigationConfig>()
            .init_resource::<super::super::ViewerOptions>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_message::<SplatLoaded>()
            .add_message::<FrameSplat>()
            .add_message::<CameraAction>()
            .add_systems(Update, sync);
        let editor = app
            .world_mut()
            .spawn(bevy::text::EditableText::default())
            .id();
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        app.update();
        assert!(app.world().resource::<NavigationConfig>().blocked);
    }

    #[test]
    fn launch_auto_frame_waits_for_a_load_and_fires_only_once() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SplatScene>()
            .init_resource::<UrlPanel>()
            .init_resource::<NavigationConfig>()
            .insert_resource(super::super::ViewerOptions {
                initial_frame: true,
                ..default()
            })
            .add_message::<SplatLoaded>()
            .add_message::<FrameSplat>()
            .add_message::<CameraAction>()
            .add_systems(Update, sync);
        let mut cursor = app
            .world()
            .resource::<Messages<CameraAction>>()
            .get_cursor();
        app.update();
        assert_eq!(
            cursor
                .read(app.world().resource::<Messages<CameraAction>>())
                .count(),
            0
        );
        assert!(
            app.world()
                .resource::<super::super::ViewerOptions>()
                .initial_frame
        );
        for expected in [1, 0] {
            app.world_mut().write_message(SplatLoaded {
                bounds: (Vec3::NEG_ONE, Vec3::ONE),
            });
            app.update();
            assert_eq!(
                cursor
                    .read(app.world().resource::<Messages<CameraAction>>())
                    .filter(|action| matches!(action, CameraAction::Frame))
                    .count(),
                expected
            );
        }
    }
}
