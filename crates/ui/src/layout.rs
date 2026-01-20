use std::path::{Path, PathBuf};

use bevy::prelude::Resource;

/// Serializable layout definition used for saving and restoring UI state.
#[derive(Clone, Debug)]
pub struct DevUiLayout {
    pub left: LayoutNode,
    pub right: LayoutNode,
    pub bottom: LayoutNode,
    pub dock: LayoutNode,
}

/// Declarative layout node used in the KDL format.
#[derive(Clone, Debug)]
pub enum LayoutNode {
    Tabs(Vec<String>),
    Split {
        dir: SplitDir,
        children: Vec<LayoutNode>,
    },
    Grid(Vec<LayoutNode>),
}

/// Split direction for layout nodes.
#[derive(Clone, Copy, Debug)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

/// Resource containing the on-disk layout path.
#[derive(Clone, Debug, Resource)]
pub struct DevUiLayoutPath(pub Option<PathBuf>);

/// Snapshot of the last applied layout for rebuilds.
#[derive(Clone, Debug, Resource)]
pub struct DevUiLayoutSnapshot {
    pub left: LayoutNode,
    pub right: LayoutNode,
    pub bottom: LayoutNode,
    pub dock: LayoutNode,
}

impl DevUiLayout {
    /// Default layout used when the layout file is missing or invalid.
    pub fn default_layout() -> Self {
        Self {
            left: LayoutNode::Tabs(
                // vec!["World"].into_iter().map(|s| s.to_string()).collect()
                vec![],
            ),
            right: LayoutNode::Tabs(
                vec!["Resources", "Assets"]
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
