//! Retained Feathers connection form shared by native and browser examples.
use super::*;
use bevy::{
    feathers::{
        controls::{FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        theme::ThemedText,
    },
    input_focus::InputFocus,
    picking::hover::Hovered,
    text::EditableText,
    ui_widgets::Activate,
};
#[derive(Message, Clone, Copy)]
pub(super) enum Action {
    Create,
    Join,
    Retry,
    Close,
}
#[derive(Component)]
pub(super) struct Root;
#[derive(Component)]
pub(super) struct Notice;
#[derive(Component, Clone, Copy, PartialEq)]
pub(super) enum Field {
    Host,
    Name,
    Code,
}

/// Build the shared form once; platform-specific URL handling stays in the example adapter.
pub(super) fn setup(mut commands: Commands, options: Res<Options>) {
    commands.spawn((Root, Hovered::default(), GlobalZIndex(20), Node {
        display: Display::None, position_type: PositionType::Absolute,
        top: px(16), left: px(16), right: px(16), max_width: px(560),
        padding: UiRect::all(px(16)), row_gap: px(10), flex_direction: FlexDirection::Column,
        ..default()
    }, BackgroundColor(Color::srgb(0.07, 0.11, 0.17))))
    .with_children(|panel| {
        panel.spawn((Text::new("Connect to a collaboration host"), ThemedText));
        for (kind, label, value, limit) in [
            (Field::Host, "Host URL", options.endpoint.as_str(), 2048),
            (Field::Name, "Display name", options.name.as_str(), 64),
            (Field::Code, "Invite code (Join only)", options.code.as_str(), 32),
        ] {
            panel.spawn((Text::new(label), ThemedText));
            let parent = panel.target_entity();
            let container = panel.commands_mut().spawn_scene(bsn! {
                @FeathersTextInputContainer Node { min_height: px(44), flex_grow: 0.0 }
            }).insert(ChildOf(parent)).id();
            panel.commands_mut().spawn_scene(bsn! {
                @FeathersTextInput { @max_characters: {Some(limit)} }
            }).insert((EditableText { max_characters: Some(limit), ..EditableText::new(value) }, kind, ChildOf(container)));
        }
        panel.spawn(Node { column_gap: px(8), row_gap: px(8), flex_wrap: FlexWrap::Wrap, ..default() })
            .with_children(|row| {
                for (action,label) in [(Action::Create,"Create"), (Action::Join,"Join"), (Action::Retry,"Retry"), (Action::Close,"Close")] {
                    let parent = row.target_entity();
                    row.commands_mut().spawn_scene(bsn! { @FeathersButton Node { min_width: px(100), height: px(44) } })
                        .insert(ChildOf(parent))
                        .observe(move |_: On<Activate>, mut actions: MessageWriter<Action>| { actions.write(action); })
                        .with_children(|button| { button.spawn((Text::new(label), ThemedText)); });
                }
            });
        panel.spawn((Notice, Text::new(""), ThemedText));
        panel.spawn((Text::new("Start the room server first. Create a room, or Join with its invite code. Retry repeats your last attempt and resumes your participant when possible. Esc closes this panel."), ThemedText));
    });
}

/// Validate semantic form actions and capture navigation while the panel owns focus.
#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    mut actions: MessageReader<Action>,
    fields: Query<(&Field, &EditableText)>,
    mut panel: ResMut<connection::Panel>,
    mut demo: ResMut<Demo>,
    session: Res<ClientSession<Cube>>,
    mut roots: Query<&mut Node, With<Root>>,
    mut notices: Query<&mut Text, With<Notice>>,
    mut focus: ResMut<InputFocus>,
    keys: Res<ButtonInput<KeyCode>>,
    mut navigation: ResMut<NavigationConfig>,
) {
    let value = |kind| {
        fields
            .iter()
            .find(|(field, _)| **field == kind)
            .map(|(_, text)| text.value().to_string())
            .unwrap_or_default()
    };
    for action in actions.read() {
        match action {
            Action::Close => {
                panel.open = false;
                focus.clear();
            }
            Action::Create | Action::Join => {
                match connection::prepare(
                    &value(Field::Host),
                    &value(Field::Name),
                    &value(Field::Code),
                    matches!(action, Action::Join),
                    connection::secure_page(),
                ) {
                    Ok(attempt) => panel.pending = Some((attempt, false)),
                    Err(error) => demo.error = error,
                }
            }
            Action::Retry => {
                if let Some(attempt) = panel.last.clone() {
                    panel.pending = Some((attempt, true));
                } else {
                    demo.error = "Create or join a room before retrying.".into();
                }
            }
        }
    }
    if panel.open && keys.just_pressed(KeyCode::Escape) {
        panel.open = false;
        focus.clear();
    }
    navigation.blocked = panel.open;
    for mut root in &mut roots {
        root.display = if panel.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    let error = if !demo.error.is_empty() {
        demo.error.clone()
    } else {
        session
            .error
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    for mut notice in &mut notices {
        notice.0 = format!(
            "{:?}{}",
            session.status,
            if error.is_empty() {
                String::new()
            } else {
                format!(" | {error}")
            }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_join_keeps_active_session_and_closing_releases_camera_focus() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<connection::Panel>()
            .init_resource::<Demo>()
            .init_resource::<ClientSession<Cube>>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NavigationConfig>()
            .add_message::<Action>()
            .add_systems(Update, update);
        let host = app
            .world_mut()
            .spawn((Field::Host, EditableText::new("ws://localhost:5002")))
            .id();
        app.world_mut()
            .spawn((Field::Name, EditableText::new("guest")));
        app.world_mut().spawn((Field::Code, EditableText::new("")));
        app.world_mut().resource_mut::<connection::Panel>().open = true;
        app.world_mut().resource_mut::<ClientSession<Cube>>().status =
            dimensify_session::ConnectionStatus::Joined;
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(host, bevy::input_focus::FocusCause::Navigated);
        app.world_mut().write_message(Action::Join);
        app.update();
        assert_eq!(
            app.world().resource::<ClientSession<Cube>>().status,
            dimensify_session::ConnectionStatus::Joined
        );
        assert!(
            app.world()
                .resource::<connection::Panel>()
                .pending
                .is_none()
        );
        assert!(app.world().resource::<Demo>().error.contains("invite code"));
        assert!(app.world().resource::<NavigationConfig>().blocked);
        app.world_mut().write_message(Action::Close);
        app.update();
        assert!(!app.world().resource::<NavigationConfig>().blocked);
        assert!(app.world().resource::<InputFocus>().get().is_none());
    }
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Resource, Default)]
pub(super) struct CommandKeys {
    left: bool,
    right: bool,
}
#[cfg(any(target_arch = "wasm32", test))]
impl CommandKeys {
    fn edit(
        &mut self,
        input: &bevy::input::keyboard::KeyboardInput,
    ) -> Option<bevy::text::TextEdit> {
        use bevy::{
            input::{ButtonState, keyboard::Key},
            text::TextEdit,
        };
        let pressed = input.state == ButtonState::Pressed;
        match input.key_code {
            KeyCode::SuperLeft => {
                self.left = pressed;
                return None;
            }
            KeyCode::SuperRight => {
                self.right = pressed;
                return None;
            }
            _ => {}
        }
        if !pressed || !(self.left || self.right) {
            return None;
        }
        let Key::Character(key) = &input.logical_key else {
            return None;
        };
        match key.to_ascii_lowercase().as_str() {
            "a" => Some(TextEdit::SelectAll),
            "c" => Some(TextEdit::Copy),
            "x" => Some(TextEdit::Cut),
            "v" => Some(TextEdit::Paste),
            _ => None,
        }
    }
}

/// Install the platform shortcut adapter without replacing retained native/WASM fields.
pub(super) fn configure(app: &mut App) {
    #[cfg(target_arch = "wasm32")]
    app.init_resource::<CommandKeys>()
        .add_observer(browser_commands)
        .add_systems(
            PreUpdate,
            clear_commands_on_focus_loss.before(bevy::input_focus::InputFocusSystems::Dispatch),
        );
    #[cfg(not(target_arch = "wasm32"))]
    let _ = app;
}

#[cfg(target_arch = "wasm32")]
fn clear_commands_on_focus_loss(
    mut events: MessageReader<bevy::input::keyboard::KeyboardFocusLost>,
    mut keys: ResMut<CommandKeys>,
) {
    if events.read().count() > 0 {
        *keys = CommandKeys::default();
    }
}

#[cfg(target_arch = "wasm32")]
/// Adapt ordered browser Command chords, including press/release within one frame.
pub(super) fn browser_commands(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    mut keys: ResMut<CommandKeys>,
    mut fields: Query<&mut EditableText, With<Field>>,
) {
    let edit = keys.edit(&event.input);
    let Ok(mut field) = fields.get_mut(event.focused_entity) else {
        return;
    };
    if let Some(edit) = edit {
        field.queue_edit(edit);
        event.propagate(false);
    }
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    use bevy::input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    };
    fn input(key_code: KeyCode, pressed: bool) -> KeyboardInput {
        KeyboardInput {
            key_code,
            logical_key: Key::Character("a".into()),
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }
    #[test]
    fn fast_command_chord_is_ordered_and_does_not_capture_later_text() {
        let mut keys = CommandKeys::default();
        assert!(keys.edit(&input(KeyCode::KeyA, true)).is_none());
        assert!(keys.edit(&input(KeyCode::SuperLeft, true)).is_none());
        assert!(matches!(
            keys.edit(&input(KeyCode::KeyA, true)),
            Some(bevy::text::TextEdit::SelectAll)
        ));
        assert!(keys.edit(&input(KeyCode::KeyA, false)).is_none());
        assert!(keys.edit(&input(KeyCode::SuperLeft, false)).is_none());
        assert!(keys.edit(&input(KeyCode::KeyA, true)).is_none());
    }
}
