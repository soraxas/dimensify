use bevy::prelude::*;
use bevy_egui::egui::{self, WidgetText};
use bevy_inspector_egui::bevy_inspector::{
    Filter,
    hierarchy::{SelectedEntities, hierarchy_ui},
};
use egui_tiles::{self, Tiles, Tree};

use super::{PanelEntry, PanelFactory, PanelLocation, PanelRegistry, ViewerTab};
use crate::pane_widgets::{AssetViewKind, PaneWidgetStates};
use bevy_inspector_egui::bevy_inspector;

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
        "World",
        PanelLocation::Left,
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

impl ViewerTab for WorldInspectorTab {
    fn title(&self) -> &'static str {
        "World"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        egui::ScrollArea::both().show(ui, |ui| {
            bevy_inspector::ui_for_world(world, ui);
            ui.allocate_space(ui.available_size());
        });
    }
}

impl ViewerTab for ResourceInspectorTab {
    fn title(&self) -> &'static str {
        "Resources"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        egui::ScrollArea::both().show(ui, |ui| {
            bevy_inspector::ui_for_resources(world, ui);
            ui.allocate_space(ui.available_size());
        });
    }
}

impl ViewerTab for AssetsTab {
    fn title(&self) -> &'static str {
        "Assets"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        world.resource_scope(|world, mut states: Mut<PaneWidgetStates>| {
            let state = states.states.entry("ui.assets".to_string()).or_default();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.asset_view, AssetViewKind::Meshes, "Meshes");
                ui.selectable_value(&mut state.asset_view, AssetViewKind::Materials, "Materials");
                ui.selectable_value(&mut state.asset_view, AssetViewKind::Images, "Images");
            });
            egui::ScrollArea::both().show(ui, |ui| match state.asset_view {
                AssetViewKind::Meshes => bevy_inspector::ui_for_assets::<Mesh>(world, ui),
                AssetViewKind::Materials => {
                    bevy_inspector::ui_for_assets::<StandardMaterial>(world, ui)
                }
                AssetViewKind::Images => bevy_inspector::ui_for_assets::<Image>(world, ui),
            });
        });
    }
}

impl ViewerTab for FilterInspectorTab {
    fn title(&self) -> &'static str {
        "Filter"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        world.resource_scope(|world, mut states: Mut<PaneWidgetStates>| {
            let state = states.states.entry("ui.filter".to_string()).or_default();

            ui.checkbox(&mut state.filter_include_children, "Include children");
            egui::ScrollArea::both().show(ui, |ui| {
                let filter =
                    Filter::<With<Transform>>::from_ui_fuzzy(ui, egui::Id::new("filter_tab"));
                bevy_inspector::ui_for_entities_filtered(
                    world,
                    ui,
                    state.filter_include_children,
                    &filter,
                );
                ui.allocate_space(ui.available_size());
            });
        });
    }
}

impl ViewerTab for StateInspectorTab {
    fn title(&self) -> &'static str {
        "State"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        bevy_inspector::ui_for_state::<crate::tabs::DevUiState>(world, ui);
    }
}

impl ViewerTab for SidePanelInspectorTab {
    fn title(&self) -> &'static str {
        "Side Panels"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        world.resource_scope(|world, mut states: Mut<PaneWidgetStates>| {
            let state = states
                .states
                .entry("ui.side_panels".to_string())
                .or_default();

            let rect = ui.available_rect_before_wrap();
            let mut panel_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));

            ui.horizontal(|ui| {
                ui.checkbox(&mut state.side_panels_show_hierarchy, "Hierarchy");
                ui.checkbox(&mut state.side_panels_show_inspector, "Inspector");
            });

            if state.side_panels_show_hierarchy {
                egui::SidePanel::left("side_panel_hierarchy")
                    .resizable(true)
                    .default_width(200.0)
                    .show_inside(&mut panel_ui, |ui| {
                        ui.heading("Hierarchy");
                        world.resource_scope(
                            |world, mut selection: Mut<InspectorSelectionState>| {
                                hierarchy_ui(world, ui, &mut selection.selected_entities);
                            },
                        );
                    });
            }

            if state.side_panels_show_inspector {
                egui::CentralPanel::default().show_inside(&mut panel_ui, |ui| {
                    ui.heading("Inspector");
                    world.resource_scope(|world, selection: Mut<InspectorSelectionState>| {
                        match selection.selected_entities.as_slice() {
                            &[entity] => bevy_inspector::ui_for_entity(world, entity, ui),
                            entities => {
                                bevy_inspector::ui_for_entities_shared_components(
                                    world, entities, ui,
                                );
                            }
                        }
                    });
                });
            }

            ui.allocate_rect(rect, egui::Sense::hover());
        });
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

impl DockPane {
    pub fn pane_title(&self) -> &str {
        match self {
            DockPane::Viewport => "Viewport",
            DockPane::World => "World",
            DockPane::Hierarchy => "Hierarchy",
            DockPane::Inspector => "Inspector",
            DockPane::Resources => "Resources",
            DockPane::Assets => "Assets",
        }
    }

    pub fn pane_fancy_title(&self) -> impl Into<WidgetText> {
        // can implements fancy widget title
        self.pane_title()
    }

    pub fn from_title(title: &str) -> Option<Self> {
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

impl ViewerTab for ConsoleTab {
    fn title(&self) -> &'static str {
        "Console"
    }

    fn ui(&self, ui: &mut egui::Ui, _world: &mut World) {
        ui.heading("Console");
    }
}

impl ViewerTab for DiagnosticsTab {
    fn title(&self) -> &'static str {
        "Diagnostics"
    }

    fn ui(&self, ui: &mut egui::Ui, _world: &mut World) {
        ui.heading("Diagnostics");
    }
}

impl ViewerTab for TasksTab {
    fn title(&self) -> &'static str {
        "Tasks"
    }

    fn ui(&self, ui: &mut egui::Ui, _world: &mut World) {
        ui.heading("Tasks");
    }
}
