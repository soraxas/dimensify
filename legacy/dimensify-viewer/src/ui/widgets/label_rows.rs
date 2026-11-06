use bevy_egui::egui;

use super::EImageButton;

pub fn label_with_buttons<const N: usize>(
    ui: &mut egui::Ui,
    btn_icons: [egui::TextureId; N],
    label: impl ToString,
    color: egui::Color32,
    margin: egui::Margin,
) -> [bool; N] {
    let mut clicked = [false; N];

    egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (label_rect, btn_rect) =
                split_rect(ui.max_rect(), 0.8, ui.spacing().interact_size.y);

            ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let text = egui::RichText::new(label.to_string()).color(color);
                    ui.add(egui::Label::new(text));
                });
            });

            ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (i, btn_icon) in btn_icons.iter().enumerate() {
                        let btn = ui.add(
                            EImageButton::new(*btn_icon)
                                .scale(1.2, 1.2)
                                .image_tint(color)
                                .bg_color(egui::Color32::TRANSPARENT),
                        );

                        if btn.clicked() {
                            clicked[i] = true;
                        }
                    }
                });
            });
        });
    });

    clicked
}

pub fn editable_label_with_buttons<const N: usize>(
    ui: &mut egui::Ui,
    btn_icons: [egui::TextureId; N],
    label: &mut String,
    color: egui::Color32,
    margin: egui::Margin,
) -> [bool; N] {
    let mut clicked = [false; N];

    egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (label_rect, btn_rect) =
                split_rect(ui.max_rect(), 0.8, ui.spacing().interact_size.y);

            ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let mut font_id = egui::TextStyle::Button.resolve(ui.style());
                    font_id.size = 12.0;
                    ui.add(egui::TextEdit::singleline(label).font(font_id).margin(8.0));
                });
            });

            ui.scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (i, btn_icon) in btn_icons.iter().enumerate() {
                        let btn = ui.add(
                            EImageButton::new(*btn_icon)
                                .scale(1.2, 1.2)
                                .image_tint(color)
                                .bg_color(egui::Color32::TRANSPARENT),
                        );

                        if btn.clicked() {
                            clicked[i] = true;
                        }
                    }
                });
            });
        });
    });

    clicked
}

fn split_rect(rect: egui::Rect, label_ratio: f32, button_width: f32) -> (egui::Rect, egui::Rect) {
    let total_width = rect.width();
    let label_width = (total_width * label_ratio).max(0.0);
    let button_width = button_width.max(0.0);

    let label_rect = egui::Rect::from_min_size(rect.min, egui::vec2(label_width, rect.height()));
    let btn_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - button_width * 1.2 * 3.0, rect.min.y),
        egui::vec2(rect.max.x - label_rect.max.x, rect.height()),
    );
    (label_rect, btn_rect)
}
