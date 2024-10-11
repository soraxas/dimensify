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
