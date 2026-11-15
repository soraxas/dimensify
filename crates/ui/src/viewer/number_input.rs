//! Preserve live numeric edits for viewer controls across Feathers versions.
use bevy::{
    feathers::controls::{FeathersNumberInput, NumberInputValue},
    input_focus::InputFocus,
    prelude::*,
    text::{EditableText, TextEditChange},
    ui::InteractionDisabled,
    ui_widgets::ValueChange,
};

/// Emit live `ValueChange<f32>` events for a dimensionless Feathers number input.
/// Bevy 0.20 emits committed numeric values only; viewers also apply valid edits
/// while typing. Partial, non-finite and disabled edits leave the current value.
#[derive(Component, Clone, Copy, Default)]
#[require(FeathersNumberInput)]
pub struct LiveNumberInput;

pub(super) fn edit_live_number(
    change: On<TextEditChange>,
    focus: Res<InputFocus>,
    editors: Query<(&EditableText, &ChildOf), Without<InteractionDisabled>>,
    controls: Query<(&NumberInputValue, Has<InteractionDisabled>), With<LiveNumberInput>>,
    mut commands: Commands,
) {
    let entity = change.event_target();
    if focus.get() != Some(entity) {
        return;
    }
    let Ok((editor, parent)) = editors.get(entity) else {
        return;
    };
    if !matches!(
        controls.get(parent.parent()),
        Ok((NumberInputValue::F32(_), false))
    ) {
        return;
    }
    if let Ok(value) = editor.value().to_string().trim().parse::<f32>()
        && value.is_finite()
    {
        commands.trigger(ValueChange {
            source: parent.parent(),
            value,
            is_final: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetApp;

    #[derive(Resource, Default)]
    struct Edits(Vec<(Entity, f32, bool)>);

    #[test]
    fn typing_emits_live_values_and_ignores_partial_disabled_or_unfocused_edits() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::text::TextPlugin,
        ))
        .init_asset::<Image>()
        .init_resource::<InputFocus>()
        .init_resource::<Edits>()
        .add_observer(edit_live_number)
        .add_observer(|event: On<ValueChange<f32>>, mut edits: ResMut<Edits>| {
            edits.0.push((event.source, event.value, event.is_final));
        });
        let root = app
            .world_mut()
            .spawn((FeathersNumberInput, LiveNumberInput))
            .id();
        let editor = app
            .world_mut()
            .spawn((EditableText::default(), ChildOf(root)))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        for (text, expected_count) in [("0.1", 1), ("-", 1), ("NaN", 1), ("inf", 1), ("0.25", 2)] {
            app.world_mut()
                .get_mut::<EditableText>(editor)
                .unwrap()
                .editor
                .set_text(text);
            app.update();
            assert_eq!(
                app.world().resource::<Edits>().0.len(),
                expected_count,
                "{text}"
            );
        }
        assert_eq!(
            app.world().resource::<Edits>().0,
            [(root, 0.1, false), (root, 0.25, false)]
        );
        app.world_mut().entity_mut(root).insert(InteractionDisabled);
        app.world_mut()
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("0.5");
        app.update();
        app.world_mut()
            .entity_mut(root)
            .remove::<InteractionDisabled>();
        app.world_mut().resource_mut::<InputFocus>().clear();
        app.world_mut()
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text("0.75");
        app.update();
        assert_eq!(app.world().resource::<Edits>().0.len(), 2);
    }
}
