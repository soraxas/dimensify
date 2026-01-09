#[cfg(feature = "protocol")]
use crate::stream::DataSource;
use bevy::{
    ecs::{
        query::{QueryFilter, With},
        system::{SystemParam, SystemState},
        world::{Mut, World},
    },
    prelude::{Entity, Res, ResMut, Resource},
    window::PrimaryWindow,
};
use bevy_egui::{EguiContext, EguiContexts, egui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub trait RootWidgetSystemExt {
    /// Adds a root widget to the primary window.
    fn add_root_widget<S: RootWidgetSystem<Args = ()> + 'static>(&mut self, id: &str) -> S::Output {
        self.add_root_widget_with::<S, With<PrimaryWindow>>(id, ())
            .expect("missing window")
    }

    /// Add a root widget with a specific filter.
    fn add_root_widget_with<S: RootWidgetSystem + 'static, F: QueryFilter>(
        &mut self,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output>;

    /// Add a root widget to a specific entity.
    fn add_root_widget_to<S: RootWidgetSystem + 'static>(
        &mut self,
        entity: Entity,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output>;

    /// Execute a closure with the egui context from a query filter.
    fn egui_context_scope<R, F: QueryFilter>(
        &mut self,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R>;

    /// Execute a closure with the egui context from a specific entity.
    fn egui_context_scope_for<R>(
        &mut self,
        id: Entity,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R>;
}

impl RootWidgetSystemExt for World {
    fn add_root_widget_with<S: RootWidgetSystem + 'static, F: QueryFilter>(
        &mut self,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output> {
        self.egui_context_scope::<_, F>(|world, mut ctx| {
            let id = WidgetId::new(id);

            if !world.contains_resource::<StateInstances<S>>() {
                world.insert_resource(StateInstances::<S> {
                    instances: HashMap::new(),
                });
            }

            world.resource_scope(|world, mut states: Mut<StateInstances<S>>| {
                let cached_state = states
                    .instances
                    .entry(id)
                    .or_insert_with(|| SystemState::new(world));
                let output = S::ctx_system(world, cached_state, &mut ctx, args);
                cached_state.apply(world);
                output
            })
        })
    }

    fn add_root_widget_to<S: RootWidgetSystem + 'static>(
        &mut self,
        entity: Entity,
        id: &str,
        args: S::Args,
    ) -> Option<S::Output> {
        self.egui_context_scope_for::<_>(entity, |world, mut ctx| {
            let id = WidgetId::new(id);

            if !world.contains_resource::<StateInstances<S>>() {
                world.insert_resource(StateInstances::<S> {
                    instances: HashMap::new(),
                });
            }

            world.resource_scope(|world, mut states: Mut<StateInstances<S>>| {
                let cached_state = states
                    .instances
                    .entry(id)
                    .or_insert_with(|| SystemState::new(world));
                let output = S::ctx_system(world, cached_state, &mut ctx, args);
                cached_state.apply(world);
                output
            })
        })
    }

    fn egui_context_scope<R, F: QueryFilter>(
        &mut self,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R> {
        let mut state = self.query_filtered::<&mut EguiContext, (With<EguiContext>, F)>();
        let Ok(mut egui_ctx) = state.single_mut(self) else {
            return None;
        };
        let ctx = egui_ctx.get_mut().clone();
        Some(f(self, ctx))
    }

    fn egui_context_scope_for<R>(
        &mut self,
        id: Entity,
        f: impl FnOnce(&mut Self, egui::Context) -> R,
    ) -> Option<R> {
        let mut egui_ctx = self.get_mut::<EguiContext>(id)?;
        let ctx = egui_ctx.get_mut().clone();
        Some(f(self, ctx))
    }
}

/// Extension trait for adding widgets to an egui UI.
pub trait WidgetSystemExt {
    /// Adds a widget to the UI without arguments.
    fn add_widget<S: WidgetSystem<Args = ()> + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
    ) -> S::Output {
        self.add_widget_with::<S>(world, id, ())
    }

    /// Adds a widget to the UI with custom arguments.
    fn add_widget_with<S: WidgetSystem + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
        args: S::Args,
    ) -> S::Output;
}

impl WidgetSystemExt for egui::Ui {
    fn add_widget_with<S: WidgetSystem + 'static>(
        &mut self,
        world: &mut World,
        id: &str,
        args: S::Args,
    ) -> S::Output {
        let id = WidgetId::new(id);

        if !world.contains_resource::<StateInstances<S>>() {
            world.insert_resource(StateInstances::<S> {
                instances: HashMap::new(),
            });
        }

        world.resource_scope(|world, mut states: Mut<StateInstances<S>>| {
            let cached_state = states
                .instances
                .entry(id)
                .or_insert_with(|| SystemState::new(world));
            let output = S::ui_system(world, cached_state, self, args);
            cached_state.apply(world);
            output
        })
    }
}

/// Trait for root widget systems that operate on the egui context level.
pub trait RootWidgetSystem: SystemParam {
    type Args;
    type Output;

    fn ctx_system(
        world: &mut World,
        state: &mut SystemState<Self>,
        ctx: &mut egui::Context,
        args: Self::Args,
    ) -> Self::Output;
}

/// Trait for widget systems that operate within an existing egui UI.
pub trait WidgetSystem: SystemParam {
    type Args;
    type Output;

    fn ui_system(
        world: &mut World,
        state: &mut SystemState<Self>,
        ui: &mut egui::Ui,
        args: Self::Args,
    ) -> Self::Output;
}

#[derive(Resource, Default)]
struct StateInstances<T: SystemParam + 'static> {
    instances: HashMap<WidgetId, SystemState<T>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(pub u64);

impl WidgetId {
    pub const fn new(str: &str) -> Self {
        Self(const_fnv1a_hash::fnv1a_hash_str_64(str))
    }
}

pub type BoxedWidget<'a> = Box<dyn FnOnce(&mut egui::Ui) + 'a>;

pub trait BoxedWidgetExt<'a> {
    fn boxed(self) -> BoxedWidget<'a>;
}

impl<'a, T> BoxedWidgetExt<'a> for T
where
    T: egui::Widget + 'a,
{
    fn boxed(self) -> BoxedWidget<'a> {
        Box::new(move |ui| {
            self.ui(ui);
        })
    }
}

pub trait BoxedWidgetFnExt<'a> {
    fn boxed_fn(self) -> BoxedWidget<'a>;
}

impl<'a, F> BoxedWidgetFnExt<'a> for F
where
    F: FnOnce(&mut egui::Ui) + 'a,
{
    fn boxed_fn(self) -> BoxedWidget<'a> {
        Box::new(self)
    }
}

pub trait DynWidget: Send + Sync {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response;
}

impl<F> DynWidget for F
where
    F: FnMut(&mut egui::Ui) -> egui::Response + Send + Sync + 'static,
{
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        (self)(ui)
    }
}

#[derive(Resource, Default)]
pub struct WidgetRegistry {
    widgets: HashMap<String, Box<dyn DynWidget>>,
}

impl WidgetRegistry {
    pub fn register<F>(&mut self, id: impl Into<String>, widget: F)
    where
        F: FnMut(&mut egui::Ui) -> egui::Response + Send + Sync + 'static,
    {
        self.widgets.insert(id.into(), Box::new(widget));
    }

    pub fn unregister(&mut self, id: &str) -> Option<Box<dyn DynWidget>> {
        self.widgets.remove(id)
    }

    pub fn show(&mut self, id: &str, ui: &mut egui::Ui) -> Option<egui::Response> {
        let Some(widget) = self.widgets.get_mut(id) else {
            bevy::log::warn!("WidgetRegistry missing widget '{}'", id);
            return None;
        };
        Some(widget.show(ui))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WidgetCommand {
    Label {
        id: String,
        text: String,
    },
    Button {
        id: String,
        text: String,
    },
    Checkbox {
        id: String,
        text: String,
        checked: bool,
    },
}

#[cfg(feature = "protocol")]
#[derive(Resource, Clone, Debug)]
pub struct WidgetStreamSettings {
    pub source: DataSource,
}

#[cfg(feature = "protocol")]
impl Default for WidgetStreamSettings {
    fn default() -> Self {
        let source = match std::env::var("DIMENSIFY_WIDGET_SOURCE")
            .unwrap_or_else(|_| "local".to_string())
            .as_str()
        {
            "file" => std::env::var("DIMENSIFY_WIDGET_FILE")
                .ok()
                .map(|path| DataSource::FileReplay { path })
                .unwrap_or(DataSource::Local),
            "db" => std::env::var("DIMENSIFY_WIDGET_DB_ADDR")
                .ok()
                .map(|addr| DataSource::Db { addr })
                .unwrap_or(DataSource::Local),
            _ => DataSource::Local,
        };
        Self { source }
    }
}

#[derive(Resource, Default)]
pub struct WidgetCommandQueue {
    commands: Vec<WidgetCommand>,
}

impl WidgetCommandQueue {
    pub fn push(&mut self, command: WidgetCommand) {
        self.commands.push(command);
    }

    pub fn drain(&mut self) -> Vec<WidgetCommand> {
        self.commands.drain(..).collect()
    }
}

#[derive(Resource, Default)]
pub struct WidgetPanel {
    pub widgets: Vec<String>,
}

pub fn register_demo_widgets(mut queue: ResMut<WidgetCommandQueue>) {
    queue.push(WidgetCommand::Label {
        id: "demo_label".to_string(),
        text: "Dimensify widget registry".to_string(),
    });
    queue.push(WidgetCommand::Button {
        id: "demo_button".to_string(),
        text: "Click me".to_string(),
    });
    queue.push(WidgetCommand::Checkbox {
        id: "demo_checkbox".to_string(),
        text: "Toggle option".to_string(),
        checked: true,
    });
}

#[cfg(feature = "protocol")]
#[cfg(not(target_arch = "wasm32"))]
pub fn load_widget_commands_from_source(
    settings: Res<WidgetStreamSettings>,
    mut queue: ResMut<WidgetCommandQueue>,
) {
    match &settings.source {
        DataSource::FileReplay { path } => {
            let content = match std::fs::read_to_string(path) {
                Ok(content) => content,
                Err(err) => {
                    bevy::log::error!("Failed to read widget file {}: {}", path, err);
                    return;
                }
            };

            for (line_no, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<WidgetCommand>(line) {
                    Ok(command) => queue.push(command),
                    Err(err) => {
                        bevy::log::warn!(
                            "Failed to parse widget command at line {}: {}",
                            line_no + 1,
                            err
                        );
                    }
                }
            }
        }
        DataSource::Db { .. } => {
            bevy::log::warn!("Widget DB source is not implemented yet");
        }
        DataSource::Local => {}
    }
}

pub fn apply_widget_commands(
    mut registry: ResMut<WidgetRegistry>,
    mut queue: ResMut<WidgetCommandQueue>,
    mut panel: ResMut<WidgetPanel>,
) {
    for command in queue.drain() {
        match command {
            WidgetCommand::Label { id, text } => {
                let label_text = text.clone();
                registry.register(id.clone(), move |ui: &mut egui::Ui| {
                    ui.add(ELabel::new(label_text.clone()))
                });
                if !panel.widgets.contains(&id) {
                    panel.widgets.push(id);
                }
            }
            WidgetCommand::Button { id, text } => {
                let button_text = text.clone();
                registry.register(id.clone(), move |ui: &mut egui::Ui| {
                    let response = ui.button(button_text.clone());
                    if response.clicked() {
                        bevy::log::info!("WidgetRegistry button clicked: {}", button_text);
                    }
                    response
                });
                if !panel.widgets.contains(&id) {
                    panel.widgets.push(id);
                }
            }
            WidgetCommand::Checkbox { id, text, checked } => {
                let checkbox_text = text.clone();
                let mut value = checked;
                registry.register(id.clone(), move |ui: &mut egui::Ui| {
                    let response = ui.add(ECheckboxButton::new(checkbox_text.clone(), &mut value));
                    if response.changed() {
                        bevy::log::info!(
                            "WidgetRegistry checkbox changed: {} -> {}",
                            checkbox_text,
                            value
                        );
                    }
                    response
                });
                if !panel.widgets.contains(&id) {
                    panel.widgets.push(id);
                }
            }
        }
    }
}

pub fn widget_registry_demo_ui(
    mut contexts: EguiContexts,
    mut registry: ResMut<WidgetRegistry>,
    panel: Res<WidgetPanel>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Widget Registry").show(ctx, |ui| {
        for id in &panel.widgets {
            if registry.show(id, ui).is_some() {
                ui.add_space(4.0);
            }
        }
    });
}

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

fn shrink_rect(rect: egui::Rect, margin: egui::Margin) -> egui::Rect {
    egui::Rect::from_min_max(
        rect.min + egui::vec2(margin.left.into(), margin.top.into()),
        rect.max - egui::vec2(margin.right.into(), margin.bottom.into()),
    )
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

fn with_opacity(color: egui::Color32, opacity: f32) -> egui::Color32 {
    let alpha = (color.a() as f32 * opacity).clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

#[cfg(test)]
mod tests {
    use super::{
        WidgetCommand, WidgetCommandQueue, WidgetPanel, WidgetRegistry, apply_widget_commands,
    };
    use bevy::prelude::{Schedule, World};

    #[test]
    fn apply_widget_commands_registers_widgets() {
        let mut world = World::new();
        world.insert_resource(WidgetRegistry::default());
        world.insert_resource(WidgetCommandQueue::default());
        world.insert_resource(WidgetPanel::default());

        {
            let mut queue = world.resource_mut::<WidgetCommandQueue>();
            queue.push(WidgetCommand::Label {
                id: "label".to_string(),
                text: "Label".to_string(),
            });
            queue.push(WidgetCommand::Button {
                id: "button".to_string(),
                text: "Button".to_string(),
            });
            queue.push(WidgetCommand::Checkbox {
                id: "checkbox".to_string(),
                text: "Checkbox".to_string(),
                checked: false,
            });
        }

        let mut schedule = Schedule::default();
        schedule.add_systems(apply_widget_commands);
        schedule.run(&mut world);

        let panel = world.resource::<WidgetPanel>();
        assert_eq!(panel.widgets.len(), 3);

        let mut registry = world.resource_mut::<WidgetRegistry>();
        assert!(registry.unregister("label").is_some());
        assert!(registry.unregister("button").is_some());
        assert!(registry.unregister("checkbox").is_some());
    }
}
