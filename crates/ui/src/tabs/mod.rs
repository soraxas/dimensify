use std::{collections::HashMap, sync::Arc};

use bevy::prelude::{Resource, World};
use bevy_egui::egui;

mod basics;

/// A pluggable panel rendered inside the UI tiles.
pub trait ViewerTab: Send + Sync {
    fn title(&self) -> &'static str;
    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World);
}

/// Boxed panel trait object used by egui_tiles.
pub type BoxedViewerTab = Box<dyn ViewerTab>;
/// Factory used to lazily construct panel instances on demand.
pub type PanelFactory = Arc<dyn Fn() -> BoxedViewerTab + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelLocation {
    Left,
    Right,
    Bottom,
    Floating,
}

/// Metadata and constructor for a single panel tab.
#[derive(Clone)]
pub struct PanelEntry {
    pub title: &'static str,
    pub location: PanelLocation,
    pub default_enabled: bool,
    pub factory: PanelFactory,
}

/// Registry for all known panels and their UI state.
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
                self.floating.insert(title.clone(), false);
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
}

pub use basics::{
    AssetsTab, ConsoleTab, DevUiState, DiagnosticsTab, DockUiState, FilterInspectorTab,
    HierarchyTab, InspectorSelectionState, InspectorTab, ResourceInspectorTab,
    SidePanelInspectorTab, StateInspectorTab, TasksTab, WorldInspectorTab, register_default_panels,
};

pub(crate) use basics::DockPane;
