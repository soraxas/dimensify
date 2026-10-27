use bevy::prelude::World;
use bevy_egui::egui;
use egui_tiles::Tree;

use crate::{
    build_ui::{BottomPanelLayout, LeftPanelLayout, RightPanelLayout},
    layout::{LayoutNode, SplitDir, UiLayoutSnapshot},
    widget::{self, PanelLocation, PanelRegistry},
};

/// Build a viewer panel tree from a layout node, filtering out disabled/floating panels.
pub fn build_viewer_tree(
    node: &LayoutNode,
    tree_id: &str,
    registry: &PanelRegistry,
) -> Option<Tree<widget::BoxedViewerTab>> {
    let mut tiles = egui_tiles::Tiles::default();
    let root = build_viewer_tiles(node, &mut tiles, registry)?;
    Some(egui_tiles::Tree::new(
        egui::Id::new(tree_id.to_string()),
        root,
        tiles,
    ))
}

/// Recursively materialize tiles for viewer panels; returns None when nothing is visible.
fn build_viewer_tiles(
    node: &LayoutNode,
    tiles: &mut egui_tiles::Tiles<widget::BoxedViewerTab>,
    registry: &PanelRegistry,
) -> Option<egui_tiles::TileId> {
    match node {
        LayoutNode::Tabs(titles) => {
            let mut panes = Vec::new();
            for title in titles {
                if !registry.is_enabled(title) || registry.is_floating(title) {
                    continue;
                }
                if let Some(pane) = registry.create_tab(title) {
                    panes.push(tiles.insert_pane(pane));
                }
            }
            if panes.is_empty() {
                return None;
            }
            Some(tiles.insert_tab_tile(panes))
        }
        LayoutNode::Split { dir, children } => {
            let mut child_ids = Vec::new();
            for child in children {
                if let Some(child_id) = build_viewer_tiles(child, tiles, registry) {
                    child_ids.push(child_id);
                }
            }
            match child_ids.len() {
                0 => None,
                1 => Some(child_ids[0]),
                _ => {
                    let container = match dir {
                        SplitDir::Horizontal => egui_tiles::Container::new_horizontal(child_ids),
                        SplitDir::Vertical => egui_tiles::Container::new_vertical(child_ids),
                    };
                    Some(tiles.insert_container(container))
                }
            }
        }
        LayoutNode::Grid(children) => {
            let mut child_ids = Vec::new();
            for child in children {
                if let Some(child_id) = build_viewer_tiles(child, tiles, registry) {
                    child_ids.push(child_id);
                }
            }
            if child_ids.is_empty() {
                None
            } else if child_ids.len() == 1 {
                Some(child_ids[0])
            } else {
                Some(tiles.insert_container(egui_tiles::Container::new_grid(child_ids)))
            }
        }
    }
}

/// Rebuild the docked panel trees after registry or layout changes.
pub fn rebuild_viewer_layouts(world: &mut World, registry: &PanelRegistry) {
    let snapshot = world.resource::<UiLayoutSnapshot>().clone();

    // Rebuild the left/right/bottom layout nodes so that panels follow their
    // current `PanelLocation` in the registry. This keeps user changes from the
    // panel manager UI in sync with the docked layout.
    fn reassign_side(
        original: &LayoutNode,
        side: PanelLocation,
        registry: &PanelRegistry,
    ) -> LayoutNode {
        match original {
            LayoutNode::Tabs(existing_titles) => {
                let mut titles: Vec<String> = Vec::new();

                // Preserve titles that are not managed by the registry.
                for t in existing_titles {
                    let managed = registry.entries().any(|e| e.title == t);
                    if !managed {
                        titles.push(t.clone());
                    }
                }

                // Add all registry entries whose location matches this side.
                for entry in registry.entries() {
                    if entry.location == side && !titles.iter().any(|t| t == entry.title) {
                        titles.push(entry.title.to_string());
                    }
                }

                LayoutNode::Tabs(titles)
            }
            // For more complex layouts (splits/grids), keep as-is for now.
            _ => original.clone(),
        }
    }

    let left_node = reassign_side(&snapshot.left, PanelLocation::Left, registry);
    let right_node = reassign_side(&snapshot.right, PanelLocation::Right, registry);
    let bottom_node = reassign_side(&snapshot.bottom, PanelLocation::Bottom, registry);
    world.insert_resource(LeftPanelLayout {
        tree: build_viewer_tree(&left_node, "left_panel", registry),
    });
    world.insert_resource(RightPanelLayout {
        tree: build_viewer_tree(&right_node, "right_panel", registry),
    });
    world.insert_resource(BottomPanelLayout {
        tree: build_viewer_tree(&bottom_node, "bottom_panel", registry),
    });
}
