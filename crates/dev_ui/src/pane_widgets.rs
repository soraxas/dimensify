use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::egui;
use bevy_inspector_egui::bevy_inspector::{
    self, Filter,
    hierarchy::{SelectedEntities, hierarchy_ui},
};

use crate::tabs::{DockPane, DockUiState, InspectorSelectionState};

#[derive(Clone, Copy, Debug)]
pub enum PaneKind {
    Hierarchy,
    Inspector,
    World,
    Resources,
    Assets,
    Filter,
    State,
    SidePanels,
    Console,
    Diagnostics,
    Tasks,
}

#[derive(Resource, Default)]
pub struct PaneWidgetStates {
    states: HashMap<String, PaneWidgetState>,
}

#[derive(Clone, Debug)]
struct PaneWidgetState {
    asset_view: AssetViewKind,
    filter_include_children: bool,
    side_panels_show_hierarchy: bool,
    side_panels_show_inspector: bool,
}

impl Default for PaneWidgetState {
    fn default() -> Self {
        Self {
            asset_view: AssetViewKind::Meshes,
            filter_include_children: true,
            side_panels_show_hierarchy: true,
            side_panels_show_inspector: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AssetViewKind {
    #[default]
    Meshes,
    Materials,
    Images,
}

pub fn add_pane_widget(ui: &mut egui::Ui, world: &mut World, id: &str, pane: PaneKind) {
    world.resource_scope(|world, mut states: Mut<PaneWidgetStates>| {
        let state = states.states.entry(id.to_string()).or_default();
        render_pane(ui, world, state, pane);
    });
}

fn render_pane(ui: &mut egui::Ui, world: &mut World, state: &mut PaneWidgetState, pane: PaneKind) {
    match pane {
        PaneKind::Hierarchy => {
            world.resource_scope(|world, mut selection: Mut<InspectorSelectionState>| {
                egui::ScrollArea::both().show(ui, |ui| {
                    hierarchy_ui(world, ui, &mut selection.selected_entities);
                    ui.allocate_space(ui.available_size());
                });
            });
        }
        PaneKind::Inspector => {
            world.resource_scope(|world, selection: Mut<InspectorSelectionState>| {
                egui::ScrollArea::both().show(ui, |ui| {
                    match selection.selected_entities.as_slice() {
                        &[entity] => bevy_inspector::ui_for_entity(world, entity, ui),
                        entities => {
                            bevy_inspector::ui_for_entities_shared_components(world, entities, ui);
                        }
                    }
                });
            });
        }
        PaneKind::World => {
            egui::ScrollArea::both().show(ui, |ui| {
                bevy_inspector::ui_for_world(world, ui);
                ui.allocate_space(ui.available_size());
            });
        }
        PaneKind::Resources => {
            egui::ScrollArea::both().show(ui, |ui| {
                bevy_inspector::ui_for_resources(world, ui);
                ui.allocate_space(ui.available_size());
            });
        }
        PaneKind::Assets => {
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
        }
        PaneKind::Filter => {
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
        }
        PaneKind::State => {
            bevy_inspector::ui_for_state::<crate::tabs::DevUiState>(world, ui);
        }
        PaneKind::SidePanels => {
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
        }
        PaneKind::Console => {
            ui.heading("Console");
        }
        PaneKind::Diagnostics => {
            ui.heading("Diagnostics");
        }
        PaneKind::Tasks => {
            ui.heading("Tasks");
        }
    }
}

struct DockBehavior<'a> {
    world: &'a mut World,
    selected_entities: &'a mut SelectedEntities,
    viewport_rect: &'a mut Option<egui::Rect>,
    viewport_active: &'a mut bool,
}

impl egui_tiles::Behavior<DockPane> for DockBehavior<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut DockPane,
    ) -> egui_tiles::UiResponse {
        match pane {
            DockPane::Viewport => {
                *self.viewport_rect = Some(ui.clip_rect());
                *self.viewport_active = true;
            }
            DockPane::World => {
                ui.heading("World");
                egui::ScrollArea::both().show(ui, |ui| {
                    bevy_inspector::ui_for_world(self.world, ui);
                });
            }
            DockPane::Hierarchy => {
                ui.heading("Hierarchy");
                egui::ScrollArea::both().show(ui, |ui| {
                    hierarchy_ui(self.world, ui, self.selected_entities);
                });
            }
            DockPane::Inspector => {
                ui.heading("Inspector");
                egui::ScrollArea::both().show(ui, |ui| match self.selected_entities.as_slice() {
                    &[entity] => bevy_inspector::ui_for_entity(self.world, entity, ui),
                    entities => {
                        bevy_inspector::ui_for_entities_shared_components(self.world, entities, ui);
                    }
                });
            }
            DockPane::Resources => {
                ui.heading("Resources");
                egui::ScrollArea::both().show(ui, |ui| {
                    bevy_inspector::ui_for_resources(self.world, ui);
                });
            }
            DockPane::Assets => {
                ui.heading("Assets");
                egui::ScrollArea::both().show(ui, |ui| {
                    ui.collapsing("Meshes", |ui| {
                        bevy_inspector::ui_for_assets::<Mesh>(self.world, ui);
                    });
                    ui.collapsing("Standard Materials", |ui| {
                        bevy_inspector::ui_for_assets::<StandardMaterial>(self.world, ui);
                    });
                    ui.collapsing("Images", |ui| {
                        bevy_inspector::ui_for_assets::<Image>(self.world, ui);
                    });
                });
            }
        }
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &DockPane) -> egui::WidgetText {
        match pane {
            DockPane::Viewport => "Viewport",
            DockPane::World => "World",
            DockPane::Hierarchy => "Hierarchy",
            DockPane::Inspector => "Inspector",
            DockPane::Resources => "Resources",
            DockPane::Assets => "Assets",
        }
        .into()
    }
}

pub fn render_dock_tree(ui: &mut egui::Ui, world: &mut World) {
    world.resource_scope(|world, mut dock_state: Mut<DockUiState>| {
        dock_state.viewport_rect = None;
        dock_state.viewport_active = false;
        let mut selected_entities = std::mem::take(&mut dock_state.selected_entities);
        let mut viewport_rect = dock_state.viewport_rect.take();
        let mut viewport_active = dock_state.viewport_active;
        let mut behavior = DockBehavior {
            world,
            selected_entities: &mut selected_entities,
            viewport_rect: &mut viewport_rect,
            viewport_active: &mut viewport_active,
        };
        dock_state.tree.ui(&mut behavior, ui);
        dock_state.selected_entities = selected_entities;
        dock_state.viewport_rect = viewport_rect.or_else(|| Some(ui.clip_rect()));
        dock_state.viewport_active = viewport_active;
    });
}
