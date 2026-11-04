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
    mut focus: Option<ResMut<bevy::input_focus::InputFocus>>,
    editors: Query<(), With<bevy::text::EditableText>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    touches: Option<Res<bevy::input::touch::Touches>>,
    hover_map: Option<Res<bevy::picking::hover::HoverMap>>,
    hovered: Query<&bevy::picking::hover::Hovered>,
    ui_nodes: Query<(), With<Node>>,
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
    let editing = focus
        .as_ref()
        .and_then(|f| f.get())
        .is_some_and(|e| editors.contains(e));
    let mouse_over_ui = hovered.iter().any(|hovered| hovered.0);
    let mouse_press = mouse.as_ref().is_some_and(|mouse| {
        mouse.any_just_pressed([MouseButton::Left, MouseButton::Middle, MouseButton::Right])
    }) && !mouse_over_ui;
    let touch_press = touches.as_ref().is_some_and(|touches| {
        // Use each finger's picking ownership; a stationary mouse may remain
        // over an editor while a new canvas touch starts elsewhere.
        let touch_over_ui = hover_map.as_ref().map_or(mouse_over_ui, |hover_map| {
            touches.iter().any(|touch| {
                hover_map
                    .get(&bevy::picking::pointer::PointerId::Touch(touch.id()))
                    .is_some_and(|hits| hits.keys().any(|entity| ui_nodes.contains(*entity)))
            })
        });
        touches.any_just_pressed() && !touch_over_ui
    });
    let canvas_press = mouse_press || touch_press;
    if editing
        && !panel.open
        && canvas_press
        && let Some(focus) = focus.as_mut()
    {
        focus.clear();
    }
    config.blocked = panel.open || (editing && !canvas_press);
    let frame_on_load = loaded_bounds.is_some() && std::mem::take(&mut options.initial_frame);
    if frames.read().count() > 0 || frame_on_load {
        actions.write(CameraAction::Frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_text_editor_blocks_camera_input() {
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
    fn canvas_press_releases_number_editor_but_ui_press_preserves_it() {
        let mut app = App::new();
        app.init_resource::<SplatScene>()
            .init_resource::<UrlPanel>()
            .init_resource::<NavigationConfig>()
            .init_resource::<super::super::ViewerOptions>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<SplatLoaded>()
            .add_message::<FrameSplat>()
            .add_message::<CameraAction>()
            .add_systems(Update, sync);
        let editor = app
            .world_mut()
            .spawn(bevy::text::EditableText::default())
            .id();
        let ui = app
            .world_mut()
            .spawn(bevy::picking::hover::Hovered(true))
            .id();
        app.world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(app.world().resource::<NavigationConfig>().blocked);
        assert_eq!(
            app.world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .reset_all();
        app.world_mut()
            .entity_mut(ui)
            .insert(bevy::picking::hover::Hovered(false));
        app.update();
        assert!(
            app.world().resource::<NavigationConfig>().blocked,
            "moving off UI without a new press must preserve editor focus"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(!app.world().resource::<NavigationConfig>().blocked);
        assert!(
            app.world()
                .resource::<bevy::input_focus::InputFocus>()
                .get()
                .is_none()
        );
    }

    #[test]
    fn canvas_touch_releases_editor_and_respects_per_finger_ui_ownership() {
        use bevy::{
            input::touch::{TouchInput, TouchPhase, Touches},
            picking::{backend::HitData, hover::HoverMap, pointer::PointerId},
        };
        for (ui_hit, mouse_hover, modal) in [
            (false, false, false),
            (false, true, false),
            (true, false, false),
            (false, false, true),
        ] {
            let mut app = App::new();
            app.init_resource::<SplatScene>()
                .init_resource::<UrlPanel>()
                .init_resource::<NavigationConfig>()
                .init_resource::<super::super::ViewerOptions>()
                .init_resource::<bevy::input_focus::InputFocus>()
                .init_resource::<Touches>()
                .init_resource::<HoverMap>()
                .add_message::<TouchInput>()
                .add_message::<SplatLoaded>()
                .add_message::<FrameSplat>()
                .add_message::<CameraAction>()
                .add_systems(PreUpdate, bevy::input::touch::touch_screen_input_system)
                .add_systems(Update, sync);
            let editor = app
                .world_mut()
                .spawn(bevy::text::EditableText::default())
                .id();
            let ui = app.world_mut().spawn(Node::default()).id();
            if mouse_hover {
                app.world_mut()
                    .entity_mut(ui)
                    .insert(bevy::picking::hover::Hovered(true));
            }
            let window = app.world_mut().spawn_empty().id();
            app.world_mut().resource_mut::<UrlPanel>().open = modal;
            app.world_mut()
                .resource_mut::<bevy::input_focus::InputFocus>()
                .set(editor, bevy::input_focus::FocusCause::Navigated);
            if ui_hit {
                app.world_mut()
                    .resource_mut::<HoverMap>()
                    .entry(PointerId::Touch(1))
                    .or_default()
                    .insert(ui, HitData::new(window, 0., None, None));
            }
            app.world_mut().write_message(TouchInput {
                window,
                id: 1,
                phase: TouchPhase::Started,
                position: Vec2::new(300., 400.),
                force: None,
            });
            app.update();
            let expected_blocked = ui_hit || modal;
            assert_eq!(
                app.world().resource::<NavigationConfig>().blocked,
                expected_blocked,
                "ui_hit={ui_hit}, mouse_hover={mouse_hover}, modal={modal}"
            );
            assert_eq!(
                app.world()
                    .resource::<bevy::input_focus::InputFocus>()
                    .get(),
                expected_blocked.then_some(editor)
            );
        }
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
