//! Viewer presentation follows the bundled Bevy Feathers gallery theme.
use bevy::{
    feathers::{
        constants::fonts,
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor},
        tokens,
    },
    prelude::*,
};

pub fn text(assets: &AssetServer, value: impl Into<String>, size: f32) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: assets.load(fonts::REGULAR).into(),
            font_size: FontSize::Px(size),
            ..default()
        },
        ThemeTextColor(tokens::TEXT_MAIN),
    )
}

pub fn panel() -> impl Bundle {
    (
        ThemeBackgroundColor(tokens::PANE_BODY_BG),
        ThemeBorderColor(tokens::PANE_HEADER_BORDER),
    )
}

pub fn header() -> impl Bundle {
    (
        ThemeBackgroundColor(tokens::PANE_HEADER_BG),
        ThemeBorderColor(tokens::PANE_HEADER_BORDER),
    )
}

/// Retained, wrapping toolbar layout used by the standalone viewers.
pub fn toolbar() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: px(16),
        left: px(16),
        right: px(16),
        padding: UiRect::all(px(12)),
        column_gap: px(16),
        row_gap: px(8),
        flex_wrap: FlexWrap::Wrap,
        align_items: AlignItems::Center,
        border_radius: BorderRadius::all(px(4)),
        border: UiRect::all(px(1)),
        ..default()
    }
}

/// Toolbar button layout; use with a Feathers button for themed interaction.
pub fn button() -> Node {
    Node {
        min_width: px(108),
        height: px(44),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        padding: UiRect::horizontal(px(8)),
        border_radius: BorderRadius::all(px(4)),
        ..default()
    }
}

/// Scrollable right-side panel, clear of the toolbar and bottom status area.
/// Spawn with `ScrollPosition` and `PanelMotion` for scrolling and motion.
pub fn panel_node(width: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: px(104),
        bottom: px(120),
        right: px(16),
        width: px(width),
        max_width: percent(94),
        padding: UiRect::all(px(14)),
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(4)),
        row_gap: px(8),
        flex_direction: FlexDirection::Column,
        overflow: Overflow::scroll_y(),
        ..default()
    }
}

/// Bottom status card layout with the same insets and typography spacing as splat.
pub fn status_node() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(16),
        right: px(16),
        bottom: px(16),
        padding: UiRect::all(px(14)),
        flex_direction: FlexDirection::Column,
        row_gap: px(6),
        border_radius: BorderRadius::all(px(4)),
        ..default()
    }
}
