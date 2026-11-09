use super::*;
use bevy::{
    feathers::{
        controls::{FeathersButton, FeathersTextInput, FeathersTextInputContainer},
        theme::ThemedText,
    },
    input_focus::FocusCause,
    picking::hover::Hovered,
    text::{EditableText, LineBreak, TextEdit},
    ui_widgets::Activate,
};

#[derive(Component, Clone, Default)]
pub(super) struct UrlField;
#[derive(Component)]
pub(super) struct PanelRoot;
#[derive(Component)]
pub(super) enum Caption {
    Notice,
    Source,
    Up,
}
#[derive(Component, Clone, Copy)]
pub(super) enum Action {
    Open,
    Load,
    Close,
    Up,
    Demo,
    File,
    Folder,
}

fn button(parent: &mut ChildSpawnerCommands, label: &str, action: Action) {
    let owner = parent.target_entity();
    parent.commands_mut().spawn_scene(bsn! {
        @FeathersButton
        AccessibleLabel({label.to_owned()})
        Node { min_height: px(44), min_width: px(80), flex_shrink: 0., padding: UiRect::horizontal(px(8)) }
    }).insert((action, ChildOf(owner))).with_children(|p| {
        let mut caption = p.spawn((Text::new(label), ThemedText));
        if matches!(action, Action::Up) { caption.insert(Caption::Up); }
    });
}

pub(super) fn setup(
    mut commands: Commands,
    source: Res<ViewerSource>,
    mut state: ResMut<OpenPanel>,
) {
    state.up = source.0.up;
    commands
        .spawn((
            GlobalZIndex(30),
            Hovered::default(),
            Node {
                position_type: PositionType::Absolute,
                left: px(140),
                bottom: px(12),
                ..default()
            },
        ))
        .with_children(|p| button(p, "Open", Action::Open));
    commands.spawn((PanelRoot, Hovered::default(), GlobalZIndex(40), ScrollPosition::default(),
        BackgroundColor(Color::srgb(0.055, 0.065, 0.08)), Node {
            display: Display::None, position_type: PositionType::Absolute,
            left: px(12), top: px(80), bottom: px(76), width: px(420), max_width: percent(94),
            padding: UiRect::all(px(14)), row_gap: px(12), flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(), ..default()
        }))
        .observe(drag_scroll)
        .with_children(|p| {
            p.spawn((Text::new("Open point cloud"), TextFont::from_font_size(FontSize::Px(18.)), Node { flex_shrink: 0., ..default() }));
            p.spawn((Text::new("PLY or Potree URL"), TextFont::from_font_size(FontSize::Px(14.)), Node { flex_shrink: 0., ..default() }));
            let owner = p.target_entity();
            p.commands_mut().spawn_scene(bsn! {
                @FeathersTextInputContainer
                Node { height: px(44), min_height: px(44), flex_grow: 0., flex_shrink: 0. }
                Children [ (
                    @FeathersTextInput { @max_characters: {Some(8192)} }
                    UrlField
                    AccessibleLabel("PLY or Potree URL")
                ) ]
            }).insert(ChildOf(owner));
            button(p, "Up axis: Y", Action::Up);
            p.spawn(Node { column_gap: px(12), flex_shrink: 0., ..default() }).with_children(|p| {
                button(p, "Load URL", Action::Load);
                button(p, "Close", Action::Close);
            });
            button(p, "Use built-in demo", Action::Demo);
            p.spawn(Node { column_gap: px(12), flex_shrink: 0., ..default() }).with_children(|p| {
                button(p, "Open PLY file", Action::File);
                button(p, "Open Potree folder", Action::Folder);
            });
            p.spawn((Caption::Notice, Text::default(), TextColor(Color::srgb(1., 0.5, 0.4)), TextFont::from_font_size(FontSize::Px(14.)), Node { flex_shrink: 0., ..default() }));
            p.spawn((Text::new("Drop a PLY file or Potree 2 folder anywhere. PLY and per-node Potree files use whole-file reads; shared hierarchy.bin and single-file octree.bin use byte ranges over HTTP. Remote hosts must allow CORS. Enter loads; Esc closes. LAS/LAZ/COPC import is not available."), TextFont::from_font_size(FontSize::Px(12.)), Node { flex_shrink: 0., ..default() }));
            p.spawn((Caption::Source, Text::default(), TextLayout { linebreak: LineBreak::AnyCharacter, ..default() }, TextFont::from_font_size(FontSize::Px(12.)), Node { flex_shrink: 0., ..default() }));
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn action(
    event: On<Activate>,
    actions: Query<&Action>,
    mut state: ResMut<OpenPanel>,
    mut requests: MessageWriter<Request>,
    mut settings: ResMut<settings::Settings>,
    mut focus: ResMut<InputFocus>,
    mut files: ResMut<files::Files>,
) {
    let Ok(action) = actions.get(event.entity) else {
        return;
    };
    match action {
        Action::Open => {
            state.submit = false;
            state.open = !state.open;
            settings.close();
            focus.clear();
        }
        Action::Close => {
            state.submit = false;
            state.open = false;
            focus.clear();
        }
        Action::Up => {
            state.up = if state.up == PointcloudUp::Y {
                PointcloudUp::Z
            } else {
                PointcloudUp::Y
            }
        }
        Action::Load => state.submit = true,
        Action::File | Action::Folder => {
            state.submit = false;
            files.choose(matches!(action, Action::Folder), state.up);
            focus.clear();
        }
        Action::Demo => {
            requests.write(Request::Demo);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update(
    mut state: ResMut<OpenPanel>,
    mut fields: Query<(Entity, &mut EditableText), With<UrlField>>,
    added: Query<Entity, Added<UrlField>>,
    source: Res<ViewerSource>,
    mut focus: ResMut<InputFocus>,
    keys: Res<ButtonInput<KeyCode>>,
    mut was_open: Local<bool>,
) {
    for entity in &added {
        if let PointcloudSource::PotreeUrl(url) | PointcloudSource::PlyUrl(url) = &source.0.source
            && let Ok((_, mut field)) = fields.get_mut(entity)
        {
            field.queue_edit(TextEdit::Insert(url.clone().into()));
        }
    }
    if state.open
        && let Ok((entity, _)) = fields.single()
    {
        if !*was_open {
            focus.set(entity, FocusCause::Navigated);
        }
        if keys.just_pressed(KeyCode::Escape) {
            state.open = false;
            state.submit = false;
            focus.clear();
        } else if *was_open && focus.get() == Some(entity) && keys.just_pressed(KeyCode::Enter) {
            state.submit = true;
        }
    }
    if *was_open && !state.open {
        focus.clear();
    }
    *was_open = state.open;
}

// Text edits are applied in PostUpdate. Read only after that set, including
// the final keystroke in the same frame as Enter and asynchronous clipboard reads.
pub(super) fn submit(
    mut state: ResMut<OpenPanel>,
    fields: Query<&EditableText, With<UrlField>>,
    mut requests: MessageWriter<Request>,
) {
    if !state.submit || !state.open {
        return;
    }
    if let Ok(field) = fields.single()
        && field.pending_edits.is_empty()
        && field.pending_paste.is_none()
    {
        requests.write(Request::Url(field.value().to_string(), state.up));
        state.submit = false;
    }
}

pub(super) fn sync(
    state: Res<OpenPanel>,
    source: Res<ViewerSource>,
    files: Res<files::Files>,
    status: Query<&PointcloudStatus, With<ViewerCloud>>,
    mut panels: Query<&mut Node, With<PanelRoot>>,
    mut text: Query<(&Caption, &mut Text)>,
) {
    for mut node in &mut panels {
        node.display = if state.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (caption, mut text) in &mut text {
        if matches!(caption, Caption::Notice) {
            text.0 = files
                .notice
                .clone()
                .or_else(|| state.error.clone())
                .unwrap_or_default();
        }
        if matches!(caption, Caption::Up) {
            text.0 = format!(
                "Up axis: {}",
                if state.up == PointcloudUp::Y {
                    "Y"
                } else {
                    "Z"
                }
            );
        }
        if matches!(caption, Caption::Source) {
            let label = match &source.0.source {
                PointcloudSource::Mesh(_) => "Built-in sphere".to_owned(),
                PointcloudSource::PotreeUrl(url) | PointcloudSource::PlyUrl(url) => url.clone(),
                PointcloudSource::PotreePath(path) | PointcloudSource::PlyPath(path) => {
                    path.display().to_string()
                }
                PointcloudSource::PlyFile { name, .. } => name.clone(),
                PointcloudSource::PotreeFiles(files) => files.name().into(),
            };
            let detail = status
                .single()
                .map(|s| s.error.clone().unwrap_or_else(|| format!("{:?}", s.phase)))
                .unwrap_or_else(|_| "Loading".into());
            text.0 = format!("Current source: {label}\n{detail}");
        }
    }
}

pub(super) fn scroll(
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut panels: Query<(&Hovered, &mut ScrollPosition), With<PanelRoot>>,
) {
    for event in wheel.read() {
        for (hovered, mut scroll) in &mut panels {
            if hovered.get() {
                let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                    30.
                } else {
                    1.
                };
                scroll.0.y = (scroll.0.y - event.y * scale).max(0.);
            }
        }
    }
}

fn drag_scroll(
    event: On<Pointer<Drag>>,
    controls: Query<(), With<Action>>,
    fields: Query<(), With<UrlField>>,
    parents: Query<&ChildOf>,
    mut panels: Query<&mut ScrollPosition, With<PanelRoot>>,
) {
    let mut entity = event.entity;
    loop {
        if controls.contains(entity) || fields.contains(entity) {
            return;
        }
        if let Ok(parent) = parents.get(entity) {
            entity = parent.parent();
        } else {
            break;
        }
    }
    for mut scroll in &mut panels {
        scroll.0.y = (scroll.0.y - event.delta.y).max(0.);
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn browser_commands(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut fields: Query<&mut EditableText, With<UrlField>>,
) {
    use bevy::input::{ButtonState, keyboard::Key};
    if event.input.state != ButtonState::Pressed
        || !keys.any_pressed([KeyCode::SuperLeft, KeyCode::SuperRight])
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
