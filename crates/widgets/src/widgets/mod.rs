//! Modular Bevy egui widget system framework.
//!
//! This module provides a lightweight framework for creating reusable UI widgets
//! that integrate with Bevy's ECS system. Widgets maintain their own state and
//! can be easily composed and nested.

pub mod button;
pub mod input;
pub mod inspector;
pub mod label;

pub use button::{Button, ButtonArgs, ButtonStyle};
pub use input::{Input, InputArgs};
pub use inspector::{Inspectable, Inspector, InspectorArgs, ValueInspector};
pub use label::{Label, LabelArgs};

// Re-export core traits for convenience
pub use crate::widget::{RootWidgetSystem, RootWidgetSystemExt, WidgetSystem, WidgetSystemExt};
