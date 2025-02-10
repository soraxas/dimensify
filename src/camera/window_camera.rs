use bevy::prelude::*;
use bevy::render::{
    camera::RenderTarget,
    render_resource::{
        Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    },
};

use bevy_egui::{EguiContexts, EguiUserTextures};
use egui::Id;

#[derive(Component, Debug)]
#[require(Transform, Camera3d)]
pub struct FloatingCamera {
    pub img_handle: Handle<Image>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, render_floating_camera_to_window);
}

/// Construct a camera that renders to an egui image texture.
pub fn build_camera_to_egui_img_texture(
    width: u32,
    height: u32,
    images: &mut Assets<Image>,
    egui_user_textures: &mut EguiUserTextures,
) -> (Handle<Image>, Camera) {
    let size = Extent3d {
        width,
        height,
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

    let camera = Camera {
        target: RenderTarget::Image(image_handle.clone()),
        clear_color: ClearColorConfig::Custom(Color::srgba(1.0, 1.0, 1.0, 0.0)),
        ..default()
    };

    (image_handle, camera)
}

pub fn render_floating_camera_to_window(
    floating_cameras: Query<(&FloatingCamera, Option<&Name>)>,
    mut contexts: EguiContexts,
) {
    for (cam, name) in floating_cameras.iter() {
        let cube_preview_texture_id = contexts.image_id(&cam.img_handle).unwrap();

        let ctx = contexts.ctx_mut();

        egui::Window::new(name.map(|n| n.as_str()).unwrap_or("Camera"))
            // .max_height(300.)
            // .max_width(200.)
            .id(Id::new(&cam.img_handle))
            .auto_sized()
            .show(ctx, |ui| {
                ui.image(egui::load::SizedTexture::new(
                    cube_preview_texture_id,
                    egui::vec2(300., 300.),
                ));
            });
    }
}
