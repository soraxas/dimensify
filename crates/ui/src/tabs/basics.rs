use bevy::prelude::*;
use bevy_egui::egui;
use bevy_inspector_egui::bevy_inspector::hierarchy::SelectedEntities;
use egui_tiles::{self, Tiles, Tree};

use super::{PanelEntry, PanelFactory, PanelLocation, PanelRegistry, ViewerTab};
use crate::pane_widgets::{PaneKind, add_pane_widget, render_dock_tree};

#[derive(Resource, Default)]
pub struct InspectorSelectionState {
    pub selected_entities: SelectedEntities,
}

#[derive(States, Reflect, Default, Debug, Hash, Eq, PartialEq, Clone)]
pub enum DevUiState {
    #[default]
    Live,
    Paused,
}

pub struct HierarchyTab;
pub struct InspectorTab;
pub struct WorldInspectorTab;
pub struct ResourceInspectorTab;
pub struct AssetsTab;
pub struct FilterInspectorTab;
pub struct StateInspectorTab;
pub struct SidePanelInspectorTab;
pub struct DockTab;
pub struct ConsoleTab;
pub struct DiagnosticsTab;
pub struct TasksTab;

pub fn register_default_panels(registry: &mut PanelRegistry) {
    let mut register = |title: &'static str,
                        location: PanelLocation,
                        default_enabled: bool,
                        factory: PanelFactory| {
        registry.register(PanelEntry {
            title,
            location,
            default_enabled,
            factory,
        });
    };

    register(
        "Hierarchy",
        PanelLocation::Left,
        true,
        std::sync::Arc::new(|| Box::new(HierarchyTab)),
    );
    register(
        "Inspector",
        PanelLocation::Left,
        true,
        std::sync::Arc::new(|| Box::new(InspectorTab)),
    );
    register(
        "World",
        PanelLocation::Right,
        true,
        std::sync::Arc::new(|| Box::new(WorldInspectorTab)),
    );
    register(
        "Resources",
        PanelLocation::Right,
        true,
        std::sync::Arc::new(|| Box::new(ResourceInspectorTab)),
    );
    register(
        "Assets",
        PanelLocation::Right,
        true,
        std::sync::Arc::new(|| Box::new(AssetsTab)),
    );
    register(
        "Console",
        PanelLocation::Bottom,
        true,
        std::sync::Arc::new(|| Box::new(ConsoleTab)),
    );
    register(
        "Diagnostics",
        PanelLocation::Bottom,
        true,
        std::sync::Arc::new(|| Box::new(DiagnosticsTab)),
    );
    register(
        "Tasks",
        PanelLocation::Bottom,
        true,
        std::sync::Arc::new(|| Box::new(TasksTab)),
    );

    register(
        "Filter",
        PanelLocation::Right,
        false,
        std::sync::Arc::new(|| Box::new(FilterInspectorTab)),
    );
    register(
        "State",
        PanelLocation::Right,
        false,
        std::sync::Arc::new(|| Box::new(StateInspectorTab)),
    );
    register(
        "Side Panels",
        PanelLocation::Right,
        false,
        std::sync::Arc::new(|| Box::new(SidePanelInspectorTab)),
    );
}

impl ViewerTab for HierarchyTab {
    fn title(&self) -> &'static str {
        "Hierarchy"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.hierarchy", PaneKind::Hierarchy);
    }
}

impl ViewerTab for InspectorTab {
    fn title(&self) -> &'static str {
        "Inspector"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.inspector", PaneKind::Inspector);
    }
}

impl ViewerTab for WorldInspectorTab {
    fn title(&self) -> &'static str {
        "World"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.world", PaneKind::World);
    }
}

impl ViewerTab for ResourceInspectorTab {
    fn title(&self) -> &'static str {
        "Resources"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.resources", PaneKind::Resources);
    }
}

impl ViewerTab for AssetsTab {
    fn title(&self) -> &'static str {
        "Assets"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.assets", PaneKind::Assets);
    }
}

impl ViewerTab for FilterInspectorTab {
    fn title(&self) -> &'static str {
        "Filter"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.filter", PaneKind::Filter);
    }
}

impl ViewerTab for StateInspectorTab {
    fn title(&self) -> &'static str {
        "State"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.state", PaneKind::State);
    }
}

impl ViewerTab for SidePanelInspectorTab {
    fn title(&self) -> &'static str {
        "Side Panels"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.side_panels", PaneKind::SidePanels);
    }
}

#[derive(Debug)]
pub(crate) enum DockPane {
    Viewport,
    World,
    Hierarchy,
    Inspector,
    Resources,
    Assets,
}

#[derive(Resource)]
pub struct DockUiState {
    pub(crate) tree: Tree<DockPane>,
    pub(crate) selected_entities: SelectedEntities,
    pub(crate) viewport_rect: Option<egui::Rect>,
    pub(crate) viewport_active: bool,
}

impl Default for DockUiState {
    fn default() -> Self {
        let mut tiles = Tiles::default();
        let viewport = tiles.insert_pane(DockPane::Viewport);
        let world = tiles.insert_pane(DockPane::World);
        let hierarchy = tiles.insert_pane(DockPane::Hierarchy);
        let inspector = tiles.insert_pane(DockPane::Inspector);
        let resources = tiles.insert_pane(DockPane::Resources);
        let assets = tiles.insert_pane(DockPane::Assets);

        let main = tiles.insert_tab_tile(vec![viewport, world, inspector]);
        let left = tiles.insert_tab_tile(vec![hierarchy]);
        let bottom = tiles.insert_tab_tile(vec![resources, assets]);
        let main = tiles.insert_container(egui_tiles::Container::new_horizontal(vec![left, main]));
        let root = tiles.insert_container(egui_tiles::Container::new_vertical(vec![main, bottom]));

        let tree = Tree::new("dock_tab", root, tiles);

        Self {
            tree,
            selected_entities: SelectedEntities::default(),
            viewport_rect: None,
            viewport_active: false,
        }
    }
}

impl ViewerTab for DockTab {
    fn title(&self) -> &'static str {
        "Dock"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        render_dock_tree(ui, world);
    }
}

impl ViewerTab for ConsoleTab {
    fn title(&self) -> &'static str {
        "Console"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.console", PaneKind::Console);
    }
}

impl ViewerTab for DiagnosticsTab {
    fn title(&self) -> &'static str {
        "Diagnostics"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.diagnostics", PaneKind::Diagnostics);
    }
}

impl ViewerTab for TasksTab {
    fn title(&self) -> &'static str {
        "Tasks"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        add_pane_widget(ui, world, "ui.tasks", PaneKind::Tasks);
    }
}
