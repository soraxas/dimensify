
use std::{marker::PhantomData, sync::Mutex};

use bevy::prelude::*;
use bevy::ecs::world;
use bevy_egui::{egui};
use super::ViewerTab;
use bevy_inspector_egui::{bevy_inspector};
// use crate::{bevy_inspector::Filter, utils::pretty_type_name};
// use bevy_app::Plugin;
// use bevy_asset::Asset;
// use bevy_ecs::{prelude::*, query::QueryFilter, schedule::BoxedCondition};
// use bevy_egui::{EguiContext, EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
// use bevy_reflect::Reflect;
// use bevy_state::state::FreelyMutableState;

// use crate::{DefaultInspectorConfigPlugin, bevy_inspector};

const DEFAULT_SIZE: (f32, f32) = (320., 160.);

pub struct WorldInspectTab;

impl ViewerTab for WorldInspectTab {
    fn title(&self) -> &'static str {
        "World Inspector"
    }

    fn ui(&self, ui: &mut egui::Ui, world: &mut World) {
        ui.heading("Inspector");

            egui::ScrollArea::both().show(ui, |ui| {
                bevy_inspector::ui_for_world(world, ui);
                ui.allocate_space(ui.available_size());
            });
    }
}
