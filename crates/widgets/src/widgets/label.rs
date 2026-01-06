//! Label widget for displaying text.

use crate::widget::WidgetSystem;
use bevy::{
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use bevy_egui::egui;

/// Arguments for the Label widget.
#[derive(Debug, Clone)]
pub struct LabelArgs {
    /// The text to display
    pub text: String,
    /// Optional text color
    pub color: Option<egui::Color32>,
    /// Optional text size override
    pub size: Option<f32>,
    /// Whether the label should be selectable
    pub selectable: bool,
    /// Optional wrap width. If None, text will not wrap.
    pub wrap: Option<f32>,
}

impl Default for LabelArgs {
    fn default() -> Self {
        Self {
            text: "Label".to_string(),
            color: None,
            size: None,
            selectable: false,
            wrap: None,
        }
    }
}

/// Label widget system that displays text.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_egui::egui;
/// use widgets::widgets::{WidgetSystemExt, Label};
///
/// fn my_ui(ui: &mut egui::Ui, world: &mut World) {
///     ui.add_widget_with::<Label>(world, "my_label", LabelArgs {
///         text: "Hello, World!".to_string(),
///         color: Some(egui::Color32::WHITE),
///         ..Default::default()
///     });
/// }
/// ```
#[derive(SystemParam)]
pub struct Label<'w, 's> {
    _marker: std::marker::PhantomData<(&'w (), &'s ())>,
}

impl WidgetSystem for Label<'_, '_> {
    type Args = LabelArgs;
    type Output = ();

    fn ui_system(
        _world: &mut World,
        _state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        args: Self::Args,
    ) -> Self::Output {
        let mut rich_text = egui::RichText::new(&args.text);

        if let Some(color) = args.color {
            rich_text = rich_text.color(color);
        }

        if let Some(size) = args.size {
            rich_text = rich_text.size(size);
        }

        let mut label = egui::Label::new(rich_text).selectable(args.selectable);

        // Wrap text if requested
        if args.wrap.is_some() {
            label = label.wrap();
        }

        ui.add(label);
    }
}
