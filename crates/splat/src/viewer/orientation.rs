//! Source-coordinate choices; the camera remains in viewer Y-up space.
use super::ViewerOptions;
use crate::{SetSplatUp, SourceUp};
use bevy::{
    feathers::{controls::FeathersButton, theme::ThemedText},
    prelude::*,
    ui_widgets::Activate,
};

#[derive(Component)]
pub(super) struct Choice(pub(super) SourceUp);
#[derive(Component)]
pub(super) struct Current;

pub(super) fn controls(panel: &mut ChildSpawnerCommands, assets: &AssetServer) {
    panel.spawn((Current, super::style::text(assets, "", 14.)));
    panel.spawn(super::style::text(
        assets,
        "Choose the file's convention. 3DGS also preserves SuperSplat's forward direction.",
        12.,
    ));
    for choices in [
        &[
            (SourceUp::Gaussian, "3DGS / SuperSplat"),
            (SourceUp::Y, "Y-up (raw)"),
            (SourceUp::Z, "Z-up"),
        ][..],
        &[
            (SourceUp::X, "+X"),
            (SourceUp::NegativeX, "-X"),
            (SourceUp::Y, "+Y"),
            (SourceUp::NegativeY, "-Y"),
            (SourceUp::Z, "+Z"),
            (SourceUp::NegativeZ, "-Z"),
        ][..],
    ] {
        if choices.len() == 6 {
            panel.spawn(super::style::text(
                assets,
                "Custom source up (legacy heading)",
                12.,
            ));
        }
        panel
            .spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                ..default()
            })
            .with_children(|row| {
                for &(up, caption) in choices {
                    let parent = row.target_entity();
                    row.commands_mut()
                        .spawn_scene(bsn! {
                            @FeathersButton
                            AccessibleLabel({format!("Source coordinates: {caption}")})
                            Node { min_height: px(44), padding: UiRect::horizontal(px(8)) }
                            on(choose)
                        })
                        .insert((Choice(up), ChildOf(parent)))
                        .with_children(|button| {
                            button.spawn((Text::new(caption), ThemedText));
                        });
                }
            });
    }
}

fn choose(
    event: On<Activate>,
    choices: Query<&Choice>,
    mut options: ResMut<ViewerOptions>,
    mut changes: MessageWriter<SetSplatUp>,
) {
    if let Ok(choice) = choices.get(event.entity) {
        options.up = choice.0;
        changes.write(SetSplatUp(choice.0));
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn update(
    options: Res<ViewerOptions>,
    mut labels: Query<&mut Text, Or<(With<Current>, With<super::ui::UpLabel>)>>,
) {
    for mut text in &mut labels {
        let value = format!("Coordinates: {}", options.up.label());
        if text.0 != value {
            text.0 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selecting_a_preset_emits_the_exact_frame() {
        let mut app = App::new();
        app.init_resource::<ViewerOptions>()
            .add_message::<SetSplatUp>();
        let button = app
            .world_mut()
            .spawn(Choice(SourceUp::Gaussian))
            .observe(choose)
            .id();
        app.world_mut().trigger(Activate { entity: button });
        assert_eq!(
            app.world().resource::<ViewerOptions>().up,
            SourceUp::Gaussian
        );
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<SetSplatUp>>()
                .drain()
                .next()
                .unwrap()
                .0,
            SourceUp::Gaussian
        );
    }
}
