use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
};

use bevy::prelude::Resource;
use bevy_egui::egui;
use material_design_icons as mdi;

pub const TOP_BAR_COLOR: egui::Color32 = egui::Color32::from_rgb(0x0d, 0x10, 0x11);
pub const TAB_BAR_COLOR: egui::Color32 = egui::Color32::from_rgb(0x1c, 0x21, 0x23);
pub const BOTTOM_BAR_COLOR: egui::Color32 = egui::Color32::from_rgb(0x14, 0x18, 0x19);
pub const PANEL_COLOR: egui::Color32 = egui::Color32::from_rgb(0x0d, 0x10, 0x11);
pub const BORDER_COLOR: egui::Color32 = egui::Color32::from_rgb(0x26, 0x2b, 0x2e);
pub const TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(0xdb, 0xdf, 0xe2);
pub const TEXT_SUBDUED: egui::Color32 = egui::Color32::from_rgb(0x8a, 0x92, 0x96);

// pub const ICON_LEFT_PANEL: egui::ImageSource<'static> =
//     egui::include_image!("../assets/dimensify/icons/left_panel_toggle.svg");
// pub const ICON_RIGHT_PANEL: egui::ImageSource<'static> =
//     egui::include_image!("../assets/dimensify/icons/right_panel_toggle.svg");
// pub const ICON_BOTTOM_PANEL: egui::ImageSource<'static> =
//     egui::include_image!("../assets/dimensify/icons/bottom_panel_toggle.svg");
// // pub const ICON_LOGO: egui::ImageSource<'static> =
// //     egui::include_image!("../assets/dimensify/icons/dimensify_logo.svg");

pub const SPACING_ITEM_SPACING: f32 = 6.0;
pub const SPACING_EDGE: f32 = 8.0;

#[derive(Resource, Default, Clone, Copy)]
pub struct StyleApplied(pub bool);

pub fn icon_left_panel() -> egui::ImageSource<'static> {
    // return ICON_LEFT_PANEL;
    static ICON: OnceLock<egui::ImageSource<'static>> = OnceLock::new();
    ICON.get_or_init(|| mdi_icon("dock-left", mdi::DOCK_LEFT))
        .clone()
}

pub fn icon_right_panel() -> egui::ImageSource<'static> {
    static ICON: OnceLock<egui::ImageSource<'static>> = OnceLock::new();
    ICON.get_or_init(|| mdi_icon("dock-right", mdi::DOCK_RIGHT))
        .clone()
}

pub fn icon_bottom_panel() -> egui::ImageSource<'static> {
    static ICON: OnceLock<egui::ImageSource<'static>> = OnceLock::new();
    ICON.get_or_init(|| mdi_icon("dock-bottom", mdi::DOCK_BOTTOM))
        .clone()
}

/// Create an SVG icon from a Material Design Icon with a white fill.
fn mdi_icon(name: &str, path: &str) -> egui::ImageSource<'static> {
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"white\"><path d=\"{}\"/></svg>",
        path
    );
    egui::ImageSource::Bytes {
        uri: Cow::Owned(format!("bytes://mdi/{}.svg", name)),
        bytes: egui::load::Bytes::from(Arc::from(svg.into_bytes())),
    }
}

pub fn apply_dimensify_style(ctx: &egui::Context, applied: &mut StyleApplied) {
    if applied.0 {
        return;
    }

    egui_extras::install_image_loaders(ctx);

    // let mut fonts = egui::FontDefinitions::default();
    // fonts.font_data.insert(
    //     "InterMedium".to_string(),
    //     egui::FontData::from_static(include_bytes!("../assets/dimensify/FiraCodeNerdFont-Light.ttf")).into(),
    //     // egui::FontData::from_static(include_bytes!("../assets/dimensify/FiraCodeNerdFont-Medium.ttf")).into(),
    //     // egui::FontData::from_static(include_bytes!("../assets/dimensify/Inter-Medium.otf")).into(),
    // );
    // fonts
    //     .families
    //     .entry(egui::FontFamily::Proportional)
    //     .or_default()
    //     .insert(0, "InterMedium".to_string());
    // fonts
    //     .families
    //     .entry(egui::FontFamily::Monospace)
    //     .or_default()
    //     .insert(0, "InterMedium".to_string());
    // ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(13.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(12.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(11.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(12.0, egui::FontFamily::Monospace),
        ),
    ]
    .into();

    // style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    // style.spacing.button_padding = egui::vec2(8.0, 4.0);
    // style.spacing.window_margin = egui::Margin::symmetric(6, 6);
    // style.spacing.menu_margin = egui::Margin::symmetric(4, 4);
    // style.spacing.icon_spacing = 6.0;

    // let mut visuals = egui::Visuals::dark();
    // visuals.panel_fill = PANEL_COLOR;
    // visuals.window_fill = PANEL_COLOR;
    // visuals.extreme_bg_color = PANEL_COLOR;
    // visuals.faint_bg_color = TAB_BAR_COLOR;
    // visuals.widgets.noninteractive.bg_fill = PANEL_COLOR;
    // visuals.widgets.noninteractive.fg_stroke.color = TEXT_SUBDUED;
    // visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x10, 0x14, 0x16);
    // visuals.widgets.inactive.fg_stroke.color = TEXT_COLOR;
    // visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x16, 0x1b, 0x1e);
    // visuals.widgets.active.fg_stroke.color = TEXT_COLOR;
    // visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x1b, 0x21, 0x24);
    // visuals.widgets.hovered.fg_stroke.color = TEXT_COLOR;
    // visuals.selection.bg_fill = egui::Color32::from_rgb(0x2a, 0x64, 0xb7);
    // visuals.selection.stroke.color = egui::Color32::from_rgb(0x5c, 0x9d, 0xff);
    // visuals.window_stroke = egui::Stroke::new(1.0, BORDER_COLOR);
    // visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER_COLOR);
    // visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER_COLOR);
    // visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, BORDER_COLOR);
    // visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, BORDER_COLOR);
    // visuals.override_text_color = Some(TEXT_COLOR);
    // style.visuals = visuals;

    ctx.set_style(style);
    applied.0 = true;
}
