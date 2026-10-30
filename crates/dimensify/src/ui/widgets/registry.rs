#[cfg(feature = "protocol")]
use crate::stream::DataSource;
use bevy::prelude::{Res, ResMut, Resource};
use bevy_egui::{EguiContexts, egui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{ECheckboxButton, ELabel};

#[derive(Resource, Default)]
pub struct WidgetRegistry {
    widgets: HashMap<String, Box<dyn DynWidget>>,
}

impl WidgetRegistry {
    /// Register a new dynamic widget under a string identifier.
    ///
    /// The widget is just a closure taking an [`egui::Ui`] and returning an
    /// [`egui::Response`]. This is the primary API for both Dimensify's own
    /// built‑in widgets and user‑defined widgets.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use bevy::prelude::*;
    /// # use dimensify::ui::widgets::{WidgetRegistry, WidgetPanel};
    /// # use bevy_egui::egui;
    /// fn setup(mut registry: ResMut<WidgetRegistry>, mut panel: ResMut<WidgetPanel>) {
    ///     registry.register("hello", |ui: &mut egui::Ui| {
    ///         ui.label("Hello from a custom widget")
    ///     });
    ///     panel.widgets.push("hello".to_string());
    /// }
    /// ```
    pub fn register<F>(&mut self, id: impl Into<String>, widget: F)
    where
        F: FnMut(&mut egui::Ui) -> egui::Response + Send + Sync + 'static,
    {
        self.widgets.insert(id.into(), Box::new(widget));
    }

    pub fn unregister(&mut self, id: &str) -> Option<Box<dyn DynWidget>> {
        self.widgets.remove(id)
    }

    pub fn show(&mut self, id: &str, ui: &mut egui::Ui) -> Option<egui::Response> {
        let Some(widget) = self.widgets.get_mut(id) else {
            bevy::log::warn!("WidgetRegistry missing widget '{}'", id);
            return None;
        };
        Some(widget.show(ui))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WidgetCommand {
    Label {
        id: String,
        text: String,
    },
    Button {
        id: String,
        text: String,
    },
    Checkbox {
        id: String,
        text: String,
        checked: bool,
    },
}

#[cfg(feature = "protocol")]
#[derive(Resource, Clone, Debug)]
pub struct WidgetStreamSettings {
    pub source: DataSource,
}

#[cfg(feature = "protocol")]
impl Default for WidgetStreamSettings {
    fn default() -> Self {
        let source = match std::env::var("DIMENSIFY_WIDGET_SOURCE")
            .unwrap_or_else(|_| "local".to_string())
            .as_str()
        {
            "file" => std::env::var("DIMENSIFY_WIDGET_FILE")
                .ok()
                .map(|path| DataSource::FileReplay { path })
                .unwrap_or(DataSource::Local),
            "db" => std::env::var("DIMENSIFY_WIDGET_DB_ADDR")
                .ok()
                .map(|addr| DataSource::Db { addr })
                .unwrap_or(DataSource::Local),
            _ => DataSource::Local,
        };
        Self { source }
    }
}

#[derive(Resource, Default)]
pub struct WidgetCommandQueue {
    commands: Vec<WidgetCommand>,
}

impl WidgetCommandQueue {
    /// Queue a new [`WidgetCommand`] to be applied to the [`WidgetRegistry`].
    ///
    /// This is an integration layer for dynamic / remote widget definitions
    /// (e.g. from a file or network source). Built‑in Dimensify widgets are
    /// registered via [`register_default_widgets`] and are not special‑cased.
    pub fn push(&mut self, command: WidgetCommand) {
        self.commands.push(command);
    }

    pub fn drain(&mut self) -> Vec<WidgetCommand> {
        self.commands.drain(..).collect()
    }
}

#[derive(Resource, Default)]
pub struct WidgetPanel {
    pub widgets: Vec<String>,
}

/// Register Dimensify's built‑in widgets into the global [`WidgetRegistry`].
///
/// This uses exactly the same API that downstream users can call themselves:
/// they can freely add more entries to [`WidgetRegistry`] or modify
/// [`WidgetPanel::widgets`] to change ordering / visibility.
pub fn register_default_widgets(
    mut registry: ResMut<WidgetRegistry>,
    mut panel: ResMut<WidgetPanel>,
) {
    fn add_to_panel(panel: &mut WidgetPanel, id: &str) {
        if !panel.widgets.iter().any(|existing| existing == id) {
            panel.widgets.push(id.to_string());
        }
    }

    // Simple label describing the registry itself.
    let label_text = "Dimensify widget registry".to_string();
    registry.register("demo_label", move |ui: &mut egui::Ui| {
        ui.add(ELabel::new(label_text.clone()))
    });
    add_to_panel(&mut panel, "demo_label");

    // Button that logs when clicked.
    let button_text = "Click me".to_string();
    registry.register("demo_button", move |ui: &mut egui::Ui| {
        let response = ui.button(button_text.clone());
        if response.clicked() {
            bevy::log::info!("WidgetRegistry button clicked: {}", button_text);
        }
        response
    });
    add_to_panel(&mut panel, "demo_button");

    // Checkbox demonstrating simple stateful interaction.
    let checkbox_text = "Toggle option".to_string();
    let mut value = true;
    registry.register("demo_checkbox", move |ui: &mut egui::Ui| {
        let response = ui.add(ECheckboxButton::new(checkbox_text.clone(), &mut value));
        if response.changed() {
            bevy::log::info!(
                "WidgetRegistry checkbox changed: {} -> {}",
                checkbox_text,
                value
            );
        }
        response
    });
    add_to_panel(&mut panel, "demo_checkbox");
}

/// Minimal demo of pushing some [`WidgetCommand`]s into the queue.
///
/// This is kept as an example of using the command‑based API for external
/// widget sources. For the widgets that ship with Dimensify itself, see
/// [`register_default_widgets`], which registers them directly into
/// [`WidgetRegistry`] so that user‑defined widgets behave exactly the same.
pub fn register_demo_widgets(mut queue: ResMut<WidgetCommandQueue>) {
    queue.push(WidgetCommand::Label {
        id: "demo_label".to_string(),
        text: "Dimensify widget registry".to_string(),
    });
    queue.push(WidgetCommand::Button {
        id: "demo_button".to_string(),
        text: "Click me".to_string(),
    });
    queue.push(WidgetCommand::Checkbox {
        id: "demo_checkbox".to_string(),
        text: "Toggle option".to_string(),
        checked: true,
    });
}

#[cfg(feature = "protocol")]
#[cfg(not(target_arch = "wasm32"))]
pub fn load_widget_commands_from_source(
    settings: Res<WidgetStreamSettings>,
    mut queue: ResMut<WidgetCommandQueue>,
) {
    match &settings.source {
        DataSource::FileReplay { path } => {
            let content = match std::fs::read_to_string(path) {
                Ok(content) => content,
                Err(err) => {
                    bevy::log::error!("Failed to read widget file {}: {}", path, err);
                    return;
                }
            };

            for (line_no, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<WidgetCommand>(line) {
                    Ok(command) => queue.push(command),
                    Err(err) => {
                        bevy::log::warn!(
                            "Failed to parse widget command at line {}: {}",
                            line_no + 1,
                            err
                        );
                    }
                }
            }
        }
        DataSource::Db { .. } => {
            bevy::log::warn!("Widget DB source is not implemented yet");
        }
        DataSource::Local => {}
    }
}

pub fn apply_widget_commands(
    mut registry: ResMut<WidgetRegistry>,
    mut queue: ResMut<WidgetCommandQueue>,
    mut panel: ResMut<WidgetPanel>,
) {
    for command in queue.drain() {
        match command {
            WidgetCommand::Label { id, text } => {
                let label_text = text.clone();
                registry.register(id.clone(), move |ui: &mut egui::Ui| {
                    ui.add(ELabel::new(label_text.clone()))
                });
                if !panel.widgets.contains(&id) {
                    panel.widgets.push(id);
                }
            }
            WidgetCommand::Button { id, text } => {
                let button_text = text.clone();
                registry.register(id.clone(), move |ui: &mut egui::Ui| {
                    let response = ui.button(button_text.clone());
                    if response.clicked() {
                        bevy::log::info!("WidgetRegistry button clicked: {}", button_text);
                    }
                    response
                });
                if !panel.widgets.contains(&id) {
                    panel.widgets.push(id);
                }
            }
            WidgetCommand::Checkbox { id, text, checked } => {
                let checkbox_text = text.clone();
                let mut value = checked;
                registry.register(id.clone(), move |ui: &mut egui::Ui| {
                    let response = ui.add(ECheckboxButton::new(checkbox_text.clone(), &mut value));
                    if response.changed() {
                        bevy::log::info!(
                            "WidgetRegistry checkbox changed: {} -> {}",
                            checkbox_text,
                            value
                        );
                    }
                    response
                });
                if !panel.widgets.contains(&id) {
                    panel.widgets.push(id);
                }
            }
        }
    }
}

pub fn widget_registry_demo_ui(
    mut contexts: EguiContexts,
    mut registry: ResMut<WidgetRegistry>,
    panel: Res<WidgetPanel>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Widget Registry").show(ctx, |ui| {
        for id in &panel.widgets {
            if registry.show(id, ui).is_some() {
                ui.add_space(4.0);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        WidgetCommand, WidgetCommandQueue, WidgetPanel, WidgetRegistry, apply_widget_commands,
    };
    use bevy::prelude::{Schedule, World};

    #[test]
    fn apply_widget_commands_registers_widgets() {
        let mut world = World::new();
        world.insert_resource(WidgetRegistry::default());
        world.insert_resource(WidgetCommandQueue::default());
        world.insert_resource(WidgetPanel::default());

        {
            let mut queue = world.resource_mut::<WidgetCommandQueue>();
            queue.push(WidgetCommand::Label {
                id: "label".to_string(),
                text: "Label".to_string(),
            });
            queue.push(WidgetCommand::Button {
                id: "button".to_string(),
                text: "Button".to_string(),
            });
            queue.push(WidgetCommand::Checkbox {
                id: "checkbox".to_string(),
                text: "Checkbox".to_string(),
                checked: false,
            });
        }

        let mut schedule = Schedule::default();
        schedule.add_systems(apply_widget_commands);
        schedule.run(&mut world);

        let panel = world.resource::<WidgetPanel>();
        assert_eq!(panel.widgets.len(), 3);

        let mut registry = world.resource_mut::<WidgetRegistry>();
        assert!(registry.unregister("label").is_some());
        assert!(registry.unregister("button").is_some());
        assert!(registry.unregister("checkbox").is_some());
    }
}
