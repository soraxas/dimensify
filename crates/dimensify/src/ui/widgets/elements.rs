use bevy_egui::egui;

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct ELabel {
    label: String,
    padding: egui::Margin,
    margin: egui::Margin,
    text_color: egui::Color32,
    height: Option<f32>,
    bottom_stroke: Option<egui::Stroke>,
}

impl ELabel {
    pub fn new(label: impl ToString) -> Self {
        Self {
            label: label.to_string(),
            padding: egui::Margin::same(8),
            margin: egui::Margin::same(0),
            text_color: egui::Color32::WHITE,
            bottom_stroke: None,
            height: None,
        }
    }

    pub fn text_color(mut self, color: egui::Color32) -> Self {
        self.text_color = color;
        self
    }

    pub fn bottom_stroke(mut self, stroke: egui::Stroke) -> Self {
        self.bottom_stroke = Some(stroke);
        self
    }

    pub fn padding(mut self, padding: egui::Margin) -> Self {
        self.padding = padding;
        self
    }

    pub fn margin(mut self, margin: egui::Margin) -> Self {
        self.margin = margin;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    fn render(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let style = ui.style_mut();
        let font_id = egui::TextStyle::Button.resolve(style);

        let default_height = font_id.size + self.margin.sum().y + self.padding.sum().y;
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), self.height.unwrap_or(default_height)),
            egui::Sense::click(),
        );

        let border_rect = shrink_rect(rect, self.margin);
        let text_rect = shrink_rect(border_rect, self.padding);

        if ui.is_rect_visible(rect) {
            let galley = ui
                .painter()
                .layout_no_wrap(self.label.clone(), font_id, self.text_color);
            let text_galley_rect = egui::Align2::LEFT_CENTER.anchor_rect(
                egui::Rect::from_min_size(text_rect.left_center(), galley.size()),
            );
            ui.painter()
                .galley(text_galley_rect.min, galley, self.text_color);

            if let Some(border) = self.bottom_stroke {
                ui.painter().hline(
                    border_rect.min.x..=border_rect.max.x,
                    border_rect.max.y,
                    border,
                );
            }
        }

        response
    }
}

impl egui::Widget for ELabel {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        self.render(ui)
    }
}

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct EImageButton {
    image_id: egui::TextureId,
    image_tint: egui::Color32,
    bg_color: egui::Color32,
    hovered_bg_color: egui::Color32,
    width: f32,
    height: f32,
}

impl EImageButton {
    pub fn new(image_id: egui::TextureId) -> Self {
        Self {
            image_id,
            image_tint: egui::Color32::WHITE,
            bg_color: egui::Color32::TRANSPARENT,
            hovered_bg_color: egui::Color32::from_gray(40),
            width: 1.0,
            height: 1.0,
        }
    }

    pub fn scale(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn bg_color(mut self, bg_color: egui::Color32) -> Self {
        self.bg_color = bg_color;
        self
    }

    pub fn hovered_bg_color(mut self, bg_color: egui::Color32) -> Self {
        self.hovered_bg_color = bg_color;
        self
    }

    pub fn image_tint(mut self, image_tint: egui::Color32) -> Self {
        self.image_tint = image_tint;
        self
    }

    fn render(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let min_interact_size = ui.spacing().interact_size.y;
        let (rect, response) = ui.allocate_exact_size(
            min_interact_size * egui::vec2(self.width, self.height),
            egui::Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            let bg_color = if response.hovered() || response.clicked() {
                self.hovered_bg_color
            } else {
                self.bg_color
            };

            ui.painter().rect(
                rect,
                egui::CornerRadius::same(3),
                bg_color,
                egui::Stroke::NONE,
                egui::StrokeKind::Middle,
            );

            let default_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            let image_rect = rect.shrink(3.0);

            ui.painter()
                .image(self.image_id, image_rect, default_uv, self.image_tint);
        }

        response
    }
}

impl egui::Widget for EImageButton {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        self.render(ui)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct ECheckboxButton<'a> {
    checked: &'a mut bool,
    label: String,
    margin: egui::Margin,
}

impl<'a> ECheckboxButton<'a> {
    pub fn new(label: impl ToString, checked: &'a mut bool) -> Self {
        Self {
            checked,
            label: label.to_string(),
            margin: egui::Margin::same(6),
        }
    }

    pub fn margin(mut self, margin: egui::Margin) -> Self {
        self.margin = margin;
        self
    }
}

impl egui::Widget for ECheckboxButton<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        egui::Frame::NONE
            .inner_margin(self.margin)
            .show(ui, |ui| ui.checkbox(self.checked, self.label))
            .response
    }
}

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct EColorButton {
    color: egui::Color32,
    corner_radius: egui::CornerRadius,
}

impl EColorButton {
    pub fn new(color: egui::Color32) -> Self {
        Self {
            color,
            corner_radius: egui::CornerRadius::same(2),
        }
    }

    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = color;
        self
    }
}

impl egui::Widget for EColorButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = egui::vec2(16.0, 16.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

        if ui.is_rect_visible(rect) {
            ui.painter().rect(
                rect,
                self.corner_radius,
                self.color,
                egui::Stroke::new(1.0, self.color),
                egui::StrokeKind::Middle,
            );
        }

        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct EButton {
    label: String,
    disabled: bool,
    color: egui::Color32,
    bg_color: egui::Color32,
    margin: egui::Margin,
    corner_radius: egui::CornerRadius,
    stroke: egui::Stroke,
    width: Option<f32>,
    loading: bool,
}

impl EButton {
    pub fn new(label: impl ToString) -> Self {
        Self {
            label: label.to_string(),
            disabled: false,
            color: egui::Color32::WHITE,
            bg_color: egui::Color32::from_gray(40),
            stroke: egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
            corner_radius: egui::CornerRadius::same(2),
            margin: egui::Margin::same(8),
            width: None,
            loading: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn color(mut self, color: egui::Color32) -> Self {
        self.color = color;
        self
    }

    pub fn bg_color(mut self, color: egui::Color32) -> Self {
        self.bg_color = color;
        self
    }

    pub fn stroke(mut self, stroke: egui::Stroke) -> Self {
        self.stroke = stroke;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
}

impl egui::Widget for EButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let font_id = egui::TextStyle::Button.resolve(ui.style());
        let galley = ui
            .painter()
            .layout_no_wrap(self.label.clone(), font_id.clone(), self.color);

        let desired_width = self.width.unwrap_or(galley.size().x + self.margin.sum().x);
        let desired_size = egui::vec2(desired_width, galley.size().y + self.margin.sum().y);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

        if ui.is_rect_visible(rect) {
            if self.disabled {
                ui.disable();
            }

            let hovered = response.hovered() && !self.disabled;
            let active = response.is_pointer_button_down_on() && !self.disabled;
            let bg_color = if active {
                with_opacity(self.bg_color, 0.6)
            } else if hovered {
                with_opacity(self.bg_color, 0.9)
            } else {
                self.bg_color
            };

            ui.painter().rect(
                rect,
                self.corner_radius,
                bg_color,
                self.stroke,
                egui::StrokeKind::Middle,
            );

            if self.loading {
                egui::Spinner::new().paint_at(
                    ui,
                    egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0)),
                );
                ui.disable();
            }

            let inner_rect = shrink_rect(rect, self.margin);
            ui.painter().text(
                inner_rect.center(),
                egui::Align2::CENTER_CENTER,
                self.label.clone(),
                font_id,
                self.color,
            );
        }

        response.on_hover_cursor(if self.disabled {
            egui::CursorIcon::Default
        } else {
            egui::CursorIcon::PointingHand
        })
    }
}

#[must_use = "You should put this widget in an ui with `ui.add(widget);`"]
pub struct ETileButton {
    label: String,
    description: Option<String>,
    image_id: egui::TextureId,
    width: Option<f32>,
    height: Option<f32>,
}

impl ETileButton {
    pub fn new(label: impl ToString, image_id: egui::TextureId) -> Self {
        Self {
            label: label.to_string(),
            description: None,
            image_id,
            width: None,
            height: None,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn description(mut self, description: impl ToString) -> Self {
        self.description = Some(description.to_string());
        self
    }
}

impl egui::Widget for ETileButton {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let desired_size = egui::vec2(
            self.width.unwrap_or(ui.available_width()),
            self.height.unwrap_or(ui.available_height()),
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

        if ui.is_rect_visible(rect) {
            let font_id = egui::TextStyle::Button.resolve(ui.style());

            let hovered = response.hovered();
            let active = response.is_pointer_button_down_on();
            let base = egui::Color32::from_gray(35);
            let bg = if active {
                with_opacity(base, 0.6)
            } else if hovered {
                with_opacity(base, 0.9)
            } else {
                with_opacity(base, 0.35)
            };

            ui.painter().rect(
                rect,
                egui::CornerRadius::same(1),
                bg,
                egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
                egui::StrokeKind::Middle,
            );

            let label_rect = ui.painter().text(
                egui::pos2(
                    rect.center().x,
                    rect.center().y + (ui.spacing().interact_size.y / 2.0),
                ),
                egui::Align2::CENTER_CENTER,
                self.label.to_uppercase(),
                font_id.clone(),
                egui::Color32::WHITE,
            );

            let label_rect = label_rect.expand(8.0);
            let default_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            let image_side = ui.spacing().interact_size.y;
            let image_rect = egui::Rect::from_center_size(
                egui::pos2(
                    label_rect.center_top().x,
                    label_rect.center_top().y - image_side,
                ),
                egui::vec2(image_side, image_side),
            );

            ui.painter()
                .image(self.image_id, image_rect, default_uv, egui::Color32::WHITE);

            if let Some(description) = &self.description {
                ui.painter().text(
                    label_rect.center_bottom(),
                    egui::Align2::CENTER_TOP,
                    description.to_string(),
                    font_id,
                    egui::Color32::from_gray(200),
                );
            }
        }

        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

fn shrink_rect(rect: egui::Rect, margin: egui::Margin) -> egui::Rect {
    egui::Rect::from_min_max(
        rect.min + egui::vec2(margin.left.into(), margin.top.into()),
        rect.max - egui::vec2(margin.right.into(), margin.bottom.into()),
    )
}

fn with_opacity(color: egui::Color32, opacity: f32) -> egui::Color32 {
    let alpha = (color.a() as f32 * opacity).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}
