use bevy_egui::egui;

mod basics;

pub trait ViewerTab: Send + Sync {
    fn title(&self) -> &'static str;
    fn ui(&self, ui: &mut egui::Ui);
}

pub use basics::{AssetsTab, ConsoleTab, DiagnosticsTab, HierarchyTab, InspectorTab, TasksTab};
