//! Viewer presentation follows the bundled Bevy Feathers gallery theme.
use bevy::{
    feathers::{
        constants::fonts,
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor},
        tokens,
    },
    prelude::*,
};

pub(super) fn text(assets: &AssetServer, value: impl Into<String>, size: f32) -> impl Bundle {
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

pub(super) fn panel() -> impl Bundle {
    (
        ThemeBackgroundColor(tokens::PANE_BODY_BG),
        ThemeBorderColor(tokens::PANE_HEADER_BORDER),
    )
}

pub(super) fn header() -> impl Bundle {
    (
        ThemeBackgroundColor(tokens::PANE_HEADER_BG),
        ThemeBorderColor(tokens::PANE_HEADER_BORDER),
    )
}
