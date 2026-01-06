//! Example demonstrating the modular Bevy egui widget system framework.
//!
//! This example shows how to:
//! - Use predefined widgets (Button, Label, Input, Inspector)
//! - Create custom widgets
//! - Use both root widgets and regular widgets
//! - Manage widget state

use bevy::prelude::*;
use bevy_egui::{EguiPlugin, egui};
use dimensify_widgets::widget::{
    RootWidgetSystem, RootWidgetSystemExt, WidgetSystem, WidgetSystemExt,
};
use dimensify_widgets::widgets::{
    Button, ButtonArgs, Input, InputArgs, Inspector, InspectorArgs, Label, LabelArgs,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_systems(Update, render_ui)
        .run();
}

/// Example root widget that creates a window and uses other widgets
#[derive(SystemParam)]
pub struct ExampleRootWidget<'w, 's> {
    counter: Local<'s, u32>,
}

impl RootWidgetSystem for ExampleRootWidget<'_, '_> {
    type Args = ();
    type Output = ();

    fn ctx_system(
        world: &mut World,
        state: &mut bevy::ecs::system::SystemState<Self>,
        ctx: &mut egui::Context,
        _args: Self::Args,
    ) -> Self::Output {
        let mut state_mut = state.get_mut(world);
        let counter = state_mut.counter;

        egui::Window::new("Widget Framework Example")
            .default_size([400.0, 500.0])
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Widget Framework Demo");
                    ui.separator();
                });

                ui.add_space(10.0);

                // Example: Button widget
                ui.horizontal(|ui| {
                    ui.label("Button Example:");
                    let clicked = ui.add_widget_with::<Button>(
                        world,
                        "demo_button",
                        ButtonArgs {
                            text: format!("Clicked {} times", counter),
                            ..Default::default()
                        },
                    );

                    if clicked {
                        *state_mut.counter += 1;
                    }
                });

                ui.add_space(10.0);

                // Example: Label widget
                ui.horizontal(|ui| {
                    ui.label("Label Example:");
                    ui.add_widget_with::<Label>(
                        world,
                        "demo_label",
                        LabelArgs {
                            text: "This is a styled label!".to_string(),
                            color: Some(egui::Color32::from_rgb(100, 200, 255)),
                            size: Some(16.0),
                            ..Default::default()
                        },
                    );
                });

                ui.add_space(10.0);

                // Example: Input widget
                ui.horizontal(|ui| {
                    ui.label("Input Example:");
                    let new_text = ui.add_widget_with::<Input>(
                        world,
                        "demo_input",
                        InputArgs {
                            text: "Type here...".to_string(),
                            hint_text: Some("Enter some text".to_string()),
                            desired_width: Some(200.0),
                            ..Default::default()
                        },
                    );

                    if new_text != "Type here..." {
                        // Text changed - you could update state here
                    }
                });

                ui.add_space(10.0);

                // Example: Inspector widget
                ui.label("Inspector Example:");
                ui.add_widget_with::<Inspector>(
                    world,
                    "demo_inspector_1",
                    InspectorArgs {
                        label: "Counter Value".to_string(),
                        value: counter.to_string(),
                        editable: false,
                        ..Default::default()
                    },
                );

                ui.add_widget_with::<Inspector>(
                    world,
                    "demo_inspector_2",
                    InspectorArgs {
                        label: "Editable Value".to_string(),
                        value: "42.5".to_string(),
                        editable: true,
                        tooltip: Some("You can edit this value".to_string()),
                        ..Default::default()
                    },
                );

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(10.0);

                // Example: Custom widget
                ui.label("Custom Widget Example:");
                ui.add_widget::<CustomCounterWidget>(world, "custom_counter");
            });
    }
}

/// Example custom widget that maintains its own state
#[derive(SystemParam)]
pub struct CustomCounterWidget<'w, 's> {
    count: Local<'s, i32>,
}

impl WidgetSystem for CustomCounterWidget<'_, '_> {
    type Args = ();
    type Output = ();

    fn ui_system(
        _world: &mut World,
        state: &mut bevy::ecs::system::SystemState<Self>,
        ui: &mut egui::Ui,
        _args: Self::Args,
    ) -> Self::Output {
        let mut state_mut = state.get_mut(_world);
        let count = *state_mut.count;

        ui.horizontal(|ui| {
            ui.label(format!("Custom Counter: {}", count));

            if ui.button("-").clicked() {
                *state_mut.count -= 1;
            }

            if ui.button("+").clicked() {
                *state_mut.count += 1;
            }

            if ui.button("Reset").clicked() {
                *state_mut.count = 0;
            }
        });
    }
}

/// System that renders the UI using root widgets
fn render_ui(world: &mut World) {
    world.add_root_widget::<ExampleRootWidget>("example_root");
}
