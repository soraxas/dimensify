use bevy::prelude::*;
use bevy_egui::egui::{self};
use bevy_inspector_egui::bevy_inspector::{
    Filter,
    hierarchy::{SelectedEntities, hierarchy_ui},
};

use super::{PanelRegistry, WidgetPane};
use crate::widget::pane_widgets::{AssetViewKind, PaneWidgetStates};
use bevy_inspector_egui::bevy_inspector;

#[derive(Resource, Default)]
pub struct InspectorSelectionState {
    pub selected_entities: SelectedEntities,
}

#[derive(Default)]
pub struct InspectorTab;
#[derive(Default)]
pub struct WorldInspectorTab;
#[derive(Default)]
pub struct ResourceInspectorTab;
#[derive(Default)]
pub struct AssetsTab;
#[derive(Default)]
pub struct FilterInspectorTab;
#[derive(Default)]
pub struct StateInspectorTab;
#[derive(Default)]
pub struct SidePanelInspectorTab;
#[derive(Default)]
pub struct ConsoleTab;
#[derive(Default)]
pub struct DiagnosticsTab;

pub fn register_default_panels(registry: &mut PanelRegistry) {
    registry.register_left::<WorldInspectorTab>("World", true);
    registry.register_right::<ResourceInspectorTab>("Resources", true);
    registry.register_right::<AssetsTab>("Assets", true);
    registry.register_bottom::<ConsoleTab>("Console", true);
    registry.register_bottom::<DiagnosticsTab>("Diagnostics", true);

    registry.register_right::<FilterInspectorTab>("Filter", false);
    registry.register_right::<SidePanelInspectorTab>("Side Panels", false);
}

impl WidgetPane for WorldInspectorTab {
    fn title(&self) -> &'static str {
        "World"
    }

    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World) {
        egui::ScrollArea::both().show(ui, |ui| {
            bevy_inspector::ui_for_world(world, ui);
            ui.allocate_space(ui.available_size());
        });
    }
}

impl WidgetPane for ResourceInspectorTab {
    fn title(&self) -> &'static str {
        "Resources"
    }

    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World) {
        egui::ScrollArea::both().show(ui, |ui| {
            bevy_inspector::ui_for_resources(world, ui);
            ui.allocate_space(ui.available_size());
        });
    }
}

impl WidgetPane for AssetsTab {
    fn title(&self) -> &'static str {
        "Assets"
    }

    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World) {
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

impl WidgetPane for FilterInspectorTab {
    fn title(&self) -> &'static str {
        "Filter"
    }

    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World) {
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

impl WidgetPane for SidePanelInspectorTab {
    fn title(&self) -> &'static str {
        "Side Panels"
    }

    fn ui(&mut self, ui: &mut egui::Ui, world: &mut World) {
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

impl WidgetPane for ConsoleTab {
    fn title(&self) -> &'static str {
        "Console"
    }

    fn ui(&mut self, ui: &mut egui::Ui, _world: &mut World) {
        ui.heading("Console");
    }
}

impl WidgetPane for DiagnosticsTab {
    fn title(&self) -> &'static str {
        "Diagnostics"
    }

    fn ui(&mut self, ui: &mut egui::Ui, _world: &mut World) {
        ui.heading("Diagnostics");
    }
}
