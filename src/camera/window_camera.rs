use bevy::prelude::*;
use bevy::{
    prelude::*,
    render::{
        camera::RenderTarget,
        render_resource::{
            Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
        },
    },
};
use bevy_editor_pls::{
    editor::EditorInternalState,
    editor_window::{open_floating_window, EditorWindow, EditorWindowContext},
    AddEditorWindow,
};
use bevy_egui::{egui::Widget, EguiContexts, EguiPlugin, EguiUserTextures};

use bevy_panorbit_camera::PanOrbitCamera;

use crate::ui::robot_state_setter::EditorState;

use bevy::prelude::*;
use smooth_bevy_cameras::{
    controllers::unreal::{UnrealCameraBundle, UnrealCameraController, UnrealCameraPlugin},
    LookTransform, LookTransformBundle, LookTransformPlugin, Smoother,
};

pub(crate) fn plugin(app: &mut App) {
    app
        // .add_systems(Update, insert_colliding_marker)
        .add_editor_window::<CamEditorWindow>()
        .add_plugins(LookTransformPlugin)
        .add_plugins(UnrealCameraPlugin::default())
        .add_systems(Update, render_to_image_example_system);
}

pub(crate) struct CamEditorWindow;

impl EditorWindow for CamEditorWindow {
    type State = EditorState;

    const NAME: &'static str = "Cam Spawner";
    // const DEFAULT_SIZE: (f32, f32) = (200., 150.);

    fn app_setup(app: &mut App) {
        app.add_systems(Startup, |internal_state: ResMut<EditorInternalState>| {
            open_floating_window::<Self>(internal_state.into_inner());
        });
    }

    fn ui(world: &mut World, mut _cx: EditorWindowContext, ui: &mut egui::Ui) {
        if ui.button("Spawn Camera").clicked() {
            world.resource_scope(|world, mut egui_user_textures: Mut<EguiUserTextures>| {
                world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
                    world.resource_scope(|world, mut meshes: Mut<Assets<Mesh>>| {
                        world.resource_scope(
                            |world, mut materials: Mut<Assets<StandardMaterial>>| {
                                let mut commands = world.commands();

                                spawn_camera(
                                    &mut commands,
                                    images.as_mut(),
                                    meshes.as_mut(),
                                    materials.as_mut(),
                                    egui_user_textures.as_mut(),
                                );
                            },
                        )
                    })
                })
            });
        }

        ui.separator();

        for (mut cam, name) in world
            .query::<(&mut PanOrbitCamera, Option<&Name>)>()
            .iter_mut(world)
        {
            let name = name
                .map(|n| n.as_str())
                .unwrap_or(std::any::type_name::<PanOrbitCamera>());
            ui.checkbox(&mut cam.enabled, format!("Camera: {}", name));
        }

        for (mut cam, name) in world
            .query::<(&mut UnrealCameraController, Option<&Name>)>()
            .iter_mut(world)
        {
            let name = name
                .map(|n| n.as_str())
                .unwrap_or(std::any::type_name::<UnrealCameraController>());
            ui.checkbox(&mut cam.enabled, format!("Camera: {}", name));
        }
    }
}

#[derive(Component, Debug)]
pub struct FloatingCamera {
    pub img_handle: Handle<Image>,
}

fn spawn_camera(
    commands: &mut Commands,
    mut images: &mut Assets<Image>,
    mut meshes: &mut Assets<Mesh>,
    mut materials: &mut Assets<StandardMaterial>,
    mut egui_user_textures: &mut EguiUserTextures,
) {
    let size = Extent3d {
        width: 512,
        height: 512,
        ..default()
    };

    // This is the texture that will be rendered to.
    let mut image = Image {
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

    ////////////////////////////////////////

    let image_handle = images.add(image);
    egui_user_textures.add_image(image_handle.clone());

    let mut entity = commands.spawn(Camera3dBundle {
        camera: Camera {
            target: RenderTarget::Image(image_handle.clone()),
            clear_color: ClearColorConfig::Custom(Color::srgba(1.0, 1.0, 1.0, 0.0)),
            ..default()
        },
        transform: Transform::from_translation(Vec3::new(0.0, 0.0, 15.0))
            .looking_at(Vec3::default(), Vec3::Y),
        ..default()
    });
    entity
        .insert(UnrealCameraBundle::new(
            UnrealCameraController::default(),
            Vec3::new(-2.0, 5.0, 5.0),
            Vec3::new(0., 0., 0.),
            Vec3::Y,
        ))
        .insert(FloatingCamera {
            img_handle: image_handle,
        });

    entity.with_children(|parent| {
        parent.spawn(PbrBundle {
            // camera shape
            mesh: meshes.add(Cuboid::new(0.5, 0.35, 0.05).mesh()),
            material: materials.add(StandardMaterial {
                base_color: Color::srgba(0.3, 0.3, 0.3, 0.8),
                alpha_mode: AlphaMode::Blend,
                // Remove this if you want it to use the world's lighting.
                unlit: true,
                ..default()
            }),
            ..default()
        });
    });

    // FloatingCamera {
    //     img_handle: image_handle,
    // };
}

fn render_to_image_example_system(
    floating_cameras: Query<&FloatingCamera>,
    // preview_cube_query: Query<&Handle<StandardMaterial>, With<PreviewPassCube>>,
    // main_cube_query: Query<&Handle<StandardMaterial>, With<MainPassCube>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut contexts: EguiContexts,
) {
    for cam in floating_cameras.iter() {
        let cube_preview_texture_id = contexts.image_id(&cam.img_handle).unwrap();
        // let preview_material_handle = preview_cube_query.single();
        // let preview_material = materials.get_mut(preview_material_handle).unwrap();

        let ctx = contexts.ctx_mut();
        let mut apply = false;

        egui::Window::new(format!("Cube material preview {:?}", cam.img_handle)).show(ctx, |ui| {
            ui.image(egui::load::SizedTexture::new(
                cube_preview_texture_id,
                egui::vec2(300., 300.),
            ));
            // egui::Grid::new("preview").show(ui, |ui| {
            //     ui.label("Base color:");
            //     color_picker_widget(ui, &mut preview_material.base_color);
            //     ui.end_row();

            //     ui.label("Emissive:");
            //     let mut emissive_color = Color::from(preview_material.emissive);
            //     color_picker_widget(ui, &mut emissive_color);
            //     preview_material.emissive = emissive_color.into();
            //     ui.end_row();

            //     ui.label("Perceptual roughness:");
            //     egui::Slider::new(&mut preview_material.perceptual_roughness, 0.089..=1.0).ui(ui);
            //     ui.end_row();

            //     ui.label("Reflectance:");
            //     egui::Slider::new(&mut preview_material.reflectance, 0.0..=1.0).ui(ui);
            //     ui.end_row();

            //     ui.label("Unlit:");
            //     ui.checkbox(&mut preview_material.unlit, "");
            //     ui.end_row();
            // });

            apply = ui.button("Apply").clicked();
        });

        // if apply {
        //     let material_clone = preview_material.clone();

        //     let main_material_handle = main_cube_query.single();
        //     materials.insert(main_material_handle, material_clone);
        // }
    }
}
