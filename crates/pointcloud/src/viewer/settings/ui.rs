//! Native and browser presentation for point-cloud display controls.
use super::*;
use bevy::{
    feathers::{
        controls::{FeathersButton, FeathersNumberInput, NumberInputValue, UpdateNumberInput},
        theme::ThemedText,
    },
    input::{
        mouse::{MouseScrollUnit, MouseWheel},
        touch::Touches,
    },
    input_focus::InputFocus,
    picking::{
        hover::{HoverMap, Hovered},
        pointer::PointerId,
    },
    text::EditableText,
};
fn button(parent: &mut ChildSpawnerCommands, label: &str, field: Field, direction: i32) {
    let owner = parent.target_entity();
    parent
        .commands_mut()
        .spawn_scene(bsn! {
            @FeathersButton
            AccessibleLabel({label.to_owned()})
            Node { flex_shrink: 0., min_height: px(44), min_width: px(44), padding: UiRect::horizontal(px(8)) }
        })
        .insert((Control(field, direction), ChildOf(owner)))
        .with_children(|p| {
            p.spawn((Caption(field), Text::new(label), ThemedText));
        });
}

pub(super) fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(CircleMesh(
        meshes.add(Circle::new(0.5).mesh().resolution(32)),
    ));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(12),
                ..default()
            },
            GlobalZIndex(30),
            Hovered::default(),
        ))
        .with_children(|p| button(p, "Settings", Field::Open, 0));
    commands.spawn((Panel, Hovered::default(), ScrollPosition::default(), GlobalZIndex(20),
        BackgroundColor(Color::srgb(0.055, 0.065, 0.08)), Node {
            display: Display::None, position_type: PositionType::Absolute,
            top: px(12), right: px(12), bottom: px(76), width: px(300), max_width: percent(94),
            flex_direction: FlexDirection::Column, padding: UiRect::all(px(14)), row_gap: px(8),
            overflow: Overflow::scroll_y(), ..default()
        }))
        .observe(drag_scroll)
        .with_children(|p| {
            p.spawn((Node { flex_shrink: 0., ..default() }, Text::new("Point-cloud settings"), TextFont::from_font_size(FontSize::Px(18.))));
            for field in [Field::Units, Field::Shape, Field::Orientation, Field::Adaptive] {
                button(p, "Change", field, 0);
            }
            for (field, label) in [
                (Field::Size, "Point size (manual)"),
                (Field::MinSize, "Minimum pixels (0 = off)"),
                (Field::MaxSize, "Maximum pixels (0 = off)"),
                (Field::Budget, "Point budget (Potree)"),
                (Field::NodeRadius, "Minimum node radius (pixels)"),
            ] {
                p.spawn((Node { flex_shrink: 0., ..default() }, Text::new(label), TextFont::from_font_size(FontSize::Px(14.))));
                p.spawn(Node { column_gap: px(6), flex_shrink: 0., ..default() }).with_children(|row| {
                    button(row, "-", field, -1);
                    let owner = row.target_entity();
                    row.commands_mut().spawn_scene(bsn! {
                        @FeathersNumberInput
                        AccessibleLabel({label.to_owned()})
                        Node { width: px(140), min_height: px(44) }
                    }).insert((Control(field, 0), ChildOf(owner)));
                    button(row, "+", field, 1);
                });
            }
            p.spawn((Caption(Field::SizingInfo), Node { flex_shrink: 0., ..default() }, Text::default(), TextFont::from_font_size(FontSize::Px(12.))));
            p.spawn((Node { flex_shrink: 0., ..default() }, Text::new("Higher node radius = coarser Potree detail.\nNormal alignment uses source normals or a fallback.\nScroll or drag the panel to see all controls."), TextFont::from_font_size(FontSize::Px(12.))));
            button(p, "Reset settings", Field::Reset, 0);
            button(p, "Close settings", Field::Open, 0);
        });
}

pub(super) fn sync_ui(
    mut commands: Commands,
    state: Res<Settings>,
    mut panels: Query<&mut Node, With<Panel>>,
    mut captions: Query<(&Caption, &mut Text)>,
    labels: Query<(Entity, &Control, &AccessibleLabel)>,
) {
    for mut panel in &mut panels {
        panel.display = if state.open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (caption, mut text) in &mut captions {
        let value = state.caption(caption.0);
        if !value.is_empty() && text.0 != value {
            text.0 = value;
        }
    }
    for (entity, control, label) in &labels {
        if control.0 == Field::Adaptive {
            if state.spacing_available {
                commands
                    .entity(entity)
                    .remove::<bevy::ui::InteractionDisabled>();
            } else {
                commands
                    .entity(entity)
                    .insert(bevy::ui::InteractionDisabled);
            }
        }
        let value = state.caption(control.0);
        if !value.is_empty() && label.0 != value {
            commands.entity(entity).insert(AccessibleLabel(value));
        }
    }
}

pub(super) fn sync_numbers(
    mut commands: Commands,
    state: Res<Settings>,
    focus: Res<InputFocus>,
    controls: Query<(Entity, &Control, &Children)>,
    editors: Query<&EditableText>,
) {
    for (entity, control, children) in &controls {
        if control.1 != 0 {
            continue;
        }
        for child in children {
            if let Ok(editor) = editors.get(*child)
                && focus.get() != Some(*child)
                && editor.value().to_string() != state.number(control.0).to_string()
            {
                commands.trigger(UpdateNumberInput {
                    entity,
                    value: NumberInputValue::F32(state.number(control.0)),
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_input(
    mut config: ResMut<NavigationConfig>,
    mut focus: ResMut<InputFocus>,
    editors: Query<(), With<EditableText>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    hover_map: Res<HoverMap>,
    hovered: Query<&Hovered>,
    nodes: Query<(), With<Node>>,
) {
    let mouse_canvas =
        mouse.any_just_pressed([MouseButton::Left, MouseButton::Middle, MouseButton::Right])
            && !hovered.iter().any(|h| h.get());
    let touch_canvas = touches.iter_just_pressed().any(|touch| {
        !hover_map
            .get(&PointerId::Touch(touch.id()))
            .is_some_and(|hits| hits.keys().any(|e| nodes.contains(*e)))
    });
    if mouse_canvas || touch_canvas {
        focus.clear();
    }
    config.blocked = focus.get().is_some_and(|entity| editors.contains(entity));
}

pub(super) fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    mut panels: Query<(&Hovered, &mut ScrollPosition), With<Panel>>,
) {
    for event in wheel.read() {
        for (hover, mut position) in &mut panels {
            if hover.get() {
                position.0.y = (position.0.y
                    - event.y
                        * if event.unit == MouseScrollUnit::Line {
                            30.
                        } else {
                            1.
                        })
                .max(0.);
            }
        }
    }
}

fn drag_scroll(
    event: On<Pointer<Drag>>,
    controls: Query<(), With<Control>>,
    parents: Query<&ChildOf>,
    mut panels: Query<&mut ScrollPosition, With<Panel>>,
) {
    let mut target = event.entity;
    loop {
        if controls.contains(target) {
            return;
        }
        if let Ok(parent) = parents.get(target) {
            target = parent.parent();
        } else {
            break;
        }
    }
    for mut position in &mut panels {
        position.0.y = (position.0.y - event.delta.y).max(0.);
    }
}
