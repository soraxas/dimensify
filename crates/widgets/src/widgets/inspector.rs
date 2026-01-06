//! Inspector widget for viewing and editing values.

use crate::widget::WidgetSystem;
use bevy::{
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use bevy_egui::egui;
use std::fmt::Debug;

/// Arguments for the Inspector widget.
#[derive(Debug, Clone)]
pub struct InspectorArgs {
    /// The label/name of the value being inspected
    pub label: String,
    /// The value to display/edit (as a string representation)
    pub value: String,
    /// Whether the value can be edited
    pub editable: bool,
    /// Optional tooltip text
    pub tooltip: Option<String>,
}

impl Default for InspectorArgs {
    fn default() -> Self {
        Self {
            label: "Value".to_string(),
            value: String::new(),
            editable: false,
            tooltip: None,
        }
    }
}

/// Inspector widget system for viewing and editing values.
///
/// This widget displays a label-value pair, optionally allowing editing.
/// Check the returned value to detect changes.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_egui::egui;
/// use widgets::widgets::{WidgetSystemExt, Inspector};
///
/// fn my_ui(ui: &mut egui::Ui, world: &mut World) {
///     let mut value = "10.5".to_string();
///     let new_value = ui.add_widget_with::<Inspector>(world, "my_inspector", InspectorArgs {
///         label: "Position X".to_string(),
///         value: value.clone(),
///         editable: true,
///         ..Default::default()
///     });
///
///     if new_value != value {
///         println!("Value changed to: {}", new_value);
///         value = new_value;
///     }
/// }
/// ```
#[derive(SystemParam)]
pub struct Inspector<'w, 's> {
    _marker: std::marker::PhantomData<(&'w (), &'s ())>,
}

impl WidgetSystem for Inspector<'_, '_> {
    type Args = InspectorArgs;
    type Output = String; // Returns the current value

    fn ui_system(
        _world: &mut World,
        _state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        mut args: Self::Args,
    ) -> Self::Output {
        ui.horizontal(|ui| {
            // Label
            ui.label(&args.label);
            // Note: Tooltip support may vary by egui version
            // if let Some(tooltip) = &args.tooltip {
            //     // Tooltip implementation depends on egui version
            // }

            ui.separator();

            // Value display/edit
            if args.editable {
                ui.add(egui::TextEdit::singleline(&mut args.value));
            } else {
                ui.add(egui::Label::new(&args.value).selectable(true));
            }
        });

        args.value
    }
}

/// Helper trait for automatically converting values to inspector-friendly format.
pub trait Inspectable: Debug {
    fn to_inspector_string(&self) -> String {
        format!("{:?}", self)
    }
}

impl<T: Debug> Inspectable for T {}

/// Generic inspector widget that can display any Debug type.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_egui::egui;
/// use widgets::widgets::{WidgetSystemExt, Inspector, Inspectable};
///
/// fn my_ui(ui: &mut egui::Ui, world: &mut World) {
///     let value = 42;
///     ui.add_widget_with::<Inspector>(world, "my_inspector", InspectorArgs {
///         label: "Count".to_string(),
///         value: value.to_inspector_string(),
///         editable: false,
///         ..Default::default()
///     });
/// }
/// ```
pub use Inspector as ValueInspector;
