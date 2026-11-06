use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
};

use bevy::prelude::*;
use bevy_egui::egui;
use material_design_icons as mdi;

#[derive(Resource, Debug, Clone)]
pub struct Theme {
    pub visuals: Option<egui::Visuals>,

    pub top_bar_color: egui::Color32,
    pub tab_bar_color: egui::Color32,
    pub bottom_bar_color: egui::Color32,
    pub panel_color: egui::Color32,
    pub border_color: egui::Color32,
    pub text_color: egui::Color32,
    pub text_subdued_color: egui::Color32,
    // spacing
    pub spacing_item_spacing: f32,
    pub spacing_edge: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            visuals: None,
            top_bar_color: egui::Color32::from_rgb(0x0d, 0x10, 0x11),
            tab_bar_color: egui::Color32::from_rgb(0x1c, 0x21, 0x23),
            bottom_bar_color: egui::Color32::from_rgb(0x14, 0x18, 0x19),
            panel_color: egui::Color32::from_rgb(0x0d, 0x10, 0x11),
            border_color: egui::Color32::from_rgb(0x26, 0x2b, 0x2e),
            text_color: egui::Color32::from_rgb(0xdb, 0xdf, 0xe2),
            text_subdued_color: egui::Color32::from_rgb(0x8a, 0x92, 0x96),
            //
            spacing_item_spacing: 6.0,
            spacing_edge: 8.0,
        }
    }
}

impl Theme {
    pub fn apply(&self, ctx: &egui::Context) {
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

        let visuals = match &self.visuals {
            Some(visuals) => visuals.clone(),
            None => {
                let mut visuals = egui::Visuals::dark();

                visuals.panel_fill = self.panel_color;
                visuals.window_fill = self.panel_color;
                visuals.extreme_bg_color = self.panel_color + egui::Color32::from_white_alpha(30);
                visuals.faint_bg_color = self.tab_bar_color;
                visuals.widgets.noninteractive.bg_fill = self.panel_color;
                visuals.widgets.noninteractive.fg_stroke.color = self.text_subdued_color;
                visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(0x10, 0x14, 0x16);
                visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x16, 0x1b, 0x1e);
                visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0x1b, 0x21, 0x24);
                visuals.widgets.inactive.fg_stroke.color = self.text_color;
                visuals.widgets.active.fg_stroke.color = self.text_color;
                visuals.widgets.hovered.fg_stroke.color = self.text_color;
                // Dark purple accent
                visuals.selection.bg_fill = egui::Color32::from_rgb(0x2a, 0x64, 0xb7);
                visuals.selection.stroke.color = egui::Color32::from_rgb(0x5c, 0x9d, 0xff);
                visuals.window_stroke = egui::Stroke::new(1.0, self.border_color);
                visuals.widgets.noninteractive.bg_stroke =
                    egui::Stroke::new(1.0, self.border_color);
                visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, self.border_color);
                visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.border_color);
                visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.border_color);
                visuals.override_text_color = Some(self.text_color);
                visuals
            }
        };

        style.visuals = visuals;
        ctx.set_style(style);
    }
}

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
pub fn icon_popout() -> egui::ImageSource<'static> {
    static ICON: OnceLock<egui::ImageSource<'static>> = OnceLock::new();
    ICON.get_or_init(|| mdi_icon("open-in-new", mdi::OPEN_IN_NEW))
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
