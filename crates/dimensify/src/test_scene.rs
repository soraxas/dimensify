use crate::scene::preset::{add_floor, add_sun};
use bevy::prelude::*;

use std::f32::constants::*;

#[derive(Resource)]
struct RotateSun(bool);

pub fn plugin(app: &mut App) {
    app
        .insert_resource(RotateSun(false))
        .add_systems(Startup, (
            setup,
            add_sun,
            add_floor,
        ))
        .add_systems(Update, (animate_light_direction, switch_mode))
        // setup more distance for shadow map
        ;
}

fn setup(mut commands: Commands) {
    // Example instructions
    commands.spawn((Text::new("Press Space to rotate sun"),));
}

fn animate_light_direction(
    time: Res<Time>,
    mut query: Query<&mut Transform, With<DirectionalLight>>,
    rotate_sun: Res<RotateSun>,
) {
    if !rotate_sun.0 {
        return;
    }
    for mut transform in &mut query {
        transform.rotate_y(time.delta_secs() * PI / 5.0);
    }
}

fn switch_mode(
    mut text: Query<&mut Text>,
    keys: Res<ButtonInput<KeyCode>>,
    mut rotate_sun: ResMut<RotateSun>,
) -> Result {
    let mut text = text.single_mut()?;

    text.clear();

    if keys.just_pressed(KeyCode::Space) {
        rotate_sun.0 = !rotate_sun.0;
    }
    Ok(())
}
