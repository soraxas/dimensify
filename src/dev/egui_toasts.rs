use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::EguiContext;
use egui::FontId;
use egui_notify::Toasts;

#[derive(Resource, Default)]
pub struct EguiToasts(pub Toasts);

const TOAST_VERTICAL_MARGIN: f32 = 30.0;
const DEFAULT_TOAST_FONT_SIZE: f32 = 25.0;

pub(crate) fn plugin(app: &mut App) {
    app.insert_resource(EguiToasts(
        Toasts::default().with_margin([0., TOAST_VERTICAL_MARGIN].into()),
    ))
    .add_systems(Update, update_toasts);
}

fn update_toasts(mut toasts: ResMut<EguiToasts>, mut ctx: Query<&mut EguiContext>) {
    toasts.0.show(ctx.single_mut().get_mut());
}

pub(crate) fn error_to_toast(In(result): In<Result<(), eyre::Error>>, toasts: ResMut<EguiToasts>) {
    let toasts = &mut toasts.into_inner().0;
    if let Err(err) = result {
        toasts
            .error(format!("{}", err))
            .duration(Some(Duration::from_secs(8)))
            .font(FontId::proportional(DEFAULT_TOAST_FONT_SIZE));
    }
}
