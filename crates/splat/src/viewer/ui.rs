use super::navigation::{CameraAction, NavigationMode};
use super::{FrameSplat, ViewerOptions, files::LocalFiles};
use crate::{SetSplatUp, SourceUp, SplatScene};
use bevy::prelude::*;
use bevy::{
    feathers::{controls::FeathersButton, theme::ThemedText},
    picking::hover::Hovered,
    ui_widgets::Activate,
};

#[derive(Component, Clone, Copy)]
pub(super) enum Action {
    Open,
    Url,
    Frame,
    Reset,
    Mode,
    Up,
    Transparency,
    Settings,
}
#[derive(Component)]
pub(super) struct Status;
#[derive(Component)]
pub(super) struct DropZone;
#[derive(Component)]
pub(super) struct UpLabel;
#[derive(Component)]
pub(super) struct ModeLabel;
#[derive(Component)]
pub(super) struct ControlsLabel;
#[derive(Component)]
pub(super) struct TransparencyLabel;

fn text(value: impl Into<String>, size: f32) -> (Text, TextFont, TextColor) {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.94, 1.)),
    )
}

/// Build the shared Feathers toolbar, empty drop zone and status bar.
pub(super) fn setup(
    mut commands: Commands,
    options: Res<ViewerOptions>,
    mode: Res<crate::runtime::SplatTransparency>,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(16),
                left: px(16),
                right: px(16),
                padding: UiRect::all(px(12)),
                column_gap: px(16),
                row_gap: px(8),
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
            Hovered::default(),
        ))
        .with_children(|bar| {
            bar.spawn(text("Dimensify Splat Viewer", 18.));
            for (action, label) in [
                (Action::Open, "Open File"),
                (Action::Frame, "Frame"),
                (Action::Reset, "Reset"),
                (Action::Mode, "Mode: Fly"),
                (Action::Up, &format!("File up axis: {}", options.up.label())),
                (Action::Url, "Open URL"),
                (Action::Transparency, transparency_label(mode.0)),
                (Action::Settings, "Settings"),
            ] {
                let parent = bar.target_entity();
                bar.commands_mut()
                    .spawn_scene(bsn! {
                        @FeathersButton
                        Node { min_width: px(108), height: px(44) }
                        on(buttons)
                    })
                    .insert((action, ChildOf(parent)))
                    .with_children(|button| {
                        let mut label_entity = button.spawn((Text::new(label), ThemedText));
                        if matches!(action, Action::Up) {
                            label_entity.insert(UpLabel);
                        }
                        if matches!(action, Action::Transparency) {
                            label_entity.insert(TransparencyLabel);
                        }
                        if matches!(action, Action::Mode) {
                            label_entity.insert(ModeLabel);
                        }
                    });
            }
        });
    commands
        .spawn((
            DropZone,
            Node {
                position_type: PositionType::Absolute,
                top: percent(32),
                left: percent(20),
                right: percent(20),
                padding: UiRect::all(px(42)),
                border: UiRect::all(px(2)),
                border_radius: BorderRadius::all(px(14)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(16),
                ..default()
            },
            BorderColor::all(Color::srgb(0.35, 0.55, 0.75)),
            BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
        ))
        .with_children(|zone| {
            zone.spawn(text("Drop a PLY or SOG file here", 24.));
            zone.spawn(text("or use Open File to browse your computer", 16.));
            zone.spawn(text(
                "Use Frame to fit the file, or fly from the origin.",
                13.,
            ));
        });
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                right: px(16),
                bottom: px(16),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
        ))
        .with_children(|bar| {
            bar.spawn((
                ControlsLabel,
                text(
                    "Fly: WASD + Q/E | Drag: look | Touch: drag to look, two fingers to pan/pinch",
                    12.,
                ),
            ));
            bar.spawn(text(
                "File up axis chooses the file's vertical axis and rotates the loaded splat.",
                12.,
            ));
            bar.spawn((Status, text("Open a Gaussian PLY or bundled SOG file", 14.)));
        });
}

/// Turn toolbar interactions into file-selection, framing and source-frame intents.
#[allow(clippy::too_many_arguments)]
fn buttons(
    event: On<Activate>,
    actions: Query<&Action>,
    mut files: ResMut<LocalFiles>,
    mut options: ResMut<ViewerOptions>,
    mut frames: MessageWriter<FrameSplat>,
    mut labels: Query<&mut Text, With<UpLabel>>,
    mut axes: MessageWriter<SetSplatUp>,
    mut cameras: MessageWriter<CameraAction>,
    mut panel: ResMut<super::url_panel::UrlPanel>,
    mut render_mode: ResMut<crate::runtime::SplatTransparency>,
    mut settings: ResMut<super::settings::ViewerSettings>,
) {
    if let Ok(action) = actions.get(event.entity) {
        match action {
            Action::Settings => settings.open = !settings.open,
            Action::Transparency if settings.applying => {}
            Action::Transparency => {
                render_mode.0 = match render_mode.0 {
                    bevy_gaussian_splatting::GaussianTransparency::Sorted => {
                        bevy_gaussian_splatting::GaussianTransparency::Stochastic
                    }
                    bevy_gaussian_splatting::GaussianTransparency::Stochastic => {
                        bevy_gaussian_splatting::GaussianTransparency::Sorted
                    }
                };
            }
            Action::Open => files.choose(options.up),
            Action::Url => panel.open = !panel.open,
            Action::Frame => {
                frames.write(FrameSplat);
            }
            Action::Reset => {
                cameras.write(CameraAction::Reset);
            }
            Action::Mode => {
                cameras.write(CameraAction::ToggleMode);
            }
            Action::Up => {
                options.up = match options.up {
                    SourceUp::Z => SourceUp::Y,
                    SourceUp::Y => SourceUp::X,
                    SourceUp::X => SourceUp::NegativeX,
                    SourceUp::NegativeX => SourceUp::NegativeY,
                    SourceUp::NegativeY => SourceUp::NegativeZ,
                    SourceUp::NegativeZ => SourceUp::Z,
                };
                axes.write(SetSplatUp(options.up));
                for mut label in &mut labels {
                    **label = format!("File up axis: {}", options.up.label());
                }
            }
        }
    }
}

fn transparency_label(mode: bevy_gaussian_splatting::GaussianTransparency) -> &'static str {
    match mode {
        bevy_gaussian_splatting::GaussianTransparency::Sorted => "Render: Sorted",
        bevy_gaussian_splatting::GaussianTransparency::Stochastic => "Render: Stochastic",
    }
}

pub(super) fn update_transparency_label(
    mut commands: Commands,
    mode: Res<crate::runtime::SplatTransparency>,
    settings: Res<super::settings::ViewerSettings>,
    actions: Query<(Entity, &Action)>,
    mut labels: Query<&mut Text, With<TransparencyLabel>>,
) {
    for (entity, action) in &actions {
        if matches!(action, Action::Transparency) {
            if settings.applying {
                commands
                    .entity(entity)
                    .insert(bevy::ui::InteractionDisabled);
            } else {
                commands
                    .entity(entity)
                    .remove::<bevy::ui::InteractionDisabled>();
            }
        }
    }
    for mut text in &mut labels {
        let value = transparency_label(mode.0);
        if text.0 != value {
            **text = value.into();
        }
    }
}

/// Show loading/errors and retain a drop overlay while hovering with a file.
#[allow(clippy::type_complexity)]
pub(super) fn status(
    scene: Res<SplatScene>,
    mode: Res<NavigationMode>,
    mut mode_texts: Query<&mut Text, (With<ModeLabel>, Without<Status>, Without<ControlsLabel>)>,
    mut controls: Query<&mut Text, (With<ControlsLabel>, Without<Status>, Without<ModeLabel>)>,
    files: Res<LocalFiles>,
    mut texts: Query<&mut Text, (With<Status>, Without<ModeLabel>, Without<ControlsLabel>)>,
    mut zones: Query<&mut Node, With<DropZone>>,
) {
    for mut text in &mut mode_texts {
        let value = if *mode == NavigationMode::Fly {
            "Mode: Fly"
        } else {
            "Mode: Orbit"
        };
        if text.0 != value {
            **text = value.into();
        }
    }
    for mut text in &mut controls {
        let value = if *mode == NavigationMode::Fly {
            "Fly: WASD + Q/E | Drag: look | Touch: drag to look, two fingers to pan/pinch"
        } else {
            "Orbit: drag | Pan: right drag | Touch: one finger orbit, two fingers pan/pinch"
        };
        if text.0 != value {
            **text = value.into();
        }
    }
    let label = files
        .notice
        .clone()
        .unwrap_or_else(|| super::status_text(&scene));
    let mut changed = false;
    for mut text in &mut texts {
        if **text != label {
            **text = label.clone();
            changed = true;
        }
    }
    if changed {
        info!(target: "dimensify_splat::viewer", "{label}");
    }
    for mut zone in &mut zones {
        let display = if scene.loaded.is_none() || files.hovering {
            Display::Flex
        } else {
            Display::None
        };
        if zone.display != display {
            zone.display = display;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ui_widgets::Activate;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::scene::ScenePlugin,
        ))
        .init_asset::<Font>()
        .init_resource::<ViewerOptions>()
        .init_resource::<super::super::settings::ViewerSettings>()
        .init_resource::<crate::runtime::SplatTransparency>()
        .init_resource::<LocalFiles>()
        .init_resource::<super::super::url_panel::UrlPanel>()
        .add_message::<FrameSplat>()
        .add_message::<CameraAction>()
        .add_message::<SetSplatUp>()
        .init_resource::<NavigationMode>()
        .add_systems(Startup, setup)
        .add_systems(Update, update_transparency_label);
        app.update();
        app
    }

    #[test]
    fn settings_panel_is_available_without_a_loaded_scene() {
        let mut app = app();
        let button = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, action)| matches!(action, Action::Settings))
            .unwrap()
            .0;
        for open in [true, false] {
            app.world_mut().trigger(Activate { entity: button });
            app.update();
            assert_eq!(
                app.world()
                    .resource::<super::super::settings::ViewerSettings>()
                    .open,
                open
            );
        }
    }

    #[test]
    fn render_toolbar_is_visibly_disabled_and_ignores_activation_during_reload() {
        let mut app = app();
        let button = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, action)| matches!(action, Action::Transparency))
            .unwrap()
            .0;
        app.world_mut()
            .resource_mut::<super::super::settings::ViewerSettings>()
            .applying = true;
        app.update();
        assert!(
            app.world()
                .get::<bevy::ui::InteractionDisabled>(button)
                .is_some()
        );
        app.world_mut().trigger(Activate { entity: button });
        assert_eq!(
            app.world()
                .resource::<crate::runtime::SplatTransparency>()
                .0,
            bevy_gaussian_splatting::GaussianTransparency::Sorted
        );
        app.world_mut()
            .resource_mut::<super::super::settings::ViewerSettings>()
            .applying = false;
        app.update();
        assert!(
            app.world()
                .get::<bevy::ui::InteractionDisabled>(button)
                .is_none()
        );
    }

    #[test]
    fn render_button_switches_transparency_without_reloading_scene() {
        let mut app = app();
        let label = app
            .world_mut()
            .query::<(Entity, &Text)>()
            .iter(app.world())
            .find(|(_, text)| text.0 == "Render: Sorted")
            .map(|(entity, _)| entity)
            .expect("viewer needs a readable render setting");
        let button = app.world().get::<ChildOf>(label).unwrap().parent();
        app.world_mut().trigger(Activate { entity: button });
        app.update();
        assert_eq!(
            app.world().get::<Text>(label).unwrap().0,
            "Render: Stochastic"
        );
        app.world_mut().trigger(Activate { entity: button });
        app.update();
        assert_eq!(app.world().get::<Text>(label).unwrap().0, "Render: Sorted");
    }

    #[test]
    fn source_button_activation_updates_the_source_frame_and_visible_label() {
        let mut app = app();
        let button = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, a)| matches!(a, Action::Up))
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: button });
        assert_eq!(app.world().resource::<ViewerOptions>().up, SourceUp::Y);
        let label = app
            .world_mut()
            .query_filtered::<&Text, With<UpLabel>>()
            .single(app.world())
            .unwrap();
        assert_eq!(label.0, "File up axis: +Y");
    }

    #[test]
    fn frame_button_activation_emits_a_camera_framing_request() {
        let mut app = app();
        let button = app
            .world_mut()
            .query::<(Entity, &Action)>()
            .iter(app.world())
            .find(|(_, a)| matches!(a, Action::Frame))
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: button });
        assert_eq!(app.world().resource::<Messages<FrameSplat>>().len(), 1);
    }
    #[test]
    fn reset_and_mode_buttons_emit_camera_intents() {
        for (action, expected) in [
            (Action::Reset, super::super::navigation::CameraAction::Reset),
            (
                Action::Mode,
                super::super::navigation::CameraAction::ToggleMode,
            ),
        ] {
            let mut app = app();
            let button = app
                .world_mut()
                .query::<(Entity, &Action)>()
                .iter(app.world())
                .find(|(_, a)| std::mem::discriminant(*a) == std::mem::discriminant(&action))
                .unwrap()
                .0;
            app.world_mut().trigger(Activate { entity: button });
            let messages = app
                .world()
                .resource::<Messages<super::super::navigation::CameraAction>>();
            assert_eq!(
                messages.iter_current_update_messages().last(),
                Some(&expected)
            );
        }
    }
}
