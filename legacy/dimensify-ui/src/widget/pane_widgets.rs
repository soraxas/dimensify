use std::collections::HashMap;

use bevy::prelude::*;
#[derive(Resource, Default)]
pub struct PaneWidgetStates {
    pub states: HashMap<String, PaneWidgetState>,
}

#[derive(Clone, Debug)]
pub struct PaneWidgetState {
    pub(crate) asset_view: AssetViewKind,
    pub(crate) filter_include_children: bool,
    pub(crate) side_panels_show_hierarchy: bool,
    pub(crate) side_panels_show_inspector: bool,
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
pub enum AssetViewKind {
    #[default]
    Meshes,
    Materials,
    Images,
}
