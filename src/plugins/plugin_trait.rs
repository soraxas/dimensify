use crate::graphics::BevyMaterial;
use crate::harness::Harness;
use crate::physics::PhysicsState;
use crate::{DimensifyGraphics, DimensifyState, GraphicsManager};
use bevy::prelude::*;
use bevy_egui::egui::Ui;
// use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy_egui::EguiContexts;

pub struct DimensifyPluginDrawArgs<'a, 'b, 'c, 'd, 'e, 'f> {
    pub(crate) graphics: DimensifyGraphics<'a, 'b, 'c, 'd, 'e, 'f>,
    pub(crate) harness: &'a mut Harness,
    pub(crate) state: &'a mut DimensifyState,
}

pub trait DimensifyPlugin {
    fn init_plugin(&mut self) {}
    fn init_graphics(
        &mut self,
        _graphics: &mut GraphicsManager,
        _commands: &mut Commands,
        _meshes: &mut Assets<Mesh>,
        _materials: &mut Assets<BevyMaterial>,
        _components: &mut Query<&mut Transform>,
        _harness: &mut Harness,
    ) {
    }
    fn clear_graphics(&mut self, _graphics: &mut GraphicsManager, _commands: &mut Commands) {}
    fn run_callbacks(&mut self, _harness: &mut Harness) {}
    fn step(&mut self, _physics: &mut PhysicsState) {}
    fn draw(
        &mut self,
        _plugin_args: &mut DimensifyPluginDrawArgs, // graphics: &mut GraphicsManager,
                                                   // commands: &mut Commands,
                                                   // meshes: &mut Assets<Mesh>,
                                                   // materials: &mut Assets<BevyMaterial>,
                                                   // components: &mut Query<&mut Transform>,
                                                   // harness: &mut Harness,
    ) {
    }
    fn update_main_ui(&mut self, _ui: &mut Ui) {}
    fn update_ui(
        &mut self,
        _ui_context: &EguiContexts,
        _harness: &mut Harness,
        _graphics: &mut GraphicsManager,
        _commands: &mut Commands,
        _meshes: &mut Assets<Mesh>,
        _materials: &mut Assets<BevyMaterial>,
        _components: &mut Query<&mut Transform>,
    ) {
    }
    fn build_bevy_plugin(&mut self, _app: &mut App) {}
    fn profiling_string(&self) -> String {
        String::from("")
    }
}
