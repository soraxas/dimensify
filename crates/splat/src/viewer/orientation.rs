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
#[derive(Component)]
pub(super) struct Details;

pub(super) fn controls(panel: &mut ChildSpawnerCommands, assets: &AssetServer) {
    panel.spawn((Current, super::style::text(assets, "", 14.)));
    panel.spawn(super::style::text(
        assets,
        "Choose how this file's axes map into the viewer. This changes the scene, not the camera.",
        12.,
    ));
    for choices in [
        &[
            (SourceUp::Gaussian, "SuperSplat import (-Y up)"),
            (SourceUp::Y, "Raw (+Y up)"),
            (SourceUp::Z, "+Z up"),
        ][..],
        &[
            (SourceUp::X, "+X"),
            (SourceUp::NegativeX, "-X"),
            (SourceUp::NegativeY, "-Y"),
            (SourceUp::NegativeZ, "-Z"),
        ][..],
    ] {
        if choices.len() == 4 {
            panel.spawn(super::style::text(
                assets,
                "Other source up directions",
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
    panel.spawn((Details, super::style::text(assets, "", 12.)));
}

fn describe(up: SourceUp) -> String {
    let axis = |direction: Vec3| {
        if direction.x > 0.5 {
            "+X"
        } else if direction.x < -0.5 {
            "-X"
        } else if direction.y > 0.5 {
            "+Y"
        } else if direction.y < -0.5 {
            "-Y"
        } else if direction.z > 0.5 {
            "+Z"
        } else {
            "-Z"
        }
    };
    let source_up = if up == SourceUp::Gaussian {
        "-Y"
    } else {
        up.label()
    };
    let rotation = up.transform().rotation;
    let convention = if up == SourceUp::Gaussian {
        "SuperSplat import: 180 deg around Z\n"
    } else {
        ""
    };
    format!(
        "{convention}Source up: {source_up} -> viewer +Y\nSource axes -> viewer axes:\n+X -> {} | +Y -> {} | +Z -> {}\nHandedness: preserved (rotation only; no mirror).\nViewer: right-handed, +Y up; camera forward -Z.\nThe source convention is chosen, not detected from the file.",
        axis(rotation * Vec3::X),
        axis(rotation * Vec3::Y),
        axis(rotation * Vec3::Z)
    )
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
    mut labels: Query<(&mut Text, Has<Details>), Or<(With<Current>, With<Details>)>>,
) {
    for (mut text, details) in &mut labels {
        let value = if details {
            describe(options.up)
        } else {
            format!(
                "Selected import: {}",
                if options.up == SourceUp::Gaussian {
                    "SuperSplat import (-Y up)"
                } else {
                    options.up.label()
                }
            )
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explanation_reports_the_actual_rotation_without_guessing_file_handedness() {
        assert_eq!(SourceUp::Gaussian.label(), "SuperSplat import (-Y up)");
        let details = describe(SourceUp::Gaussian);
        assert!(details.contains("Source up: -Y -> viewer +Y"));
        assert!(details.contains("+X -> -X"));
        assert!(details.contains("+Z -> +Z"));
        assert!(details.contains("Handedness: preserved"));
        assert!(details.contains("not detected"));
        let legacy = describe(SourceUp::NegativeY);
        assert!(legacy.contains("+X -> +X"));
        assert!(legacy.contains("+Z -> -Z"));
        assert_ne!(details, legacy);
    }

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
