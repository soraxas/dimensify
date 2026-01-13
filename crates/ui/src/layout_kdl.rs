use std::path::{Path, PathBuf};

use bevy::{
    log::{info, warn},
    prelude::{Commands, Mut, Resource, World},
};
use bevy_egui::egui;

use crate::{
    build_ui::{BottomPanelLayout, LeftPanelLayout, RightPanelLayout},
    tabs::{self, DockPane, DockUiState, PanelRegistry},
};

#[derive(Clone, Debug)]
pub struct DevUiLayout {
    pub left: LayoutNode,
    pub right: LayoutNode,
    pub bottom: LayoutNode,
    pub dock: LayoutNode,
}

#[derive(Clone, Debug)]
pub enum LayoutNode {
    Tabs(Vec<String>),
    Split {
        dir: SplitDir,
        children: Vec<LayoutNode>,
    },
    Grid(Vec<LayoutNode>),
}

#[derive(Clone, Copy, Debug)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Resource)]
pub struct DevUiLayoutPath(pub PathBuf);

#[derive(Clone, Debug, Resource)]
pub struct DevUiLayoutSnapshot {
    pub left: LayoutNode,
    pub right: LayoutNode,
    pub bottom: LayoutNode,
    pub dock: LayoutNode,
}

impl DevUiLayout {
    pub fn default_layout() -> Self {
        Self {
            left: LayoutNode::Tabs(
                vec!["Hierarchy", "Inspector"]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            right: LayoutNode::Tabs(
                vec!["World", "Resources", "Assets"]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            bottom: LayoutNode::Tabs(
                vec!["Console", "Diagnostics", "Tasks"]
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            dock: LayoutNode::Tabs(vec!["Viewport".to_string()]),
        }
    }
}

pub fn resolve_layout_path() -> PathBuf {
    std::env::var("DIMENSIFY_DEV_UI_LAYOUT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("dev_ui_layout.kdl"))
}

pub fn load_layout_from_path(path: &Path) -> DevUiLayout {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return DevUiLayout::default_layout();
    };
    match parse_layout(&contents) {
        Ok(layout) => layout,
        Err(err) => {
            warn!("Failed to parse ui layout from {:?}: {}", path, err);
            DevUiLayout::default_layout()
        }
    }
}

pub fn save_layout_to_path(path: &Path, layout: &DevUiLayout) {
    if let Some(parent) = path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        warn!("Failed to create ui layout directory {:?}: {}", parent, err);
    }
    if let Err(err) = std::fs::write(path, layout_to_kdl(layout)) {
        warn!("Failed to save ui layout to {:?}: {}", path, err);
    } else {
        info!("Saved ui layout to {:?}", path);
    }
}

pub fn snapshot_layout(world: &World) -> DevUiLayout {
    let left = layout_from_viewer_tree(&world.resource::<LeftPanelLayout>().tree);
    let right = layout_from_viewer_tree(&world.resource::<RightPanelLayout>().tree);
    let bottom = layout_from_viewer_tree(&world.resource::<BottomPanelLayout>().tree);
    let dock = layout_from_dock_tree(&world.resource::<DockUiState>().tree);
    DevUiLayout {
        left,
        right,
        bottom,
        dock,
    }
}

pub fn apply_layout_from_startup(
    commands: &mut Commands,
    layout: DevUiLayout,
    registry: &PanelRegistry,
) {
    commands.insert_resource(LeftPanelLayout {
        tree: build_viewer_tree(&layout.left, "left_panel", registry),
    });
    commands.insert_resource(RightPanelLayout {
        tree: build_viewer_tree(&layout.right, "right_panel", registry),
    });
    commands.insert_resource(BottomPanelLayout {
        tree: build_viewer_tree(&layout.bottom, "bottom_panel", registry),
    });
    commands.insert_resource(DockUiState {
        tree: build_dock_tree(&layout.dock, "dock_panel"),
        selected_entities: Default::default(),
        viewport_rect: None,
        viewport_active: false,
    });
    commands.insert_resource(DevUiLayoutSnapshot {
        left: layout.left,
        right: layout.right,
        bottom: layout.bottom,
        dock: layout.dock,
    });
}

pub fn apply_layout(world: &mut World, layout: DevUiLayout, registry: &PanelRegistry) {
    world.insert_resource(LeftPanelLayout {
        tree: build_viewer_tree(&layout.left, "left_panel", registry),
    });
    world.insert_resource(RightPanelLayout {
        tree: build_viewer_tree(&layout.right, "right_panel", registry),
    });
    world.insert_resource(BottomPanelLayout {
        tree: build_viewer_tree(&layout.bottom, "bottom_panel", registry),
    });
    world.resource_scope(|_world, mut dock_state: Mut<DockUiState>| {
        dock_state.tree = build_dock_tree(&layout.dock, "dock_panel");
    });
    world.insert_resource(DevUiLayoutSnapshot {
        left: layout.left,
        right: layout.right,
        bottom: layout.bottom,
        dock: layout.dock,
    });
}

pub fn reload_layout(world: &mut World) {
    let path = world.resource::<DevUiLayoutPath>().0.clone();
    let layout = load_layout_from_path(&path);
    let registry = world.resource::<PanelRegistry>().clone();
    apply_layout(world, layout, &registry);
}

pub fn save_current_layout(world: &World) {
    let path = world.resource::<DevUiLayoutPath>().0.clone();
    let layout = snapshot_layout(world);
    save_layout_to_path(&path, &layout);
}

fn layout_from_viewer_tree(tree: &egui_tiles::Tree<tabs::BoxedViewerTab>) -> LayoutNode {
    let Some(root) = tree.root() else {
        return LayoutNode::Tabs(Vec::new());
    };
    let title = |pane: &tabs::BoxedViewerTab| pane.title().to_string();
    layout_from_tile(tree, root, &title)
}

fn layout_from_dock_tree(tree: &egui_tiles::Tree<DockPane>) -> LayoutNode {
    let Some(root) = tree.root() else {
        return LayoutNode::Tabs(Vec::new());
    };
    layout_from_tile(tree, root, &tabs::dock_pane_title)
}

fn layout_from_tile<T, F>(
    tree: &egui_tiles::Tree<T>,
    tile_id: egui_tiles::TileId,
    pane_title: &F,
) -> LayoutNode
where
    F: Fn(&T) -> String,
{
    let Some(tile) = tree.tiles.get(tile_id) else {
        return LayoutNode::Tabs(Vec::new());
    };
    match tile {
        egui_tiles::Tile::Pane(pane) => LayoutNode::Tabs(vec![pane_title(pane)]),
        egui_tiles::Tile::Container(container) => match container.kind() {
            egui_tiles::ContainerKind::Tabs => {
                let mut titles = Vec::new();
                for child in container.children() {
                    if let Some(egui_tiles::Tile::Pane(pane)) = tree.tiles.get(*child) {
                        titles.push(pane_title(pane));
                    }
                }
                LayoutNode::Tabs(titles)
            }
            egui_tiles::ContainerKind::Horizontal => LayoutNode::Split {
                dir: SplitDir::Horizontal,
                children: container
                    .children()
                    .map(|child| layout_from_tile(tree, *child, pane_title))
                    .collect(),
            },
            egui_tiles::ContainerKind::Vertical => LayoutNode::Split {
                dir: SplitDir::Vertical,
                children: container
                    .children()
                    .map(|child| layout_from_tile(tree, *child, pane_title))
                    .collect(),
            },
            egui_tiles::ContainerKind::Grid => LayoutNode::Grid(
                container
                    .children()
                    .map(|child| layout_from_tile(tree, *child, pane_title))
                    .collect(),
            ),
        },
    }
}

pub fn build_viewer_tree(
    node: &LayoutNode,
    tree_id: &str,
    registry: &PanelRegistry,
) -> egui_tiles::Tree<tabs::BoxedViewerTab> {
    let mut tiles = egui_tiles::Tiles::default();
    let root = build_viewer_tiles(node, &mut tiles, registry);
    egui_tiles::Tree::new(egui::Id::new(tree_id.to_string()), root, tiles)
}

fn build_dock_tree(node: &LayoutNode, tree_id: &str) -> egui_tiles::Tree<DockPane> {
    let mut tiles = egui_tiles::Tiles::default();
    let root = build_dock_tiles(node, &mut tiles);
    egui_tiles::Tree::new(egui::Id::new(tree_id.to_string()), root, tiles)
}

fn build_viewer_tiles(
    node: &LayoutNode,
    tiles: &mut egui_tiles::Tiles<tabs::BoxedViewerTab>,
    registry: &PanelRegistry,
) -> egui_tiles::TileId {
    match node {
        LayoutNode::Tabs(titles) => {
            let mut panes = Vec::new();
            for title in titles {
                if registry.is_enabled(title)
                    && !registry.is_floating(title)
                    && let Some(pane) = registry.create_tab(title)
                {
                    panes.push(tiles.insert_pane(pane));
                } else {
                    warn!("Unknown ui tab title '{}', skipping", title);
                }
            }
            if panes.is_empty() {
                let fallback = registry
                    .first_enabled_title()
                    .or_else(|| registry.entries().next().map(|entry| entry.title));
                if let Some(title) = fallback
                    && let Some(pane) = registry.create_tab(title)
                {
                    panes.push(tiles.insert_pane(pane));
                }
            }
            tiles.insert_tab_tile(panes)
        }
        LayoutNode::Split { dir, children } => {
            let child_ids = children
                .iter()
                .map(|child| build_viewer_tiles(child, tiles, registry))
                .collect::<Vec<_>>();
            let container = match dir {
                SplitDir::Horizontal => egui_tiles::Container::new_horizontal(child_ids),
                SplitDir::Vertical => egui_tiles::Container::new_vertical(child_ids),
            };
            tiles.insert_container(container)
        }
        LayoutNode::Grid(children) => {
            let child_ids = children
                .iter()
                .map(|child| build_viewer_tiles(child, tiles, registry))
                .collect::<Vec<_>>();
            tiles.insert_container(egui_tiles::Container::new_grid(child_ids))
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

fn build_dock_tiles(
    node: &LayoutNode,
    tiles: &mut egui_tiles::Tiles<DockPane>,
) -> egui_tiles::TileId {
    match node {
        LayoutNode::Tabs(titles) => {
            let mut panes = Vec::new();
            for title in titles {
                if let Some(pane) = tabs::dock_pane_by_title(title) {
                    panes.push(tiles.insert_pane(pane));
                } else {
                    warn!("Unknown ui dock pane '{}', skipping", title);
                }
            }
            if panes.is_empty() {
                panes.push(tiles.insert_pane(DockPane::World));
            }
            tiles.insert_tab_tile(panes)
        }
        LayoutNode::Split { dir, children } => {
            let child_ids = children
                .iter()
                .map(|child| build_dock_tiles(child, tiles))
                .collect::<Vec<_>>();
            let container = match dir {
                SplitDir::Horizontal => egui_tiles::Container::new_horizontal(child_ids),
                SplitDir::Vertical => egui_tiles::Container::new_vertical(child_ids),
            };
            tiles.insert_container(container)
        }
        LayoutNode::Grid(children) => {
            let child_ids = children
                .iter()
                .map(|child| build_dock_tiles(child, tiles))
                .collect::<Vec<_>>();
            tiles.insert_container(egui_tiles::Container::new_grid(child_ids))
        }
    }
}

fn parse_layout(src: &str) -> Result<DevUiLayout, String> {
    let nodes = parse_kdl(src)?;
    let mut left = None;
    let mut right = None;
    let mut bottom = None;
    let mut dock = None;

    for node in &nodes {
        if node.name == "layout" {
            for child in &node.children {
                parse_panel_node(child, &mut left, &mut right, &mut bottom, &mut dock)?;
            }
            continue;
        }
        parse_panel_node(node, &mut left, &mut right, &mut bottom, &mut dock)?;
    }

    let default = DevUiLayout::default_layout();
    Ok(DevUiLayout {
        left: left.unwrap_or(default.left),
        right: right.unwrap_or(default.right),
        bottom: bottom.unwrap_or(default.bottom),
        dock: dock.unwrap_or(default.dock),
    })
}

fn parse_panel_node(
    node: &AstNode,
    left: &mut Option<LayoutNode>,
    right: &mut Option<LayoutNode>,
    bottom: &mut Option<LayoutNode>,
    dock: &mut Option<LayoutNode>,
) -> Result<(), String> {
    if node.name != "panel" {
        return Ok(());
    }
    let panel_name = node
        .args
        .first()
        .ok_or_else(|| "panel name missing".to_string())?;
    let Some(layout_node) = node.children.first() else {
        return Ok(());
    };
    let parsed = parse_layout_node(layout_node)?;
    match panel_name.as_str() {
        "left" => *left = Some(parsed),
        "right" => *right = Some(parsed),
        "bottom" => *bottom = Some(parsed),
        "dock" => *dock = Some(parsed),
        _ => {}
    }
    Ok(())
}

fn parse_layout_node(node: &AstNode) -> Result<LayoutNode, String> {
    match node.name.as_str() {
        "tabs" => {
            let mut titles = Vec::new();
            for child in &node.children {
                if child.name != "tab" {
                    continue;
                }
                if let Some(title) = child.args.first() {
                    titles.push(title.to_string());
                }
            }
            Ok(LayoutNode::Tabs(titles))
        }
        "hsplit" | "vsplit" | "grid" => {
            let mut children_nodes = Vec::new();
            for child in &node.children {
                children_nodes.push(parse_layout_node(child)?);
            }
            match node.name.as_str() {
                "hsplit" => Ok(LayoutNode::Split {
                    dir: SplitDir::Horizontal,
                    children: children_nodes,
                }),
                "vsplit" => Ok(LayoutNode::Split {
                    dir: SplitDir::Vertical,
                    children: children_nodes,
                }),
                "grid" => Ok(LayoutNode::Grid(children_nodes)),
                _ => Err(format!("Unknown layout node '{}'", node.name)),
            }
        }
        other => Err(format!("Unknown layout node '{}'", other)),
    }
}

fn layout_to_kdl(layout: &DevUiLayout) -> String {
    let mut out = String::new();
    out.push_str("layout {\n");
    write_panel(&mut out, 1, "left", &layout.left);
    write_panel(&mut out, 1, "right", &layout.right);
    write_panel(&mut out, 1, "bottom", &layout.bottom);
    write_panel(&mut out, 1, "dock", &layout.dock);
    out.push_str("}\n");
    out
}

fn write_panel(out: &mut String, indent: usize, name: &str, node: &LayoutNode) {
    indent_line(out, indent);
    out.push_str("panel \"");
    out.push_str(name);
    out.push_str("\" {\n");
    write_node(out, indent + 1, node);
    indent_line(out, indent);
    out.push_str("}\n");
}

fn write_node(out: &mut String, indent: usize, node: &LayoutNode) {
    match node {
        LayoutNode::Tabs(titles) => {
            indent_line(out, indent);
            out.push_str("tabs {\n");
            for title in titles {
                indent_line(out, indent + 1);
                out.push_str("tab \"");
                out.push_str(title);
                out.push_str("\"\n");
            }
            indent_line(out, indent);
            out.push_str("}\n");
        }
        LayoutNode::Split { dir, children } => {
            let name = match dir {
                SplitDir::Horizontal => "hsplit",
                SplitDir::Vertical => "vsplit",
            };
            indent_line(out, indent);
            out.push_str(name);
            out.push_str(" {\n");
            for child in children {
                write_node(out, indent + 1, child);
            }
            indent_line(out, indent);
            out.push_str("}\n");
        }
        LayoutNode::Grid(children) => {
            indent_line(out, indent);
            out.push_str("grid {\n");
            for child in children {
                write_node(out, indent + 1, child);
            }
            indent_line(out, indent);
            out.push_str("}\n");
        }
    }
}

fn indent_line(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

#[derive(Debug)]
struct AstNode {
    name: String,
    args: Vec<String>,
    children: Vec<AstNode>,
}

#[derive(Debug, Clone)]
enum Token<'a> {
    Ident(&'a str),
    Str(String),
    LBrace,
    RBrace,
}

fn parse_kdl(src: &str) -> Result<Vec<AstNode>, String> {
    let tokens = tokenize(src)?;
    let mut cursor = 0;
    parse_nodes(&tokens, &mut cursor)
}

fn parse_nodes<'a>(tokens: &[Token<'a>], cursor: &mut usize) -> Result<Vec<AstNode>, String> {
    let mut nodes = Vec::new();
    while *cursor < tokens.len() {
        match tokens[*cursor] {
            Token::RBrace => break,
            _ => nodes.push(parse_node(tokens, cursor)?),
        }
    }
    Ok(nodes)
}

fn parse_node<'a>(tokens: &[Token<'a>], cursor: &mut usize) -> Result<AstNode, String> {
    let name = match tokens.get(*cursor) {
        Some(Token::Ident(name)) => {
            *cursor += 1;
            name.to_string()
        }
        _ => return Err("expected node identifier".to_string()),
    };

    let mut args = Vec::new();
    while let Some(Token::Str(value)) = tokens.get(*cursor) {
        args.push(value.clone());
        *cursor += 1;
    }

    let mut children = Vec::new();
    if matches!(tokens.get(*cursor), Some(Token::LBrace)) {
        *cursor += 1;
        children = parse_nodes(tokens, cursor)?;
        if !matches!(tokens.get(*cursor), Some(Token::RBrace)) {
            return Err("missing closing '}'".to_string());
        }
        *cursor += 1;
    }

    Ok(AstNode {
        name,
        args,
        children,
    })
}

fn tokenize(src: &str) -> Result<Vec<Token<'_>>, String> {
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();

    while let Some((idx, ch)) = chars.peek().copied() {
        match ch {
            '{' => {
                chars.next();
                tokens.push(Token::LBrace);
            }
            '}' => {
                chars.next();
                tokens.push(Token::RBrace);
            }
            '"' => {
                chars.next();
                let mut buf = String::new();
                let mut done = false;
                while let Some((_, next)) = chars.next() {
                    match next {
                        '"' => {
                            done = true;
                            break;
                        }
                        '\\' => {
                            if let Some((_, escaped)) = chars.next() {
                                buf.push(escaped);
                            }
                        }
                        _ => buf.push(next),
                    }
                }
                if !done {
                    return Err("unterminated string literal".to_string());
                }
                tokens.push(Token::Str(buf));
            }
            '/' => {
                if let Some((_, '/')) = chars.clone().nth(1) {
                    for (_, next) in chars.by_ref() {
                        if next == '\n' {
                            break;
                        }
                    }
                } else {
                    return Err(format!("unexpected '/' at {}", idx));
                }
            }
            '#' => {
                for (_, next) in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            _ => {
                let start = idx;
                while let Some((_, next)) = chars.peek() {
                    if next.is_whitespace() || *next == '{' || *next == '}' {
                        break;
                    }
                    chars.next();
                }
                let end = chars.peek().map(|(i, _)| *i).unwrap_or(src.len());
                let ident = src[start..end].trim();
                if !ident.is_empty() {
                    tokens.push(Token::Ident(ident));
                }
            }
        }
    }

    Ok(tokens)
}
