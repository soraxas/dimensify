use super::ViewerTab;
use bevy_egui::egui;

pub struct HierarchyTab;
pub struct InspectorTab;
pub struct AssetsTab;
pub struct ConsoleTab;
pub struct DiagnosticsTab;
pub struct TasksTab;

impl ViewerTab for HierarchyTab {
    fn title(&self) -> &'static str {
        "Hierarchy"
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.heading("Hierarchy");
    }
}

impl ViewerTab for InspectorTab {
    fn title(&self) -> &'static str {
        "Inspector"
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.heading("Inspector");
    }
}

impl ViewerTab for AssetsTab {
    fn title(&self) -> &'static str {
        "Assets"
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.heading("Assets");
    }
}

impl ViewerTab for ConsoleTab {
    fn title(&self) -> &'static str {
        "Console"
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.heading("Console");
    }
}

impl ViewerTab for DiagnosticsTab {
    fn title(&self) -> &'static str {
        "Diagnostics"
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.heading("Diagnostics");
    }
}

impl ViewerTab for TasksTab {
    fn title(&self) -> &'static str {
        "Tasks"
    }

    fn ui(&self, ui: &mut egui::Ui) {
        ui.heading("Tasks");
    }
}
