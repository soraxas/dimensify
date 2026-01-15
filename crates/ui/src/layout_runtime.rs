use bevy::prelude::World;
use bevy_egui::egui;
use egui_tiles::Tree;

use crate::{
    build_ui::{BottomPanelLayout, LeftPanelLayout, RightPanelLayout},
    layout_kdl::{DevUiLayoutSnapshot, LayoutNode, SplitDir},
    tabs::{self, PanelRegistry},
};

pub fn build_viewer_tree(
    node: &LayoutNode,
    tree_id: &str,
    registry: &PanelRegistry,
) -> Option<Tree<tabs::BoxedViewerTab>> {
    let mut tiles = egui_tiles::Tiles::default();
    let root = build_viewer_tiles(node, &mut tiles, registry)?;
    Some(egui_tiles::Tree::new(
        egui::Id::new(tree_id.to_string()),
        root,
        tiles,
    ))
}

fn build_viewer_tiles(
    node: &LayoutNode,
    tiles: &mut egui_tiles::Tiles<tabs::BoxedViewerTab>,
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

pub fn rebuild_viewer_layouts(world: &mut World, registry: &PanelRegistry) {
    let snapshot = world.resource::<DevUiLayoutSnapshot>().clone();
    world.insert_resource(LeftPanelLayout {
        tree: build_viewer_tree(&snapshot.left, "left_panel", registry),
    });
    world.insert_resource(RightPanelLayout {
        tree: build_viewer_tree(&snapshot.right, "right_panel", registry),
    });
    world.insert_resource(BottomPanelLayout {
        tree: build_viewer_tree(&snapshot.bottom, "bottom_panel", registry),
    });
}
