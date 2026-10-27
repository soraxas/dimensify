//! Dormant pre-migration custom-widget example (not an active Cargo target).
//!
//! Requires the retired egui/panel registry APIs. For supported examples,
//! see docs/examples.md at the workspace root.
//!
//! A **widget** is a boxed trait object that implements drawing and can use
//! system parameters (via `&mut World`) for rendering. The **widget system**
//! is the panel registry: you register widgets with a **placement** (left,
//! right, bottom, or floating). This example uses the same API as the upstream
//! UI crate ([`dimensify_ui::tabs`]): implement [`WidgetPane`], then register
//! with [`PanelRegistry`] and a location.

use bevy::prelude::*;
use dimensify::{DimensifyPlugins, SimDevPlugin, SimShowcasePlugin, test_scene, util};
use dimensify_ui::widget::{PanelRegistry, WidgetPane};
use eyre::Result;

fn main() -> Result<()> {
    util::initialise()?;

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DimensifyPlugins::default())
        .add_plugins(SimDevPlugin)
        .add_plugins(SimShowcasePlugin)
        .add_plugins(test_scene::plugin)
        .add_systems(Startup, register_custom_widgets)
        .run();

    Ok(())
}

// ---- Custom widgets: each is a type implementing WidgetPane (trait object) ----

#[derive(Default)]
struct CustomHeadingWidget;

impl WidgetPane for CustomHeadingWidget {
    fn title(&self) -> &'static str {
        "Custom heading"
    }

    fn ui(&mut self, ui: &mut bevy_egui::egui::Ui, _world: &mut World) {
        ui.heading("Custom widgets example");
        ui.label(
            "This panel is a trait object (Box<dyn WidgetPane>) registered with the panel registry.",
        );
    }
}

#[derive(Resource, Default)]
struct CounterState {
    value: i32,
}

#[derive(Default)]
struct CounterWidget;

impl WidgetPane for CounterWidget {
    fn title(&self) -> &'static str {
        "Counter"
    }

    fn ui(&mut self, ui: &mut bevy_egui::egui::Ui, world: &mut World) {
        world.resource_scope(|_world, mut state: Mut<CounterState>| {
            ui.horizontal(|ui| {
                ui.label("Count:");
                if ui.button("+").clicked() {
                    state.value = state.value.saturating_add(1);
                }
                if ui.button("-").clicked() {
                    state.value = state.value.saturating_sub(1);
                }
                ui.label(state.value.to_string());
            });
        });
    }
}

#[derive(Resource, Default)]
struct GreeterState {
    name: String,
}

#[derive(Default)]
struct GreeterWidget;

impl WidgetPane for GreeterWidget {
    fn title(&self) -> &'static str {
        "Greeter"
    }

    fn ui(&mut self, ui: &mut bevy_egui::egui::Ui, world: &mut World) {
        world.resource_scope(|_world, mut state: Mut<GreeterState>| {
            ui.horizontal(|ui| {
                ui.label("Say hello to:");
                ui.text_edit_singleline(&mut state.name);
                if ui.button("Log greeting").clicked() {
                    bevy::log::info!("Hello, {}!", state.name);
                }
            });
        });
    }
}

/// Register our custom widgets with the panel registry. Each widget is a type
/// that implements [`WidgetPane`]; the registry stores them as boxed trait
/// objects and places them by location (left/right/bottom/floating).
fn register_custom_widgets(mut commands: Commands, mut registry: ResMut<PanelRegistry>) {
    // Ensure our widget state resources exist so they persist across layout rebuilds.
    commands.init_resource::<CounterState>();
    commands.init_resource::<GreeterState>();

    registry.register_left::<CustomHeadingWidget>("Custom heading", true);
    registry.register_right::<CounterWidget>("Counter", true);
    registry.register_bottom::<GreeterWidget>("Greeter", true);
}
