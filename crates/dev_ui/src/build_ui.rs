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

use crate::{layout_kdl, tabs};
use bevy_inspector_egui::DefaultInspectorConfigPlugin;

/// Setup the development UI.
/// This function adds the necessary plugins and systems to the app to allow the development UI to be displayed.
pub fn setup_ui(app: &mut App) {
    if !app.is_plugin_added::<bevy_egui::EguiPlugin>() {
        app.add_plugins(EguiPlugin::default());
    }
    if !app.is_plugin_added::<DefaultInspectorConfigPlugin>() {
        app.add_plugins(DefaultInspectorConfigPlugin);
    }
    app.init_resource::<tabs::InspectorSelectionState>()
        .init_resource::<tabs::DockUiState>()
        .init_resource::<DevUiPanelVisibility>()
        .init_resource::<crate::pane_widgets::PaneWidgetStates>()
        .init_state::<tabs::DevUiState>()
        .register_type::<tabs::DevUiState>()
        .register_type::<Name>()
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .add_systems(PreStartup, no_egui_primary_context)
        .add_systems(Startup, setup_editor_layout)
        .add_systems(PostStartup, setup_cameras)
        .add_systems(PostStartup, setup_panel_sizes)
        .add_systems(EguiPrimaryContextPass, update_ui);
}

/// A 3D camera that renders the scene in the worldspace.
#[derive(Component)]
pub struct WorldSpaceCamera;

/// A 2D camera that renders the development UI in the UI space.
#[derive(Component)]
pub struct UiSpaceCamera;

#[derive(Resource)]
struct ViewportDimensions {
    left_width: f32,
    right_width: f32,
    bottom_height: f32,
}

#[derive(Resource, Default)]
struct PanelSizesInitialized(bool);

use tabs::BoxedViewerTab;

#[derive(Resource)]
pub(crate) struct LeftPanelLayout {
    pub(crate) tree: Tree<BoxedViewerTab>,
}

#[derive(Resource)]
pub(crate) struct BottomPanelLayout {
    pub(crate) tree: Tree<BoxedViewerTab>,
}

#[derive(Resource)]
pub(crate) struct RightPanelLayout {
    pub(crate) tree: Tree<BoxedViewerTab>,
}

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

    fn tab_title_for_pane(&mut self, pane: &BoxedViewerTab) -> egui::WidgetText {
        pane.title().into()
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        self.tab_bar_height
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        self.gap_width
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        self.simplification
    }
}

fn setup_editor_layout(mut commands: Commands) {
    let layout_path = layout_kdl::resolve_layout_path();
    let layout = layout_kdl::load_layout_from_path(&layout_path);
    layout_kdl::apply_layout_from_startup(&mut commands, layout);
    commands.insert_resource(layout_kdl::DevUiLayoutPath(layout_path));
    commands.insert_resource(ViewportDimensions {
        left_width: 0.0,
        right_width: 0.0,
        bottom_height: 0.0,
    });
}

#[derive(Resource)]
struct DevUiPanelVisibility {
    show_left: bool,
    show_right: bool,
    show_bottom: bool,
}

impl Default for DevUiPanelVisibility {
    fn default() -> Self {
        Self {
            show_left: true,
            show_right: true,
            show_bottom: true,
        }
    }
}

/// Disable automatic creation of primary context - we'll set it up manually
/// As we don't want the primary context to be created automatically on the first camera.
fn no_egui_primary_context(mut egui_global_settings: ResMut<EguiGlobalSettings>) {
    egui_global_settings.auto_create_primary_context = false;
}

fn setup_cameras(
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

fn setup_panel_sizes(mut commands: Commands) {
    // Initialize the flag resource - sizes will be set on first frame
    commands.insert_resource(PanelSizesInitialized(false));
}

#[allow(clippy::type_complexity)]
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

    let mut save_layout = false;
    let mut reload_layout = false;

    // Menu bar
    let top_height = egui::TopBottomPanel::top("top_panel")
        .show(ctx, |ui| {
            use egui::containers::PopupCloseBehavior;
            let menu_bar = egui::MenuBar::new().config(
                egui::containers::menu::MenuConfig::default()
                    .close_behavior(PopupCloseBehavior::CloseOnClickOutside),
            );

            menu_bar.ui(ui, |ui| {
                let mut panels = world.resource_mut::<DevUiPanelVisibility>();
                ui.menu_button("File", |ui| {
                    if ui.button("Open Project…").clicked() {
                        ui.close();
                    }
                    if ui.button("Save Layout").clicked() {
                        save_layout = true;
                        ui.close();
                    }
                    if ui.button("Reload Layout").clicked() {
                        reload_layout = true;
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut panels.show_left, "Left panel");
                    ui.checkbox(&mut panels.show_right, "Right panel");
                    ui.checkbox(&mut panels.show_bottom, "Bottom panel");
                });
                ui.menu_button("Tools", |ui| {
                    ui.label("Simulation controls coming soon");
                });
            });
        })
        .response
        .rect
        .height();

    let (show_left, show_right, show_bottom) = {
        let panels = world.resource::<DevUiPanelVisibility>();
        (panels.show_left, panels.show_right, panels.show_bottom)
    };

    // Left panel with tabs - set default width only on first frame
    let panel_sizes_initialized = world.resource::<PanelSizesInitialized>().0;

    let mut left_width = 0.0;
    if show_left {
        let left_panel = egui::SidePanel::left("left_panel").resizable(true);
        let left_panel = if !panel_sizes_initialized {
            left_panel.default_width(220.0)
        } else {
            left_panel
        };
        left_width = world.resource_scope(|world, mut left_layout: Mut<LeftPanelLayout>| {
            left_panel
                .show(ctx, |ui| {
                    let mut behavior = EditorBehavior::new(world);
                    left_layout.tree.ui(&mut behavior, ui);
                })
                .response
                .rect
                .width()
        });
    }

    // Bottom panel with tabs - set default height only on first frame
    let mut bottom_height = 0.0;
    if show_bottom {
        let bottom_panel = egui::TopBottomPanel::bottom("bottom_panel").resizable(true);
        let bottom_panel = if !panel_sizes_initialized {
            bottom_panel.default_height(160.0)
        } else {
            bottom_panel
        };
        bottom_height = world.resource_scope(|world, mut bottom_layout: Mut<BottomPanelLayout>| {
            bottom_panel
                .show(ctx, |ui| {
                    let mut behavior = EditorBehavior::new(world);
                    bottom_layout.tree.ui(&mut behavior, ui);
                })
                .response
                .rect
                .height()
        });
    }

    let mut right_width = 0.0;
    if show_right {
        let right_panel = egui::SidePanel::right("right_panel").resizable(true);
        let right_panel = if !panel_sizes_initialized {
            right_panel.default_width(240.0)
        } else {
            right_panel
        };
        right_width = world.resource_scope(|world, mut right_layout: Mut<RightPanelLayout>| {
            right_panel
                .show(ctx, |ui| {
                    let mut behavior = EditorBehavior::new(world);
                    right_layout.tree.ui(&mut behavior, ui);
                })
                .response
                .rect
                .width()
        });
    }

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
            // panic!("Viewport dimensions have not changed");
            return Ok(());
        }

        // Update camera viewport
        viewport.physical_position = pos;
        viewport.physical_size = size;
    }

    if save_layout {
        layout_kdl::save_current_layout(world);
    }
    if reload_layout {
        layout_kdl::reload_layout(world);
    }

    Ok(())
}
