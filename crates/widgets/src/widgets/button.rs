//! Button widget for handling click interactions.

use crate::widget::WidgetSystem;
use bevy::{
    ecs::system::{SystemParam, SystemState},
    prelude::*,
};
use bevy_egui::egui;

/// Arguments for the Button widget.
#[derive(Debug, Clone)]
pub struct ButtonArgs {
    /// The text label displayed on the button
    pub text: String,
    /// Optional width for the button. If None, button will size to content.
    pub width: Option<f32>,
    /// Optional height for the button. If None, button will size to content.
    pub height: Option<f32>,
    /// Optional button style customization
    pub style: Option<ButtonStyle>,
}

/// Button style customization options.
#[derive(Debug, Clone)]
pub struct ButtonStyle {
    /// Fill color when not hovered
    pub fill: Option<egui::Color32>,
    /// Fill color when hovered
    pub hovered_fill: Option<egui::Color32>,
    /// Fill color when clicked
    pub clicked_fill: Option<egui::Color32>,
    /// Text color
    pub text_color: Option<egui::Color32>,
}

impl Default for ButtonArgs {
    fn default() -> Self {
        Self {
            text: "Button".to_string(),
            width: None,
            height: None,
            style: None,
        }
    }
}

/// Button widget system that renders a clickable button.
///
/// To handle button clicks, check the returned boolean value or use Bevy events.
///
/// # Example
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_egui::egui;
/// use widgets::widgets::{WidgetSystemExt, Button};
///
/// fn my_ui(ui: &mut egui::Ui, world: &mut World) {
///     let clicked = ui.add_widget_with::<Button>(world, "my_button", ButtonArgs {
///         text: "Click Me".to_string(),
///         ..Default::default()
///     });
///
///     if clicked {
///         println!("Button was clicked!");
///     }
/// }
/// ```
#[derive(SystemParam)]
pub struct Button<'w, 's> {
    _marker: std::marker::PhantomData<(&'w (), &'s ())>,
}

impl WidgetSystem for Button<'_, '_> {
    type Args = ButtonArgs;
    type Output = bool; // Returns true if clicked this frame

    fn ui_system(
        _world: &mut World,
        _state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        args: Self::Args,
    ) -> Self::Output {
        // Apply style if provided
        if let Some(style) = &args.style {
            if let Some(fill) = style.fill {
                ui.style_mut().visuals.widgets.inactive.bg_fill = fill;
            }
            if let Some(hovered) = style.hovered_fill {
                ui.style_mut().visuals.widgets.hovered.bg_fill = hovered;
            }
            if let Some(clicked) = style.clicked_fill {
                ui.style_mut().visuals.widgets.active.bg_fill = clicked;
            }
        }

        let mut button = egui::Button::new(&args.text);

        // Apply text color if provided
        if let Some(style) = &args.style {
            if let Some(text_color) = style.text_color {
                button = button.fill(text_color);
            }
        }

        let response = if let (Some(width), Some(height)) = (args.width, args.height) {
            ui.add_sized([width, height], button)
        } else if let Some(width) = args.width {
            ui.add_sized([width, 0.0], button)
        } else if let Some(height) = args.height {
            ui.add_sized([0.0, height], button)
        } else {
            ui.add(button)
        };

        response.clicked()
    }
}
