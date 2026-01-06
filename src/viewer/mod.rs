use bevy::prelude::*;

#[derive(Resource)]
pub struct ViewerState;

pub fn plugin(app: &mut App) {
    app.init_resource::<ViewerState>();
}

impl Default for ViewerState {
    fn default() -> Self {
        Self
    }
}
