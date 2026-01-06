//! Input widget for text input.

use crate::widget::WidgetSystem;
use bevy::{
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use bevy_egui::egui;

/// Arguments for the Input widget.
#[derive(Debug, Clone)]
pub struct InputArgs {
    /// The current text value (will be mutated)
    pub text: String,
    /// Optional placeholder text
    pub hint_text: Option<String>,
    /// Optional desired width. If None, will fill available width.
    pub desired_width: Option<f32>,
    /// Whether this is a password field (masks input)
    pub password: bool,
    /// Whether the input is read-only
    pub readonly: bool,
}

impl Default for InputArgs {
    fn default() -> Self {
        Self {
            text: String::new(),
            hint_text: None,
            desired_width: None,
            password: false,
            readonly: false,
        }
    }
}

/// Input widget system for text input.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_egui::egui;
/// use widgets::widgets::{WidgetSystemExt, Input};
///
/// fn my_ui(ui: &mut egui::Ui, world: &mut World) {
///     let mut text = "Hello".to_string();
///     ui.add_widget_with::<Input>(world, "my_input", InputArgs {
///         text: text.clone(),
///         hint_text: Some("Enter text...".to_string()),
///         ..Default::default()
///     });
/// }
/// ```
#[derive(SystemParam)]
pub struct Input<'w, 's> {
    _marker: std::marker::PhantomData<(&'w (), &'s ())>,
}

impl WidgetSystem for Input<'_, '_> {
    type Args = InputArgs;
    type Output = String; // Returns the current text value

    fn ui_system(
        _world: &mut World,
        _state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        mut args: Self::Args,
    ) -> Self::Output {
        let mut text_edit = if args.password {
            egui::TextEdit::singleline(&mut args.text).password(true)
        } else {
            egui::TextEdit::singleline(&mut args.text)
        };

        if let Some(hint) = &args.hint_text {
            text_edit = text_edit.hint_text(hint);
        }

        if let Some(width) = args.desired_width {
            text_edit = text_edit.desired_width(width);
        }

        if args.readonly {
            text_edit = text_edit.interactive(false);
        }

        ui.add(text_edit);
        args.text
    }
}
