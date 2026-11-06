//! # Widget system (panels with placement)
//!
//! A **widget** is a boxed trait object that knows how to draw itself and can request
//! ECS data for rendering. The **widget system** is the registry that assigns each
//! widget a **placement**: left, right, bottom, or floating.
//!
//! ## Defining a widget
//!
//! 1. Implement the [`WidgetPane`] trait: provide [`WidgetPane::title`] and
//!    [`WidgetPane::ui`]. The `ui` method receives `&mut World` so the widget can
//!    access any system parameters (resources, queries) via `world.resource_scope`,
//!    `world.query_*`, etc.
//! 2. Register the widget with [`PanelRegistry`] and a [`PanelLocation`] using
//!    [`PanelRegistry::register_left`], [`register_right`](PanelRegistry::register_right),
//!    [`register_bottom`](PanelRegistry::register_bottom), or
//!    [`register_floating`](PanelRegistry::register_floating).
//!
//! The runtime stores widgets as [`BoxedViewerTab`] (i.e. `Box<dyn WidgetPane>`).

use std::{collections::HashMap, sync::Arc};

use bevy::prelude::{Resource, World};
use bevy_egui::egui;

mod basics;
mod ext_trait;
pub mod pane_widgets;

/// A pluggable widget (panel) rendered in the UI. Implement this trait and register
/// with [`PanelRegistry`] to add left/right/bottom or floating panels.
///
/// The widget is stored and rendered as a **trait object** ([`BoxedViewerTab`]).
/// Rendering receives `&mut World` so the widget can use any system parameters
/// (resources, queries) via `world.resource_scope`, `world.query_*`, etc.
pub trait WidgetPane: Send + Sync {
    fn title(&self) -> &'static str;
    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World);
    /// Allows the widget to decorate a floating window.
    /// Returns true if the widget decorated the window, false otherwise.
    fn decorate_floating_window<'a>(
        &mut self,
        window: egui::Window<'a>,
        _ctx: &mut egui::Context,
        _world: &mut World,
    ) -> (bool, egui::Window<'a>) {
        (false, window)
    }
}

/// Boxed widget trait object used by the layout (egui_tiles).
pub type BoxedViewerTab = Box<dyn WidgetPane>;
/// Alias for the boxed widget type (same as [`BoxedViewerTab`]).
pub type BoxedWidget = BoxedViewerTab;
/// Factory used to lazily construct widget instances on demand.
pub type PanelFactory = Arc<dyn Fn() -> BoxedViewerTab + Send + Sync>;

/// Where a widget can be placed: docked left/right/bottom or in a floating window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelLocation {
    Left,
    Right,
    Bottom,
    Floating,
}

/// Metadata and constructor for a single widget (panel).
#[derive(Clone)]
pub struct PanelEntry {
    pub title: &'static str,
    pub location: PanelLocation,
    pub default_enabled: bool,
    pub factory: PanelFactory,
}

/// Registry for all known widgets and their placement (left/right/bottom/floating).
#[derive(Resource, Default, Clone)]
pub struct PanelRegistry {
    entries: Vec<PanelEntry>,
    by_title: HashMap<String, usize>,
    enabled: HashMap<String, bool>,
    floating: HashMap<String, bool>,
}

impl PanelRegistry {
    /// Register a panel if it is not already known.
    pub fn register(&mut self, entry: PanelEntry) {
        let title = entry.title.to_string();
        if self.by_title.contains_key(&title) {
            return;
        }
        let index = self.entries.len();
        self.by_title.insert(title.clone(), index);
        self.enabled.insert(title.clone(), entry.default_enabled);
        match entry.location {
            PanelLocation::Left | PanelLocation::Right | PanelLocation::Bottom => {}
            PanelLocation::Floating => {
                self.floating.insert(title.clone(), entry.default_enabled);
            }
        }
        self.entries.push(entry);
    }

    /// Iterate all registered panel definitions in insertion order.
    pub fn entries(&self) -> impl Iterator<Item = &PanelEntry> {
        self.entries.iter()
    }

    /// Returns whether a panel is visible in a docked panel.
    pub fn is_enabled(&self, title: &str) -> bool {
        self.enabled.get(title).copied().unwrap_or(false)
    }

    /// Toggle whether a panel should be visible in its dock.
    pub fn set_enabled(&mut self, title: &str, enabled: bool) {
        if self.by_title.contains_key(title) {
            self.enabled.insert(title.to_string(), enabled);
        }
    }

    /// Returns whether a panel is floating in its own window.
    pub fn is_floating(&self, title: &str) -> bool {
        self.floating.get(title).copied().unwrap_or(false)
    }

    /// Toggle whether a panel should render in its own floating window.
    pub fn set_floating(&mut self, title: &str, floating: bool) {
        if self.by_title.contains_key(title) {
            self.floating.insert(title.to_string(), floating);
        }
    }

    /// Create a new panel instance by title.
    pub fn create_tab(&self, title: &str) -> Option<BoxedViewerTab> {
        let index = self.by_title.get(title)?;
        let entry = self.entries.get(*index)?;
        Some((entry.factory)())
    }

    /// First enabled, non-floating panel title, used for initial selection.
    pub fn first_enabled_title(&self) -> Option<&'static str> {
        self.entries
            .iter()
            .find(|entry| self.is_enabled(entry.title) && !self.is_floating(entry.title))
            .map(|entry| entry.title)
    }

    /// Update the current placement of a panel.
    ///
    /// This controls where the widget is docked (left/right/bottom) or whether
    /// it is treated as a \"floating\" panel for layout purposes. Callers are
    /// responsible for triggering any layout rebuilds that depend on location.
    pub fn set_location(&mut self, title: &str, location: PanelLocation) {
        if let Some(index) = self.by_title.get(title).copied()
            && let Some(entry) = self.entries.get_mut(index)
        {
            entry.location = location;
        }
    }

    /// Convenience helper to register a panel from a [`WidgetPane`] type.
    ///
    /// This is the recommended API for both built‑in panels and user panels:
    /// implement [`WidgetPane`] for your type, then call one of the
    /// `register_*` helpers below.
    pub fn register_panel<T>(
        &mut self,
        title: &'static str,
        location: PanelLocation,
        default_enabled: bool,
    ) where
        T: WidgetPane + Default + 'static,
    {
        self.register(PanelEntry {
            title,
            location,
            default_enabled,
            factory: Arc::new(|| Box::new(T::default())),
        });
    }

    /// Register a panel anchored to the left side.
    pub fn register_left<T>(&mut self, title: &'static str, default_enabled: bool)
    where
        T: WidgetPane + Default + 'static,
    {
        self.register_panel::<T>(title, PanelLocation::Left, default_enabled);
    }

    /// Register a panel anchored to the right side.
    pub fn register_right<T>(&mut self, title: &'static str, default_enabled: bool)
    where
        T: WidgetPane + Default + 'static,
    {
        self.register_panel::<T>(title, PanelLocation::Right, default_enabled);
    }

    /// Register a panel anchored to the bottom.
    pub fn register_bottom<T>(&mut self, title: &'static str, default_enabled: bool)
    where
        T: WidgetPane + Default + 'static,
    {
        self.register_panel::<T>(title, PanelLocation::Bottom, default_enabled);
    }

    /// Register a panel that opens as a floating window by default.
    pub fn register_floating<T>(&mut self, title: &'static str, default_enabled: bool)
    where
        T: WidgetPane + Default + 'static,
    {
        self.register_panel::<T>(title, PanelLocation::Floating, default_enabled);
    }
}

pub use basics::{
    AssetsTab, ConsoleTab, DiagnosticsTab, FilterInspectorTab, InspectorSelectionState,
    InspectorTab, ResourceInspectorTab, SidePanelInspectorTab, StateInspectorTab,
    WorldInspectorTab, register_default_panels,
};
pub use ext_trait::AppRegisterWidgetTrait;
