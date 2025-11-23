use bevy::{
    camera::{Viewport, visibility::RenderLayers},
    prelude::*,
    window::PrimaryWindow,
};
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use egui_tiles::{self, Tiles, Tree};

use crate::tabs;

fn default_left_tabs() -> Vec<BoxedViewerTab> {
    vec![
        Box::new(tabs::HierarchyTab),
        Box::new(tabs::InspectorTab),
        Box::new(tabs::AssetsTab),
    ]
}

fn default_bottom_tabs() -> Vec<BoxedViewerTab> {
    vec![
        Box::new(tabs::ConsoleTab),
        Box::new(tabs::DiagnosticsTab),
        Box::new(tabs::TasksTab),
    ]
}

/// Setup the development UI.
/// This function adds the necessary plugins and systems to the app to allow the development UI to be displayed.
pub fn setup_dev_ui(app: &mut App) {
    app.add_plugins(EguiPlugin::default())
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
    bottom_height: f32,
}

#[derive(Resource, Default)]
struct PanelSizesInitialized(bool);

use tabs::ViewerTab;
type BoxedViewerTab = Box<dyn ViewerTab>;

#[derive(Resource)]
struct LeftPanelLayout {
    tree: Tree<BoxedViewerTab>,
}

#[derive(Resource)]
struct BottomPanelLayout {
    tree: Tree<BoxedViewerTab>,
}

struct EditorBehavior {
    simplification: egui_tiles::SimplificationOptions,
    tab_bar_height: f32,
    gap_width: f32,
}

impl EditorBehavior {
    fn new() -> Self {
        Self {
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

impl egui_tiles::Behavior<BoxedViewerTab> for EditorBehavior {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut BoxedViewerTab,
    ) -> egui_tiles::UiResponse {
        pane.ui(ui);
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
    // Left panel tabs
    let mut left_tiles = Tiles::default();
    let left_tabs: Vec<_> = default_left_tabs()
        .into_iter()
        .map(|tab| left_tiles.insert_pane(tab))
        .collect();
    let left_root = left_tiles.insert_tab_tile(left_tabs);
    let left_tree = Tree::new("left_panel", left_root, left_tiles);

    // Bottom panel tabs
    let mut bottom_tiles = Tiles::default();
    let bottom_tabs: Vec<_> = default_bottom_tabs()
        .into_iter()
        .map(|tab| bottom_tiles.insert_pane(tab))
        .collect();
    let bottom_root = bottom_tiles.insert_tab_tile(bottom_tabs);
    let bottom_tree = Tree::new("bottom_panel", bottom_root, bottom_tiles);

    commands.insert_resource(LeftPanelLayout { tree: left_tree });
    commands.insert_resource(BottomPanelLayout { tree: bottom_tree });
    commands.insert_resource(ViewportDimensions {
        left_width: 0.0,
        bottom_height: 0.0,
    });
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
fn update_ui(
    mut contexts: EguiContexts,
    mut left_layout: ResMut<LeftPanelLayout>,
    mut bottom_layout: ResMut<BottomPanelLayout>,
    mut viewport_dims: ResMut<ViewportDimensions>,
    mut camera: Query<
        &mut Camera,
        (
            With<WorldSpaceCamera>,
            Without<PrimaryEguiContext>,
            Without<Camera2d>,
        ),
    >,
    window: Query<&Window, With<PrimaryWindow>>,
    mut panel_sizes_initialized: ResMut<PanelSizesInitialized>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let window = window.single()?;
    let mut camera = camera.single_mut()?;

    // Note: Panel sizes are set via default_width/default_height in the panel builders below
    // The panel_sizes_initialized flag is kept for potential future use

    // Menu bar
    let top_height = egui::TopBottomPanel::top("top_panel")
        .show(ctx, |ui| {
            use egui::containers::PopupCloseBehavior;
            let menu_bar = egui::MenuBar::new().config(
                egui::containers::menu::MenuConfig::default()
                    .close_behavior(PopupCloseBehavior::CloseOnClickOutside),
            );

            menu_bar.ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open Project…").clicked() {
                        ui.close();
                    }
                    if ui.button("Save Layout").clicked() {
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.label("Toggle panels (not yet wired)");
                });
                ui.menu_button("Tools", |ui| {
                    ui.label("Simulation controls coming soon");
                });
            });
        })
        .response
        .rect
        .height();

    // Left panel with tabs - set default width only on first frame
    let left_panel = egui::SidePanel::left("left_panel").resizable(true);
    let left_panel = if !panel_sizes_initialized.0 {
        left_panel.default_width(200.0)
    } else {
        left_panel
    };
    let left_width = left_panel
        .show(ctx, |ui| {
            let mut behavior = EditorBehavior::new();
            left_layout.tree.ui(&mut behavior, ui);
        })
        .response
        .rect
        .width();

    // Bottom panel with tabs - set default height only on first frame
    let bottom_panel = egui::TopBottomPanel::bottom("bottom_panel").resizable(true);
    let bottom_panel = if !panel_sizes_initialized.0 {
        bottom_panel.default_height(140.0)
    } else {
        bottom_panel
    };
    let bottom_height = bottom_panel
        .show(ctx, |ui| {
            let mut behavior = EditorBehavior::new();
            bottom_layout.tree.ui(&mut behavior, ui);
        })
        .response
        .rect
        .height();

    // Mark as initialized after first frame
    if !panel_sizes_initialized.0 {
        panel_sizes_initialized.0 = true;
    }

    // Update viewport dimensions
    viewport_dims.left_width = left_width;
    viewport_dims.bottom_height = bottom_height;

    // Calculate viewport in physical pixels
    let scale_factor = window.scale_factor();
    let window_width = window.physical_width();
    let window_height = window.physical_height();

    let left_physical = (left_width * scale_factor).round().max(0.0) as u32;
    let top_physical = (top_height * scale_factor).round().max(0.0) as u32;
    let bottom_physical = (bottom_height * scale_factor).round().max(0.0) as u32;

    // these padding prevent the viewport from being too close to the edges of the pane
    // which will cause the drag event of resizing the pane affects the viewport.
    // EDIT: it doesn't work. Will need to hook into panorbit camera's drag event to prevent it.
    const VIEWPORT_LEFT_PADDING: u32 = 0;
    // const VIEWPORT_TOP_PADDING: u32 = 10;
    const VIEWPORT_BOTTOM_PADDING: u32 = 0;

    // Ensure viewport position doesn't exceed window bounds
    let pos = UVec2::new(
        left_physical.min(window_width.saturating_sub(1)) + VIEWPORT_LEFT_PADDING,
        top_physical.min(window_height.saturating_sub(1)),
    );

    // Calculate size with bounds checking to prevent overflow
    let available_width = window_width.saturating_sub(pos.x);
    let available_height = window_height.saturating_sub(pos.y + VIEWPORT_BOTTOM_PADDING);
    let size = UVec2::new(
        available_width.saturating_sub(0), // No right panel for now
        available_height.saturating_sub(bottom_physical).max(1),
    );

    // check if the viewport dimensions have changed. If not, early return.
    if let Some(viewport) = &mut camera.viewport {
        if viewport.physical_size == size {
            // panic!("Viewport dimensions have not changed");
            return Ok(());
        }
        println!("Viewport dimensions have changed");

        // Update camera viewport
        viewport.physical_position = pos;
        viewport.physical_size = size;
    }
    dbg!(&camera);

    Ok(())
}
