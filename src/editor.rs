use bevy::{
    camera::{ImageRenderTarget, RenderTarget},
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    },
    window::PrimaryWindow,
};
use bevy_egui::{
    egui::{self, TextureId},
    EguiContexts, EguiPlugin, EguiUserTextures,
};
use bevy_egui::{EguiPrimaryContextPass, EguiTextureHandle};
use egui::load::SizedTexture;
use egui::ImageSource;
use egui_tiles::{self, Tiles, Tree};

pub fn plugin(app: &mut App) {
    app.add_plugins(EguiPlugin::default())
        .add_systems(Startup, setup_editor_layout)
        .add_systems(Startup, setup_viewport)
        // .add_systems(Startup, setup_scene)
        .add_systems(EguiPrimaryContextPass, update_ui)
        // .add_systems(Update, rotate_cube)
        .add_systems(Startup, setup_scene);
}

// stores the image which the camera renders to, so that we can display a viewport inside a tab
#[derive(Deref, Resource)]
struct Viewport(Handle<Image>);

// marker struct for the example cube
#[derive(Component)]
struct ExampleCube;

#[derive(Resource, Clone)]
struct EditorMaterials {
    cube: Handle<StandardMaterial>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LeftTab {
    Hierarchy,
    Inspector,
    Assets,
}

impl LeftTab {
    const ALL: [Self; 3] = [Self::Hierarchy, Self::Inspector, Self::Assets];

    fn title(self) -> &'static str {
        match self {
            Self::Hierarchy => "Hierarchy",
            Self::Inspector => "Inspector",
            Self::Assets => "Assets",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BottomTab {
    Console,
    Diagnostics,
    Tasks,
}

impl BottomTab {
    const ALL: [Self; 3] = [Self::Console, Self::Diagnostics, Self::Tasks];

    fn title(self) -> &'static str {
        match self {
            Self::Console => "Console",
            Self::Diagnostics => "Diagnostics",
            Self::Tasks => "Tasks",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum EditorPane {
    Viewport,
    Left(LeftTab),
    Bottom(BottomTab),
}

impl EditorPane {
    fn title(&self) -> &'static str {
        match self {
            EditorPane::Viewport => "Viewport",
            EditorPane::Left(tab) => tab.title(),
            EditorPane::Bottom(tab) => tab.title(),
        }
    }
}

#[derive(Resource)]
struct EditorLayout {
    tree: Tree<EditorPane>,
}

struct EditorBehavior<'a> {
    viewport_image: &'a mut Image,
    viewport_tex_id: TextureId,
    window_scale_factor: f64,
    cube_material: &'a mut StandardMaterial,
    simplification: egui_tiles::SimplificationOptions,
    tab_bar_height: f32,
    gap_width: f32,
}

impl<'a> EditorBehavior<'a> {
    fn new(
        viewport_image: &'a mut Image,
        viewport_tex_id: TextureId,
        window_scale_factor: f64,
        cube_material: &'a mut StandardMaterial,
    ) -> Self {
        Self {
            viewport_image,
            viewport_tex_id,
            window_scale_factor,
            cube_material,
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

impl egui_tiles::Behavior<EditorPane> for EditorBehavior<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut EditorPane,
    ) -> egui_tiles::UiResponse {
        match pane {
            EditorPane::Viewport => {
                let mut desired_size = ui.available_size();
                desired_size.x = desired_size.x.max(64.0);
                desired_size.y = desired_size.y.max(64.0);

                let pixel_size = UVec2::new(
                    (desired_size.x * self.window_scale_factor as f32)
                        .round()
                        .max(1.0) as u32,
                    (desired_size.y * self.window_scale_factor as f32)
                        .round()
                        .max(1.0) as u32,
                );

                if self.viewport_image.size() != pixel_size {
                    self.viewport_image.resize(Extent3d {
                        width: pixel_size.x,
                        height: pixel_size.y,
                        ..default()
                    });
                }

                let image = egui::Image::new(ImageSource::Texture(SizedTexture::new(
                    self.viewport_tex_id,
                    desired_size,
                )))
                .fit_to_exact_size(desired_size);

                ui.add(image);
            }
            EditorPane::Left(tab) => match tab {
                LeftTab::Hierarchy => {
                    ui.heading("Scene Graph");
                    ui.separator();
                    ui.label("Root\n ├─ Robot\n └─ Environment");
                }
                LeftTab::Inspector => {
                    ui.heading("Inspector");
                    ui.label(format!("Cube color: {:?}", self.cube_material.base_color));
                }
                LeftTab::Assets => {
                    ui.heading("Assets");
                    ui.label("Placeholder for asset browser.");
                }
            },
            EditorPane::Bottom(tab) => match tab {
                BottomTab::Console => {
                    ui.heading("Console");
                    ui.label("Logs will appear here.");
                }
                BottomTab::Diagnostics => {
                    ui.heading("Diagnostics");
                    ui.label("Frame timing, GPU metrics, etc.");
                }
                BottomTab::Tasks => {
                    ui.heading("Tasks");
                    ui.label("Background task status.");
                }
            },
        }

        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &EditorPane) -> egui::WidgetText {
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
    let mut tiles = Tiles::default();

    let left_tabs: Vec<_> = LeftTab::ALL
        .iter()
        .map(|tab| tiles.insert_pane(EditorPane::Left(*tab)))
        .collect();
    let left_column = tiles.insert_tab_tile(left_tabs);

    let viewport = tiles.insert_pane(EditorPane::Viewport);

    let bottom_tabs: Vec<_> = BottomTab::ALL
        .iter()
        .map(|tab| tiles.insert_pane(EditorPane::Bottom(*tab)))
        .collect();
    let bottom_strip = tiles.insert_tab_tile(bottom_tabs);

    let right_stack = tiles.insert_vertical_tile(vec![viewport, bottom_strip]);
    let root = tiles.insert_horizontal_tile(vec![left_column, right_stack]);

    let tree = Tree::new("editor_layout", root, tiles);

    commands.insert_resource(EditorLayout { tree });
}

fn setup_viewport(
    mut egui_user_textures: ResMut<EguiUserTextures>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    // default size (will be immediately overwritten)
    let size = Extent3d {
        width: 512,
        height: 512,
        ..default()
    };

    // this is the texture that will be rendered to
    let mut image: Image = Image {
        texture_descriptor: TextureDescriptor {
            label: None,
            size,
            dimension: TextureDimension::D2,
            format: TextureFormat::Bgra8UnormSrgb,
            mip_level_count: 1,
            sample_count: 1,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        },
        ..default()
    };

    // fill image.data with zeroes
    image.resize(size);

    // create a handle to the image
    let image_handle = images.add(image);

    egui_user_textures.add_image(EguiTextureHandle::Weak(image_handle.id()));
    commands.insert_resource(Viewport(image_handle.clone()));

    // spawn a camera which renders to the image handle
    commands.spawn((
        Camera3d::default(),
        Camera {
            // render to the image
            target: RenderTarget::Image(ImageRenderTarget::from(image_handle)),
            ..default()
        },
        Transform::from_translation(Vec3::new(20., 20., 20.)).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // cube mesh and material
    let mesh = meshes.add(Mesh::from(Cuboid::new(4.0, 4.0, 4.0)));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.8, 0.7, 0.6),
        reflectance: 0.02,
        unlit: false,
        ..default()
    });
    commands.insert_resource(EditorMaterials {
        cube: material.clone(),
    });
    // example cube
    commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(Vec3::new(0., 0., 0.)),
        ))
        .insert(ExampleCube);
    // // directional light
    // commands.spawn(DirectionalLightBundle {
    //     transform: Transform::from_translation(Vec3::new(10., 30., 15.))
    //         .looking_at(Vec3::ZERO, Vec3::Y),

    //     ..default()
    // });
    // // ambient light
    // commands.insert_resource(AmbientLight {
    //     color: Color::WHITE,
    //     brightness: 0.5,
    // });
}

fn update_ui(
    mut contexts: EguiContexts,
    mut layout: ResMut<EditorLayout>,
    viewport: Res<Viewport>,
    mut image_assets: ResMut<Assets<Image>>,
    mut material_assets: ResMut<Assets<StandardMaterial>>,
    materials_handles: Res<EditorMaterials>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let viewport_image = image_assets
        .get_mut(&viewport.0)
        .expect("Could not get viewport image");
    let viewport_tex_id = contexts
        .image_id(&viewport.0)
        .expect("Could not get viewport texture ID");
    let window_scale_factor = window.single().unwrap().scale_factor();
    let ctx = contexts.ctx_mut().unwrap();

    // as an example we get the cube material so it can be edited in the UI
    let selected_material = materials_handles.cube.clone();
    let cube_material = material_assets
        .get_mut(selected_material.id())
        .expect("Cube material missing from asset storage");

    // menu bar along the top of the screen
    egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
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
    });

    let mut behavior = EditorBehavior::new(
        viewport_image,
        viewport_tex_id,
        window_scale_factor as f64,
        cube_material,
    );

    egui::CentralPanel::default().show(ctx, |ui| {
        layout.tree.ui(&mut behavior, ui);
    });
}

// fn rotate_cube(time: Res<Time>, mut query: Query<&mut Transform, With<ExampleCube>>) {
//     for mut transform in &mut query {
//         transform.rotate_x(1.5 * time.delta_seconds());
//         transform.rotate_z(1.3 * time.delta_seconds());
//     }
// }
