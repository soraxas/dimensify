use bevy::prelude::*;
use bevy::state::state::FreelyMutableState;
use strum::{AsRefStr, EnumIter, IntoEnumIterator, IntoStaticStr};

/// able to create dropdown menu for egui
pub trait AsEguiDropdownExt {
    fn with_egui_dropdown(world: &mut World, ui: &mut egui::Ui, description: &str)
    where
        Self: States + FreelyMutableState,
        Self: AsRef<str> + IntoEnumIterator,
    {
        let ori_state = world.resource::<State<Self>>().get();
        let mut state = ori_state.clone();

        egui::ComboBox::from_label(description)
            .selected_text(state.as_ref())
            .show_ui(ui, |ui| {
                Self::iter().for_each(|variant| {
                    ui.selectable_value(&mut state, variant.clone(), variant.as_ref());
                });
            });
        if ori_state != &state {
            world.resource_mut::<NextState<Self>>().set(state);
        }
    }
}

impl<T> AsEguiDropdownExt for T
where
    T: States + FreelyMutableState,
    T: AsRef<str> + IntoEnumIterator,
{
}

fn lerp_ray3d(ray: &Ray3d, other: &Ray3d, t: f32) -> Ray3d {
    Ray3d {
        origin: ray.origin.lerp(other.origin, t),
        direction: Dir3::new(ray.direction.lerp(*other.direction, t)).unwrap(),
    }
}


/// Trait to interpolate between two Ray3d
pub trait LinearParameterisedTrait {
    type Output;

    /// t must be between 0.0 and 1.0
    fn sample(&self, t: f32) -> Self::Output;
}

impl LinearParameterisedTrait for Vec<Ray3d> {
    type Output = Ray3d;

    fn sample(&self, t: f32) -> Ray3d {
        assert!((0.0..=1.0).contains(&t), "t must be between 0.0 and 1.0.");

        let num_rays = self.len();
        if t <= 0.0 {
            return self[0]; // Return the first ray for t == 0.0
        } else if t >= 1.0 {
            return self[num_rays - 1]; // Return the last ray for t == 1.0
        }

        // For intermediate values, we perform linear interpolation
        let index = (t * (num_rays as f32 - 1.0)).floor() as usize;
        let next_index = (index + 1).min(num_rays - 1);

        let lerp_t = (t * (num_rays as f32 - 1.0)) - index as f32;
        lerp_ray3d(&self[index], &self[next_index], lerp_t)
    }
}
