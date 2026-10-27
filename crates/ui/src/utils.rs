use bevy_egui::egui;

use super::style::Theme;

/// Panel toggle button with a medium size icon.
pub fn medium_icon_toggle_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    alt_text: &str,
    selected: &mut bool,
    theme: &Theme,
) -> egui::Response {
    let size_points = egui::Vec2::splat(16.0);
    let tint = if *selected {
        theme.text_color
    } else {
        theme.text_subdued_color
    };
    let image = egui::Image::new(icon)
        .fit_to_exact_size(size_points)
        .tint(tint);
    let mut response = ui.add(egui::Button::image(image));
    if response.clicked() {
        *selected = !*selected;
        response.mark_changed();
    }
    response.on_hover_text(alt_text)
}

pub fn panel_frame(theme: &Theme) -> egui::Frame {
    egui::Frame::NONE
        .fill(theme.panel_color)
        .stroke(egui::Stroke::new(1.0, theme.border_color))
}
