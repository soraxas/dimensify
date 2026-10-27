use super::{ViewerOptions, files::LocalFiles};
use bevy::{
    feathers::{
        controls::{FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        theme::ThemedText,
    },
    input_focus::{FocusCause, InputFocus},
    picking::hover::Hovered,
    prelude::*,
    text::EditableText,
    ui_widgets::Activate,
};

#[derive(Resource, Default)]
pub(super) struct UrlPanel {
    pub open: bool,
}
#[derive(Component)]
pub(super) struct PanelRoot;
#[derive(Component, Clone, Default)]
pub(super) struct UrlField;
#[derive(Component, Clone, Copy)]
enum Action {
    Load,
    Cancel,
}

/// Build one retained URL entry panel for native and browser viewers.
pub(super) fn setup(mut commands: Commands) {
    commands.spawn((PanelRoot, Hovered::default(), Node {
        display: Display::None,
        position_type: PositionType::Absolute,
        top: px(150), left: percent(15), right: percent(15),
        padding: UiRect::all(px(20)), row_gap: px(14),
        flex_direction: FlexDirection::Column,
        border_radius: BorderRadius::all(px(10)), ..default()
    }, GlobalZIndex(10), BackgroundColor(Color::srgb(0.07, 0.11, 0.17))))
    .with_children(|panel| {
        panel.spawn((Text::new("Open a direct .ply, .sog or lod-meta.json URL"), ThemedText));
        let parent = panel.target_entity();
        panel.commands_mut().spawn_scene(bsn! {
            @FeathersTextInputContainer
            Children [
                (
                    @FeathersTextInput { @max_characters: {Some(8192)} }
                    UrlField
                )
            ]
        }).insert(ChildOf(parent));
        panel.spawn((Text::new("HTTP(S) link | Enter: load | Esc: cancel | Browser hosts must allow CORS"), ThemedText));
        panel.spawn(Node { column_gap: px(12), ..default() }).with_children(|row| {
            for (action, label) in [(Action::Load, "Load URL"), (Action::Cancel, "Cancel")] {
                let parent = row.target_entity();
                row.commands_mut().spawn_scene(bsn! { @FeathersButton Node { min_width: px(108), height: px(36) } on(buttons) })
                    .insert((action, ChildOf(parent))).with_children(|button| { button.spawn((Text::new(label), ThemedText)); });
            }
        });
    });
}

fn buttons(
    event: On<Activate>,
    actions: Query<&Action>,
    fields: Query<&EditableText, With<UrlField>>,
    options: Res<ViewerOptions>,
    mut panel: ResMut<UrlPanel>,
    mut files: ResMut<LocalFiles>,
) {
    if let Ok(action) = actions.get(event.entity) {
        if matches!(action, Action::Load)
            && let Ok(field) = fields.single()
        {
            files.open_url(&field.value().to_string(), options.up);
        }
        panel.open = false;
    }
}

/// Synchronize panel visibility, initial text focus and submit/cancel shortcuts.
#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    mut panel: ResMut<UrlPanel>,
    mut roots: Query<&mut Node, With<PanelRoot>>,
    fields: Query<(Entity, &EditableText), With<UrlField>>,
    mut focus: ResMut<InputFocus>,
    keys: Res<ButtonInput<KeyCode>>,
    options: Res<ViewerOptions>,
    mut files: ResMut<LocalFiles>,
    mut was_open: Local<bool>,
) {
    if panel.open
        && let Ok((entity, field)) = fields.single()
    {
        if !*was_open {
            focus.set(entity, FocusCause::Navigated);
        }
        if keys.just_pressed(KeyCode::Escape) {
            panel.open = false;
        } else if *was_open && focus.get() == Some(entity) && keys.just_pressed(KeyCode::Enter) {
            files.open_url(&field.value().to_string(), options.up);
            panel.open = false;
        }
    }
    if *was_open && !panel.open {
        focus.clear();
    }
    for mut root in &mut roots {
        let display = if panel.open {
            Display::Flex
        } else {
            Display::None
        };
        if root.display != display {
            root.display = display;
        }
    }
    *was_open = panel.open;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enter_that_opens_the_panel_does_not_submit_it() {
        let mut app = App::new();
        app.init_resource::<LocalFiles>()
            .init_resource::<ViewerOptions>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(UrlPanel { open: true })
            .add_systems(Update, update);
        app.world_mut().spawn((UrlField, EditableText::default()));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert!(
            app.world().resource::<UrlPanel>().open,
            "activation Enter must only open the panel"
        );
        assert!(app.world().resource::<LocalFiles>().notice.is_none());
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::Enter);
        keys.press(KeyCode::Enter);
        app.update();
        assert!(
            !app.world().resource::<UrlPanel>().open,
            "a fresh field-focused Enter submits"
        );
        assert!(app.world().resource::<LocalFiles>().notice.is_some());
    }
}

#[cfg(target_arch = "wasm32")]
/// Adapt browser Command shortcuts to the shared editable-field intents.
pub(super) fn browser_commands(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut fields: Query<&mut EditableText, With<UrlField>>,
) {
    use bevy::{
        input::{ButtonState, keyboard::Key},
        text::TextEdit,
    };
    if !event.input.state.eq(&ButtonState::Pressed)
        || !(keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight))
    {
        return;
    }
    let Ok(mut field) = fields.get_mut(event.focused_entity) else {
        return;
    };
    let Key::Character(key) = &event.input.logical_key else {
        return;
    };
    let edit = match key.to_ascii_lowercase().as_str() {
        "a" => TextEdit::SelectAll,
        "c" => TextEdit::Copy,
        "x" => TextEdit::Cut,
        "v" => TextEdit::Paste,
        _ => return,
    };
    field.queue_edit(edit);
    event.propagate(false);
}
