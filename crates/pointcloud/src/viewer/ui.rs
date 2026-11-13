//! Shared viewer chrome; point-cloud loading and rendering stay in their own modules.
use super::*;
use bevy::{
    feathers::{
        controls::{ButtonVariant, FeathersButton},
        theme::ThemedText,
    },
    input_focus::InputFocus,
    picking::hover::Hovered,
    ui_widgets::Activate,
};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    File,
    Frame,
    Reset,
    Mode,
    Grid,
    Url,
    Settings,
}
#[derive(Component)]
pub(super) struct ModeLabel;
pub(super) use dimensify_ui::viewer::{StatusCard, Toolbar};

pub(super) fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            Toolbar,
            AccessibleLabel("Point-cloud toolbar".into()),
            style::toolbar(),
            style::header(),
            Hovered::default(),
            GlobalZIndex(30),
        ))
        .with_children(|bar| {
            bar.spawn(style::text(&assets, "Dimensify Point Cloud Viewer", 18.));
            for (action, label) in [
                (Action::File, "Open File"),
                (Action::Frame, "Frame"),
                (Action::Reset, "Reset"),
                (Action::Grid, "Grid: Off"),
                (Action::Mode, "Mode: Orbit"),
                (Action::Url, "Open URL"),
                (Action::Settings, "Settings"),
            ] {
                let parent = bar.target_entity();
                bar.commands_mut()
                    .spawn_scene(bsn! {
                        @FeathersButton { @variant: {if action == Action::File { ButtonVariant::Primary } else { ButtonVariant::Normal }} }
                        AccessibleLabel({label.to_owned()})
                    })
                    .insert((action, ChildOf(parent), style::button()))
                    .with_children(|button| {
                        let mut text = button.spawn((Text::new(label), ThemedText));
                        if action == Action::Grid {
                            text.insert(dimensify_ui::viewer::GridLabel);
                        }
                        if action == Action::Mode {
                            text.insert(ModeLabel);
                        }
                    });
            }
        });
    commands
        .spawn((
            StatusCard,
            AccessibleLabel("Point-cloud status".into()),
            style::panel(),
            style::status_node(),
        ))
        .with_children(|bar| {
            bar.spawn((StatusText, style::text(&assets, "Loading point cloud", 14.)));
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn activate(
    event: On<Activate>,
    actions: Query<&Action>,
    mut camera: MessageWriter<CameraAction>,
    mut panel: ResMut<open::OpenPanel>,
    mut settings: ResMut<settings::Settings>,
    mut files: ResMut<open::files::Files>,
    mut focus: ResMut<InputFocus>,
    mut grid: ResMut<dimensify_ui::viewer::ViewerGrid>,
) {
    let Ok(action) = actions.get(event.entity) else {
        return;
    };
    focus.clear();
    match action {
        Action::Grid => grid.enabled = !grid.enabled,
        Action::Frame => {
            camera.write(CameraAction::Frame);
        }
        Action::Reset => {
            camera.write(CameraAction::Reset);
        }
        Action::Mode => {
            camera.write(CameraAction::ToggleMode);
        }
        Action::File => {
            panel.submit = false;
            files.choose(false, panel.up);
        }
        Action::Url => {
            panel.open = !panel.open;
            panel.submit = false;
            settings.close();
        }
        Action::Settings => {
            settings.open = !settings.open;
            panel.open = false;
            panel.submit = false;
        }
    }
}

pub(super) fn sync(
    mode: Res<NavigationMode>,
    windows: Query<&Window>,
    mut labels: Query<&mut Text, With<ModeLabel>>,
    mut status: Query<&mut TextFont, With<StatusText>>,
) {
    for mut text in &mut labels {
        text.0 = format!("Mode: {:?}", *mode);
    }
    let size = if windows.iter().next().is_some_and(|w| w.width() < 600.) {
        12.
    } else {
        14.
    };
    for mut font in &mut status {
        font.font_size = FontSize::Px(size);
    }
}

// Browser checks use actual laid-out controls, including scrolling and DPI.
#[cfg(target_arch = "wasm32")]
pub(super) fn browser_locations(
    controls: Query<(
        &AccessibleLabel,
        &ComputedNode,
        &UiGlobalTransform,
        &InheritedVisibility,
    )>,
) {
    let entries = js_sys::Array::new();
    for (label, node, transform, visibility) in &controls {
        if node.is_empty() || !visibility.get() {
            continue;
        }
        let center = transform.translation * node.inverse_scale_factor();
        let entry = js_sys::Object::new();
        for (key, value) in [
            ("label", wasm_bindgen::JsValue::from_str(&label.0)),
            ("x", center.x.into()),
            ("y", center.y.into()),
            (
                "width",
                (node.size().x * node.inverse_scale_factor()).into(),
            ),
            (
                "height",
                (node.size().y * node.inverse_scale_factor()).into(),
            ),
        ] {
            let _ = js_sys::Reflect::set(&entry, &key.into(), &value);
        }
        entries.push(&entry);
    }
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(&window, &"__dimensifyPointcloudControls".into(), &entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::scene::ScenePlugin,
        ))
        .init_asset::<Font>()
        .insert_resource(settings::Settings::new(fixture_source().settings.splats))
        .init_resource::<open::OpenPanel>()
        .init_resource::<open::files::Files>()
        .init_resource::<InputFocus>()
        .init_resource::<dimensify_ui::viewer::ViewerGrid>()
        .add_message::<CameraAction>()
        .add_observer(activate)
        .add_systems(Startup, setup);
        app.update();
        app
    }
    fn click(app: &mut App, action: Action) {
        let entity = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, candidate)| **candidate == action)
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity });
    }
    #[test]
    fn grid_toolbar_toggles_shared_state() {
        let mut app = app();
        assert!(
            !app.world()
                .resource::<dimensify_ui::viewer::ViewerGrid>()
                .enabled
        );
        let entity = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, action)| **action == Action::Grid)
            .unwrap()
            .0;
        for enabled in [true, false] {
            app.world_mut().trigger(Activate { entity });
            assert_eq!(
                app.world()
                    .resource::<dimensify_ui::viewer::ViewerGrid>()
                    .enabled,
                enabled
            );
        }
    }

    #[test]
    fn mode_label_shows_only_the_navigation_mode() {
        let mut app = App::new();
        app.insert_resource(NavigationMode::Orbit)
            .add_systems(Update, sync);
        let label = app.world_mut().spawn((ModeLabel, Text::default())).id();
        for (mode, expected) in [
            (NavigationMode::Orbit, "Mode: Orbit"),
            (NavigationMode::Fly, "Mode: Fly"),
        ] {
            *app.world_mut().resource_mut::<NavigationMode>() = mode;
            app.update();
            assert_eq!(app.world().get::<Text>(label).unwrap().0, expected);
        }
    }

    #[test]
    fn toolbar_exposes_all_seven_viewer_actions() {
        let mut app = app();
        let mut labels: Vec<_> = app
            .world_mut()
            .query::<(&Action, &AccessibleLabel)>()
            .iter(app.world())
            .map(|(_, label)| label.0.as_str())
            .collect();
        labels.sort();
        assert_eq!(
            labels,
            [
                "Frame",
                "Grid: Off",
                "Mode: Orbit",
                "Open File",
                "Open URL",
                "Reset",
                "Settings"
            ]
        );
    }
    #[test]
    fn toolbar_camera_actions_dispatch_and_clear_editor_focus() {
        for (action, expected) in [
            (Action::Frame, "Frame"),
            (Action::Reset, "Reset"),
            (Action::Mode, "ToggleMode"),
        ] {
            let mut app = app();
            let editor = app.world_mut().spawn_empty().id();
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(editor, bevy::input_focus::FocusCause::Navigated);
            click(&mut app, action);
            assert_eq!(app.world().resource::<InputFocus>().get(), None);
            let events = app.world().resource::<Messages<CameraAction>>();
            let mut cursor = events.get_cursor();
            assert_eq!(
                cursor
                    .read(events)
                    .map(|a| format!("{a:?}"))
                    .collect::<Vec<_>>(),
                [expected]
            );
        }
    }
    #[test]
    fn toolbar_panels_are_mutually_exclusive_and_clear_focus() {
        let mut app = app();
        click(&mut app, Action::Url);
        assert!(app.world().resource::<open::OpenPanel>().open);
        let editor = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        click(&mut app, Action::Settings);
        assert!(app.world().resource::<settings::Settings>().open);
        assert!(!app.world().resource::<open::OpenPanel>().open);
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
        click(&mut app, Action::Url);
        assert!(!app.world().resource::<settings::Settings>().open);
        assert!(app.world().resource::<open::OpenPanel>().open);
        click(&mut app, Action::Url);
        assert!(!app.world().resource::<open::OpenPanel>().open);
        click(&mut app, Action::Settings);
        click(&mut app, Action::Settings);
        assert!(!app.world().resource::<settings::Settings>().open);
    }
}
