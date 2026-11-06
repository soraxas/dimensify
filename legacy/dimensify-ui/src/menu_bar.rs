use crate::{
    style::{self, Theme},
    utils::medium_icon_toggle_button,
    widget,
};
use bevy::prelude::*;
use bevy_egui::egui;
use egui_extras::{Column, TableBuilder};

/// One row in the View → Panels menu, describing the state of a single widget.
#[derive(Clone, Debug)]
pub struct PanelToggle {
    pub title: String,
    pub location: widget::PanelLocation,
    pub enabled: bool,
    pub floating: bool,
}

/// UI state for the panel management window.
#[derive(Resource, Default)]
pub struct PanelManagerUiState {
    pub open: bool,
}

#[derive(Resource)]
pub struct UiMenuBarBuilder(pub Box<dyn UiMenuBarBuilderTrait + Send + Sync>);

impl Default for UiMenuBarBuilder {
    fn default() -> Self {
        Self(Box::new(DefaultUiMenuBarBuilder))
    }
}

pub trait UiMenuBarBuilderTrait: Send + Sync {
    // Build the logo.
    fn build_logo(&self, ui: &mut egui::Ui, theme: &Theme) -> egui::Response {
        ui.label(
            egui::RichText::new("Dimensify")
                .color(theme.text_color)
                .monospace()
                .small_raised(),
        )
    }

    // The main build function.
    fn build(
        &self,
        ctx: &mut egui::Context,
        world: &mut World,
        layout_save_reload: &mut (bool, bool),
        panel_toggles: &mut [PanelToggle],
    ) -> (f32, bool) {
        world.resource_scope(|world, theme: Mut<Theme>| {
            let mut panel_toggles_changed = false;
            let mut should_open_panels_window = false;
            // Menu bar
            let top_frame = egui::Frame::NONE
                .fill(theme.top_bar_color)
                .stroke(egui::Stroke::new(1.0, theme.border_color));
            let top_height = egui::TopBottomPanel::top("top_panel")
                .frame(top_frame)
                .show(ctx, |ui| {
                    use egui::containers::PopupCloseBehavior;
                    let menu_bar = egui::MenuBar::new().config(
                        egui::containers::menu::MenuConfig::default()
                            .close_behavior(PopupCloseBehavior::CloseOnClickOutside),
                    );

                    menu_bar.ui(ui, |ui| {
                        ui.add_space(theme.spacing_edge);
                        ui.set_height(28.0);
                        ui.horizontal(|ui| {
                            ui.add_space(theme.spacing_edge);
                            self.build_logo(ui, theme.as_ref());
                            ui.separator();
                            ui.menu_button("File", |ui| {
                                if ui.button("Open Project…").clicked() {
                                    ui.close();
                                }
                                if ui.button("Save Layout").clicked() {
                                    layout_save_reload.0 = true;
                                    ui.close();
                                }
                                if ui.button("Reload Layout").clicked() {
                                    layout_save_reload.1 = true;
                                    ui.close();
                                }
                            });
                            ui.menu_button("View", |ui| {
                                if ui.button("⚙️Widgets Management").clicked() {
                                    should_open_panels_window = true;
                                    ui.close();
                                }
                            });
                            ui.menu_button("Tools", |ui| {
                                ui.label("Simulation controls coming soon");
                            });
                        });

                        let available = ui.available_size_before_wrap();
                        ui.allocate_ui_with_layout(
                            available,
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.add_space(theme.spacing_edge);
                                ui.spacing_mut().item_spacing.x = theme.spacing_item_spacing;
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("LOCAL")
                                            .color(theme.text_subdued_color)
                                            .size(10.0),
                                    )
                                    .selectable(false),
                                );
                                let mut panels = world.resource_mut::<UiPanelVisibility>();
                                medium_icon_toggle_button(
                                    ui,
                                    style::icon_bottom_panel(),
                                    "Bottom panel",
                                    &mut panels.show_bottom,
                                    &theme,
                                );
                                medium_icon_toggle_button(
                                    ui,
                                    style::icon_right_panel(),
                                    "Right panel",
                                    &mut panels.show_right,
                                    &theme,
                                );
                                medium_icon_toggle_button(
                                    ui,
                                    style::icon_left_panel(),
                                    "Left panel",
                                    &mut panels.show_left,
                                    &theme,
                                );
                            },
                        );
                    });
                })
                .response
                .rect
                .height();

            if should_open_panels_window {
                world
                    .get_resource_or_insert_with::<PanelManagerUiState>(Default::default)
                    .open = true;
            }

            // Dedicated panel management window. Use a table so columns align nicely.
            let mut open = world
                .get_resource_or_insert_with::<PanelManagerUiState>(Default::default)
                .open;

            widget_management_window(
                ctx,
                theme.as_ref(),
                panel_toggles,
                &mut open,
                &mut panel_toggles_changed,
            );

            world
                .get_resource_or_insert_with::<PanelManagerUiState>(Default::default)
                .open = open;

            (top_height, panel_toggles_changed)
        })
    }
}

pub struct DefaultUiMenuBarBuilder;

impl UiMenuBarBuilderTrait for DefaultUiMenuBarBuilder {}

fn widget_management_window(
    ctx: &mut egui::Context,
    theme: &Theme,
    panel_toggles: &mut [PanelToggle],
    open: &mut bool,
    panel_toggles_changed: &mut bool,
) {
    egui::Window::new("Widgets Management")
        .open(open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Manage panels: enable/disable, dock side, and pop-out.")
                    .color(theme.text_subdued_color),
            );
            ui.add_space(6.0);

            // Sort by title for discoverability, but edit the original slice.
            let mut indices: Vec<usize> = (0..panel_toggles.len()).collect();
            indices.sort_by_key(|&i| panel_toggles[i].title.clone());

            let row_height = 24.0;
            TableBuilder::new(ui)
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(26.0)) // enabled checkbox
                .column(Column::remainder()) // title
                .column(Column::exact(110.0)) // dock side icons
                .column(Column::exact(70.0)) // floating toggle
                .header(row_height, |mut header| {
                    header.col(|ui| {
                        ui.strong("On");
                    });
                    header.col(|ui| {
                        ui.strong("Panel");
                    });
                    header.col(|ui| {
                        ui.strong("Dock");
                    });
                    header.col(|ui| {
                        ui.strong("Pop out");
                    });
                })
                .body(|mut body| {
                    for i in indices {
                        body.row(row_height, |mut row| {
                            let toggle = &mut panel_toggles[i];

                            row.col(|ui| {
                                let mut enabled = toggle.enabled;
                                if ui.checkbox(&mut enabled, "").changed() {
                                    toggle.enabled = enabled;
                                    *panel_toggles_changed = true;
                                }
                            });

                            row.col(|ui| {
                                ui.label(&toggle.title);
                            });

                            row.col(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;

                                fn dock_icon_button(
                                    ui: &mut egui::Ui,
                                    icon: egui::ImageSource<'static>,
                                    selected: bool,
                                    selected_fill: egui::Color32,
                                    selected_stroke: egui::Stroke,
                                    tooltip: &str,
                                ) -> egui::Response {
                                    let img = egui::Image::new(icon)
                                        .fit_to_exact_size(egui::vec2(16.0, 16.0));
                                    egui::Frame::NONE
                                        .fill(if selected {
                                            selected_fill
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .stroke(if selected {
                                            selected_stroke
                                        } else {
                                            egui::Stroke::NONE
                                        })
                                        .corner_radius(egui::CornerRadius::same(3))
                                        .inner_margin(egui::Margin::same(2))
                                        .show(ui, |ui| ui.add(egui::Button::image(img)))
                                        .inner
                                        .on_hover_text(tooltip)
                                }

                                let mut location = toggle.location;
                                let selected_fill = theme.tab_bar_color;
                                let selected_stroke = egui::Stroke::new(1.0, theme.border_color);
                                if dock_icon_button(
                                    ui,
                                    style::icon_left_panel(),
                                    location == widget::PanelLocation::Left,
                                    selected_fill,
                                    selected_stroke,
                                    "Dock left",
                                )
                                .clicked()
                                {
                                    location = widget::PanelLocation::Left;
                                    // Docking implies we are no longer floating.
                                    if toggle.floating {
                                        toggle.floating = false;
                                        *panel_toggles_changed = true;
                                    }
                                }
                                if dock_icon_button(
                                    ui,
                                    style::icon_right_panel(),
                                    location == widget::PanelLocation::Right,
                                    selected_fill,
                                    selected_stroke,
                                    "Dock right",
                                )
                                .clicked()
                                {
                                    location = widget::PanelLocation::Right;
                                    if toggle.floating {
                                        toggle.floating = false;
                                        *panel_toggles_changed = true;
                                    }
                                }
                                if dock_icon_button(
                                    ui,
                                    style::icon_bottom_panel(),
                                    location == widget::PanelLocation::Bottom,
                                    selected_fill,
                                    selected_stroke,
                                    "Dock bottom",
                                )
                                .clicked()
                                {
                                    location = widget::PanelLocation::Bottom;
                                    if toggle.floating {
                                        toggle.floating = false;
                                        *panel_toggles_changed = true;
                                    }
                                }

                                if location != toggle.location {
                                    toggle.location = location;
                                    *panel_toggles_changed = true;
                                }
                            });

                            row.col(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                let img = egui::Image::new(style::icon_popout())
                                    .fit_to_exact_size(egui::vec2(16.0, 16.0));
                                let response = egui::Frame::NONE
                                    .fill(if toggle.floating {
                                        theme.tab_bar_color
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    })
                                    .stroke(if toggle.floating {
                                        egui::Stroke::new(1.0, theme.border_color)
                                    } else {
                                        egui::Stroke::NONE
                                    })
                                    .corner_radius(egui::CornerRadius::same(3))
                                    .inner_margin(egui::Margin::same(2))
                                    .show(ui, |ui| ui.add(egui::Button::image(img)))
                                    .inner
                                    .on_hover_text("Pop out / dock");
                                if response.clicked() {
                                    // Treat \"floating\" as a fourth placement option:
                                    // when enabling, move the panel to Floating; when
                                    // disabling, keep the last dock side in `location`.
                                    if toggle.floating {
                                        toggle.floating = false;
                                    } else {
                                        toggle.floating = true;
                                        toggle.location = widget::PanelLocation::Floating;
                                    }
                                    *panel_toggles_changed = true;
                                }
                            });
                        });
                    }
                });
        });
}

/// Tracks the visibility of the left, right, and bottom panels.
#[derive(Resource)]
pub struct UiPanelVisibility {
    pub show_left: bool,
    pub show_right: bool,
    pub show_bottom: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for UiPanelVisibility {
    fn default() -> Self {
        // TODO: restore from system state
        Self {
            show_left: false,
            show_right: false,
            show_bottom: false,
        }
    }
}
