use bevy::prelude::World;
use bevy_egui::egui;

mod basics;

pub trait ViewerTab: Send + Sync {
    fn title(&self) -> &'static str;
    fn ui(&self, ui: &mut egui::Ui, world: &mut World);
}

pub type BoxedViewerTab = Box<dyn ViewerTab>;

pub use basics::{
    AssetsTab, ConsoleTab, DevUiState, DiagnosticsTab, DockTab, DockUiState, FilterInspectorTab,
    HierarchyTab, InspectorSelectionState, InspectorTab, ResourceInspectorTab,
    SidePanelInspectorTab, StateInspectorTab, TasksTab, WorldInspectorTab,
};

pub(crate) use basics::DockPane;

pub fn tab_by_title(title: &str) -> Option<BoxedViewerTab> {
    match title {
        "Hierarchy" => Some(Box::new(HierarchyTab)),
        "Inspector" => Some(Box::new(InspectorTab)),
        "World" => Some(Box::new(WorldInspectorTab)),
        "Resources" => Some(Box::new(ResourceInspectorTab)),
        "Assets" => Some(Box::new(AssetsTab)),
        "Filter" => Some(Box::new(FilterInspectorTab)),
        "State" => Some(Box::new(StateInspectorTab)),
        "Side Panels" => Some(Box::new(SidePanelInspectorTab)),
        "Console" => Some(Box::new(ConsoleTab)),
        "Diagnostics" => Some(Box::new(DiagnosticsTab)),
        "Tasks" => Some(Box::new(TasksTab)),
        _ => None,
    }
}

pub(crate) fn dock_pane_by_title(title: &str) -> Option<DockPane> {
    match title {
        "Viewport" => Some(DockPane::Viewport),
        "World" => Some(DockPane::World),
        "Hierarchy" => Some(DockPane::Hierarchy),
        "Inspector" => Some(DockPane::Inspector),
        "Resources" => Some(DockPane::Resources),
        "Assets" => Some(DockPane::Assets),
        _ => None,
    }
}

pub(crate) fn dock_pane_title(pane: &DockPane) -> String {
    match pane {
        DockPane::Viewport => "Viewport",
        DockPane::World => "World",
        DockPane::Hierarchy => "Hierarchy",
        DockPane::Inspector => "Inspector",
        DockPane::Resources => "Resources",
        DockPane::Assets => "Assets",
    }
    .to_string()
}
