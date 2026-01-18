use std::ops::DerefMut;

use bevy::{
    camera::{Viewport, visibility::RenderLayers},
    prelude::*,
    window::PrimaryWindow,
};
use bevy_egui::{
    EguiContext, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use egui_tiles::{self, Tree};

use crate::{layout_kdl, layout_runtime, style, tabs};
use bevy_inspector_egui::DefaultInspectorConfigPlugin;

use tabs::BoxedViewerTab;

/// Setup the dimensify UI plugins, resources, and systems.
pub fn setup_ui(app: &mut App) {
    if !app.is_plugin_added::<bevy_egui::EguiPlugin>() {
        app.add_plugins(EguiPlugin::default());
    }
    if !app.is_plugin_added::<DefaultInspectorConfigPlugin>() {
        app.add_plugins(DefaultInspectorConfigPlugin);
    }
    app.init_resource::<tabs::InspectorSelectionState>()
        .init_resource::<tabs::DockUiState>()
        .init_resource::<tabs::PanelRegistry>()
        .init_resource::<UiPanelVisibility>()
        .init_resource::<PanelLayoutDirty>()
        .init_resource::<crate::pane_widgets::PaneWidgetStates>()
        .init_state::<tabs::DevUiState>()
        .add_systems(PreStartup, no_egui_primary_context)
        .add_systems(Startup, (setup_panel_registry, setup_editor_layout).chain())
        .add_systems(PostStartup, setup_cameras_and_egui_ctx)
        .add_systems(
            PostStartup,
            apply_dimensify_style.after(setup_cameras_and_egui_ctx),
        )
        .add_systems(EguiPrimaryContextPass, update_ui);
}

/// Camera that renders the 3D/2D scene in world space.
#[derive(Component)]
#[require(Name::new("camera_worldspace"))]
pub struct WorldSpaceCamera;

/// Camera that renders the development UI in screen space.
#[derive(Component)]
#[require(Name::new("camera_ui"))]
pub struct UiSpaceCamera;

#[derive(Resource)]
struct ViewportDimensions {
    left_width: f32,
    right_width: f32,
    bottom_height: f32,
}

/// Tracks whether panel sizes have been initialized at least once.
#[derive(Resource, Default)]
struct PanelSizesInitialized(bool);

/// Flags that panel layout trees should be rebuilt on the next UI tick.
#[derive(Resource, Default)]
struct PanelLayoutDirty(bool);

/// Docked panel layout for the left side.
#[derive(Resource)]
pub(crate) struct LeftPanelLayout {
    pub(crate) tree: Option<Tree<BoxedViewerTab>>,
}

/// Docked panel layout for the bottom side.
#[derive(Resource)]
pub(crate) struct BottomPanelLayout {
    pub(crate) tree: Option<Tree<BoxedViewerTab>>,
}

/// Docked panel layout for the right side.
#[derive(Resource)]
pub(crate) struct RightPanelLayout {
    pub(crate) tree: Option<Tree<BoxedViewerTab>>,
}

/// Helper trait to access the tree from a panel layout resource.
trait PanelLayout: Resource {
    fn tree(&mut self) -> Option<&mut Tree<BoxedViewerTab>>;
}

impl PanelLayout for LeftPanelLayout {
    fn tree(&mut self) -> Option<&mut Tree<BoxedViewerTab>> {
        self.tree.as_mut()
    }
}

impl PanelLayout for RightPanelLayout {
    fn tree(&mut self) -> Option<&mut Tree<BoxedViewerTab>> {
        self.tree.as_mut()
    }
}

/// Behavior for the editor layout tiles (tabs, colors, and actions).
struct EditorBehavior<'a> {
    world: &'a mut World,
    simplification: egui_tiles::SimplificationOptions,
    tab_bar_height: f32,
    gap_width: f32,
}

impl<'a> EditorBehavior<'a> {
    fn new(world: &'a mut World) -> Self {
        Self {
            world,
            simplification: egui_tiles::SimplificationOptions {
                all_panes_must_have_tabs: true,
                join_nested_linear_containers: true,
                ..Default::default()
            },
            tab_bar_height: 26.0,
            gap_width: 4.0,
        }
    }
}

impl egui_tiles::Behavior<BoxedViewerTab> for EditorBehavior<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut BoxedViewerTab,
    ) -> egui_tiles::UiResponse {
        pane.ui(ui, self.world);
        egui_tiles::UiResponse::None
    }

    // fn is_tab_closable(&self, _tiles: &egui_tiles::Tiles<BoxedViewerTab>, _tile_id: egui_tiles::TileId) -> bool {
    //     true
    // }

    fn tab_title_for_pane(&mut self, pane: &BoxedViewerTab) -> egui::WidgetText {
        pane.title().into()
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        self.tab_bar_height
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        self.gap_width
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> egui::Color32 {
        style::TAB_BAR_COLOR
    }

    fn tab_bg_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &egui_tiles::Tiles<BoxedViewerTab>,
        _tile_id: egui_tiles::TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        if state.active {
            style::PANEL_COLOR
        } else {
            style::TAB_BAR_COLOR
        }
    }

    fn tab_text_color(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &egui_tiles::Tiles<BoxedViewerTab>,
        _tile_id: egui_tiles::TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        if state.active {
            style::TEXT_COLOR
        } else {
            style::TEXT_SUBDUED
        }
    }

    fn tab_outline_stroke(
        &self,
        _visuals: &egui::Visuals,
        _tiles: &egui_tiles::Tiles<BoxedViewerTab>,
        _tile_id: egui_tiles::TileId,
        _state: &egui_tiles::TabState,
    ) -> egui::Stroke {
        egui::Stroke::new(1.0, style::BORDER_COLOR)
    }

    fn tab_title_spacing(&self, _visuals: &egui::Visuals) -> f32 {
        10.0
    }

    fn top_bar_right_ui(
        &mut self,
        tiles: &egui_tiles::Tiles<BoxedViewerTab>,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        tabs: &egui_tiles::Tabs,
        _scroll_offset: &mut f32,
    ) {
        ui.add_space(style::SPACING_EDGE);
        let Some(active) = tabs.active else {
            return;
        };
        let Some(egui_tiles::Tile::Pane(pane)) = tiles.get(active) else {
            return;
        };
        let title = pane.title();
        let mut should_toggle = false;
        let mut floating = false;
        self.world
            .resource_scope(|_world, registry: Mut<tabs::PanelRegistry>| {
                if registry.is_enabled(title) {
                    floating = registry.is_floating(title);
                }
            });

        ui.spacing_mut().item_spacing.x = 6.0;
        let mut next_state = floating;
        let response =
            medium_icon_toggle_button(ui, style::icon_popout(), "Pop out", &mut next_state);
        if response.clicked() && next_state != floating {
            should_toggle = true;
            floating = next_state;
        }

        if should_toggle {
            self.world
                .resource_scope(|_world, mut registry: Mut<tabs::PanelRegistry>| {
                    registry.set_floating(title, floating);
                });
            if let Some(mut dirty) = self.world.get_resource_mut::<PanelLayoutDirty>() {
                dirty.0 = true;
            }
        }
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        self.simplification
    }
}

fn setup_editor_layout(mut commands: Commands, registry: Res<tabs::PanelRegistry>) {
    let layout_path = layout_kdl::resolve_layout_path();
    let layout = layout_kdl::load_layout_from_path(&layout_path);
    layout_kdl::apply_layout_from_startup(&mut commands, layout, &registry);
    commands.insert_resource(layout_kdl::DevUiLayoutPath(layout_path));
    commands.insert_resource(ViewportDimensions {
        left_width: 0.0,
        right_width: 0.0,
        bottom_height: 0.0,
    });
}

fn setup_panel_registry(mut registry: ResMut<tabs::PanelRegistry>) {
    tabs::register_default_panels(&mut registry);
}

#[derive(Resource)]
struct UiPanelVisibility {
    show_left: bool,
    show_right: bool,
    show_bottom: bool,
}

impl Default for UiPanelVisibility {
    fn default() -> Self {
        // TODO: restore from system state
        Self {
            show_left: true,
            show_right: false,
            show_bottom: false,
        }
    }
}

/// Disable automatic creation of primary context - we'll set it up manually
/// As we don't want the primary context to be created automatically on the first camera.
fn no_egui_primary_context(mut egui_global_settings: ResMut<EguiGlobalSettings>) {
    egui_global_settings.auto_create_primary_context = false;
}

fn apply_dimensify_style(mut ctx: Single<&mut EguiContext, With<PrimaryEguiContext>>) {
    style::apply_dimensify_style(&mut ctx.get_mut());
}
fn setup_cameras_and_egui_ctx(
    mut commands: Commands,
    mut q_worldspace_camera: Query<(Entity, &mut Camera, Option<&WorldSpaceCamera>)>,
    // mut q_worldspace_camera: Query<Entity, (With<WorldSpaceCamera>, With<Camera>)>,
) {
    // Disable automatic creation of primary context - we'll set it up manually
    // egui_global_settings.auto_create_primary_context = false;

    // We support auto using the first camera as the worldspace camera if no camera is added with component WorldSpaceCamera.
    // If multiple cameras are found, there should be exactly one camera with component `WorldSpaceCamera`.

    if q_worldspace_camera.is_empty() {
        panic!(
            "No worldspace camera found. Have you added any camera before this system is called?"
        );
    }

    // count how many worldspace cameras with component WorldSpaceCamera
    let worldspace_camera_count = q_worldspace_camera
        .iter()
        .filter(|(_, _, worldspace_camera_component)| worldspace_camera_component.is_some())
        .count();
    let (camera_entity, mut worldspace_camera) = if worldspace_camera_count > 1 {
        panic!(
            "Multiple worldspace camera with component WorldSpaceCamera found. Only one is allowed."
        );
    } else if worldspace_camera_count == 1 {
        // iterate through q_worldspace_camera and find the one with component WorldSpaceCamera (with iterator)
        let (camera_entity, worldspace_camera, _) = q_worldspace_camera
            .iter_mut()
            .find(|(_, _, worldspace_camera_component)| worldspace_camera_component.is_some())
            .expect("this should not happen");
        (camera_entity, worldspace_camera)
    } else {
        // no component WorldSpaceCamera found, use the first camera
        if q_worldspace_camera.iter().count() > 1 {
            panic!(
                "Multiple worldspace camera found. If you need more than one camera, please add the `WorldSpaceCamera` component to the camera you want to use for the worldspace camera."
            );
        }
        let (camera_entity, worldspace_camera, _) = q_worldspace_camera
            .single_mut()
            .expect("this should not happen");
        (camera_entity, worldspace_camera)
    };

    // Egui camera - renders UI on top
    commands.spawn((
        UiSpaceCamera,
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));

    // 3D world camera - renders to viewport
    commands.entity(camera_entity).insert(WorldSpaceCamera);
    if worldspace_camera.viewport.is_none() {
        // always create a default viewport for the worldspace camera
        worldspace_camera.viewport = Some(Viewport {
            physical_position: UVec2::new(0, 0),
            physical_size: UVec2::new(200, 200),
            ..default()
        });
    }
}

/// Update the UI.
///
/// TODO: investigate if we should avoid using mut World for performance reasons.
fn update_ui(world: &mut World) -> Result {
    let Ok(mut egui_context) = world
        .query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>()
        .single_mut(world)
    else {
        return Ok(());
    };
    let mut egui_context = egui_context.deref_mut().clone();
    let ctx = egui_context.get_mut();

    let (scale_factor, window_width, window_height) = {
        let window = world
            .query_filtered::<&Window, With<PrimaryWindow>>()
            .single(world)?;
        (
            window.scale_factor(),
            window.physical_width(),
            window.physical_height(),
        )
    };

    // Note: Panel sizes are set via default_width/default_height in the panel builders below
    // The panel_sizes_initialized flag is kept for potential future use

    // let mut save_layout = false;
    // let mut reload_layout = false;
    let mut layout_save_reload = (false, false);
    // let mut panel_toggles_changed = false;
    let mut panel_toggles = Vec::new();
    let mut panel_enabled = std::collections::HashMap::new();
    let mut panel_floating = std::collections::HashMap::new();
    let mut floating_titles = Vec::new();

    {
        let registry = world.resource::<tabs::PanelRegistry>();
        for entry in registry.entries() {
            let enabled = registry.is_enabled(entry.title);
            let floating = registry.is_floating(entry.title);
            panel_enabled.insert(entry.title.to_string(), enabled);
            panel_floating.insert(entry.title.to_string(), floating);
            panel_toggles.push((entry.title.to_string(), entry.location, enabled, floating));
            if floating {
                floating_titles.push(entry.title.to_string());
            }
        }
    }

    let (top_height, panel_toggles_changed) =
        menu_bar(ctx, world, &mut layout_save_reload, &mut panel_toggles);

    let (show_left, show_right, show_bottom) = {
        let panels = world.resource::<UiPanelVisibility>();
        (panels.show_left, panels.show_right, panels.show_bottom)
    };
    let (_has_left, _has_right, _has_bottom) = {
        let registry = world.resource::<tabs::PanelRegistry>();
        let mut left = false;
        let mut right = false;
        let mut bottom = false;
        for entry in registry.entries() {
            if !registry.is_enabled(entry.title) || registry.is_floating(entry.title) {
                continue;
            }
            match entry.location {
                tabs::PanelLocation::Left => left = true,
                tabs::PanelLocation::Right => right = true,
                tabs::PanelLocation::Bottom => bottom = true,
            }
        }
        (left, right, bottom)
    };
    // let show_left = show_left && has_left;
    // let show_right = show_right && has_right;
    // let show_bottom = show_bottom && has_bottom;

    // update panel registry
    if panel_toggles_changed {
        let registry_snapshot =
            world.resource_scope(|_world, mut registry: Mut<tabs::PanelRegistry>| {
                for (title, _location, enabled, floating) in &panel_toggles {
                    let prev_enabled = panel_enabled.get(title).copied().unwrap_or(false);
                    if prev_enabled != *enabled {
                        registry.set_enabled(title, *enabled);
                    }
                    let prev_floating = panel_floating.get(title).copied().unwrap_or(false);
                    if prev_floating != *floating {
                        registry.set_floating(title, *floating);
                    }
                }
                registry.clone()
            });
        layout_runtime::rebuild_viewer_layouts(world, &registry_snapshot);
    }

    if world.resource::<PanelLayoutDirty>().0 {
        let registry_snapshot = world.resource::<tabs::PanelRegistry>().clone();
        layout_runtime::rebuild_viewer_layouts(world, &registry_snapshot);
        world.resource_mut::<PanelLayoutDirty>().0 = false;
    }
    // control floating panels visibility
    if !floating_titles.is_empty() {
        let mut closed = Vec::new();
        for title in &floating_titles {
            let mut open = true;
            egui::Window::new(title)
                .frame(panel_frame(style::PANEL_COLOR))
                .open(&mut open)
                .show(ctx, |ui| {
                    if let Some(tab) = world.resource::<tabs::PanelRegistry>().create_tab(title) {
                        tab.ui(ui, world);
                    } else {
                        ui.label("Panel unavailable");
                    }
                });
            if !open {
                closed.push(title.clone());
            }
        }
        if !closed.is_empty() {
            let registry_snapshot =
                world.resource_scope(|_world, mut registry: Mut<tabs::PanelRegistry>| {
                    for title in &closed {
                        registry.set_floating(title, false);
                    }
                    registry.clone()
                });
            layout_runtime::rebuild_viewer_layouts(world, &registry_snapshot);
        }
    }

    // Left panel with tabs - set default width only on first frame
    // let panel_sizes_initialized = world.resource::<PanelSizesInitialized>().0;
    let panel_sizes_initialized = true;

    let left_width = side_panel_setup::<LeftPanelLayout>(
        ctx,
        world,
        egui::SidePanel::left("left_panel"),
        show_left,
        220.0,
    );
    let right_width = side_panel_setup::<RightPanelLayout>(
        ctx,
        world,
        egui::SidePanel::right("right_panel"),
        show_right,
        240.0,
    );

    // Bottom panel with tabs - set default height only on first frame
    let bottom_height = {
        let bottom_panel = egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .frame(panel_frame(style::BOTTOM_BAR_COLOR));
        let bottom_panel = if !panel_sizes_initialized {
            bottom_panel.default_height(160.0)
        } else {
            bottom_panel
        };
        world.resource_scope(|world, mut bottom_layout: Mut<BottomPanelLayout>| {
            bottom_panel
                .show_animated(ctx, show_bottom, |ui| {
                    if let Some(tree) = &mut bottom_layout.tree {
                        let mut behavior = EditorBehavior::new(world);
                        tree.ui(&mut behavior, ui);
                    }
                })
                .map(|response| response.response.rect.height())
                .unwrap_or(0.0)
        })
    };

    // Mark as initialized after first frame
    if !panel_sizes_initialized {
        world.resource_mut::<PanelSizesInitialized>().0 = true;
    }

    // Update viewport dimensions
    {
        let mut viewport_dims = world.resource_mut::<ViewportDimensions>();
        viewport_dims.left_width = left_width;
        viewport_dims.right_width = right_width;
        viewport_dims.bottom_height = bottom_height;
    }

    // Calculate viewport in physical pixels based on side panels.
    let left_physical = (left_width * scale_factor).round().max(0.0) as u32;
    let top_physical = (top_height * scale_factor).round().max(0.0) as u32;
    let bottom_physical = (bottom_height * scale_factor).round().max(0.0) as u32;

    const VIEWPORT_LEFT_PADDING: u32 = 0;
    const VIEWPORT_BOTTOM_PADDING: u32 = 0;

    let pos = UVec2::new(
        left_physical.min(window_width.saturating_sub(1)) + VIEWPORT_LEFT_PADDING,
        top_physical.min(window_height.saturating_sub(1)),
    );

    let available_width = window_width.saturating_sub(pos.x);
    let available_height = window_height.saturating_sub(pos.y + VIEWPORT_BOTTOM_PADDING);
    let size = UVec2::new(
        available_width.saturating_sub(0),
        available_height.saturating_sub(bottom_physical).max(1),
    );

    let mut camera = world
        .query_filtered::<&mut Camera, (
            With<WorldSpaceCamera>,
            Without<PrimaryEguiContext>,
            Without<Camera2d>,
        )>()
        .single_mut(world)?;

    // check if the viewport dimensions have changed. If not, early return.
    if let Some(viewport) = &mut camera.viewport {
        if viewport.physical_size == size {
            return Ok(());
        }

        // Update camera viewport
        viewport.physical_position = pos;
        viewport.physical_size = size;
    }

    if layout_save_reload.0 {
        layout_kdl::save_current_layout(world);
    }
    if layout_save_reload.1 {
        layout_kdl::reload_layout(world);
    }

    Ok(())
}

fn menu_bar(
    ctx: &mut egui::Context,
    world: &mut World,
    layout_save_reload: &mut (bool, bool),
    panel_toggles: &mut Vec<(String, tabs::PanelLocation, bool, bool)>,
) -> (f32, bool) {
    let mut panel_toggles_changed = false;
    // Menu bar
    let top_frame = egui::Frame::NONE
        .fill(style::TOP_BAR_COLOR)
        .stroke(egui::Stroke::new(1.0, style::BORDER_COLOR));
    let top_height = egui::TopBottomPanel::top("top_panel")
        .frame(top_frame)
        .show(ctx, |ui| {
            use egui::containers::PopupCloseBehavior;
            let menu_bar = egui::MenuBar::new().config(
                egui::containers::menu::MenuConfig::default()
                    .close_behavior(PopupCloseBehavior::CloseOnClickOutside),
            );

            menu_bar.ui(ui, |ui| {
                ui.add_space(style::SPACING_EDGE);
                ui.set_height(28.0);
                let mut panels = world.resource_mut::<UiPanelVisibility>();
                ui.horizontal(|ui| {
                    ui.add_space(style::SPACING_EDGE);
                    // let logo = egui::Image::new(style::ICON_LOGO)
                    //     .fit_to_exact_size(egui::vec2(92.0, 16.0))
                    //     .tint(style::TEXT_COLOR);
                    // ui.add(logo);
                    ui.label(
                        egui::RichText::new("Dimensify")
                            .color(style::TEXT_COLOR)
                            .monospace()
                            .small_raised(),
                    );
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
                    // display all toggles for each location in a submenu
                    ui.menu_button("View", |ui| {
                        // ui.label("Tabs");
                        ui.menu_button("By panel location", |ui| {
                            for location in [
                                tabs::PanelLocation::Left,
                                tabs::PanelLocation::Right,
                                tabs::PanelLocation::Bottom,
                            ] {
                                let label = match location {
                                    tabs::PanelLocation::Left => "Left",
                                    tabs::PanelLocation::Right => "Right",
                                    tabs::PanelLocation::Bottom => "Bottom",
                                };

                                ui.menu_button(label, |ui| {
                                    for toggle in panel_toggles
                                        .iter_mut()
                                        .filter(|(_, loc, _, _)| *loc == location)
                                    {
                                        let mut enabled = toggle.2;
                                        let label = toggle.0.as_str();
                                        let mut changed = false;
                                        // ui.horizontal(|ui| {
                                        if ui.checkbox(&mut enabled, label).changed() {
                                            changed = true;
                                        }
                                        if changed {
                                            panel_toggles_changed = true;
                                        }
                                        toggle.2 = enabled;
                                    }
                                });
                            }
                        });
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
                        ui.add_space(style::SPACING_EDGE);
                        ui.spacing_mut().item_spacing.x = style::SPACING_ITEM_SPACING;
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("LOCAL")
                                    .color(style::TEXT_SUBDUED)
                                    .size(10.0),
                            )
                            .selectable(false),
                        );
                        medium_icon_toggle_button(
                            ui,
                            style::icon_bottom_panel(),
                            "Bottom panel",
                            &mut panels.show_bottom,
                        );
                        medium_icon_toggle_button(
                            ui,
                            style::icon_right_panel(),
                            "Right panel",
                            &mut panels.show_right,
                        );
                        medium_icon_toggle_button(
                            ui,
                            style::icon_left_panel(),
                            "Left panel",
                            &mut panels.show_left,
                        );
                    },
                );
            });
        })
        .response
        .rect
        .height();
    (top_height, panel_toggles_changed)
}

/// Setup a side panel with a given layout.
/// Works with both left and right side panels.
fn side_panel_setup<P: PanelLayout>(
    ctx: &mut egui::Context,
    world: &mut World,
    panel: egui::SidePanel,
    display_panel: bool,
    default_width: f32,
) -> f32 {
    let panel = panel
        .resizable(true)
        .frame(panel_frame(style::PANEL_COLOR))
        .default_width(default_width);

    world.resource_scope(|world, mut layout: Mut<P>| {
        panel
            .show_animated(ctx, display_panel, |ui| {
                if let Some(tree) = &mut layout.tree() {
                    let mut behavior = EditorBehavior::new(world);
                    tree.ui(&mut behavior, ui);
                }
            })
            .map(|response| response.response.rect.width())
            .unwrap_or(0.0)
    })
}

/// Panel toggle button with a medium size icon.
fn medium_icon_toggle_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    alt_text: &str,
    selected: &mut bool,
) -> egui::Response {
    let size_points = egui::Vec2::splat(16.0);
    let tint = if *selected {
        style::TEXT_COLOR
    } else {
        style::TEXT_SUBDUED
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

fn panel_frame(fill: egui::Color32) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, style::BORDER_COLOR))
}
