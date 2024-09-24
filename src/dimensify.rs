#![allow(clippy::bad_bit_mask)] // otherwise clippy complains because of viewerStateFlags::NONE which is 0.
#![allow(clippy::unnecessary_cast)] // allowed for f32 -> f64 cast for the f64 viewer.

use std::env;

use bevy::prelude::*;

use crate::plugins::DebugRenderDimensifyPlugin;
// use crate::bevy_plugins::debug_render::{RapierDebugRenderPlugin};
use crate::physics::{DeserializedPhysicsSnapshot, PhysicsEvents, PhysicsSnapshot, PhysicsState};
use crate::plugins::{DimensifyPlugin, DimensifyPluginDrawArgs};
use crate::{graphics, harness, mouse, ui};
use crate::{graphics::GraphicsManager, harness::RunState};

use na::{self, Point2, Point3, Vector3};
use rapier3d::control::DynamicRayCastVehicleController;
use rapier3d::control::KinematicCharacterController;
use rapier3d::dynamics::{
    ImpulseJointSet, IntegrationParameters, MultibodyJointSet, RigidBodyActivation,
    RigidBodyHandle, RigidBodySet,
};
use rapier3d::geometry::{ColliderHandle, ColliderSet};
use rapier3d::math::{Real, Vector};
use rapier3d::pipeline::{PhysicsHooks, QueryFilter};

use crate::harness::Harness;
use bevy::render::camera::{Camera, ClearColor};
use bevy_egui::EguiContexts;

use crate::camera3d::{OrbitCamera, OrbitCameraPlugin};
use crate::graphics::{BevyMaterial, ResetWorldGraphicsEvent};
// use bevy::render::render_resource::RenderPipelineDescriptor;

#[derive(PartialEq)]
pub enum RunMode {
    Running,
    Stop,
    Step,
}

bitflags::bitflags! {
    #[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
    pub struct DimensifyStateFlags: u32 {
        const NONE = 0;
        const SLEEP = 1 << 0;
        const SUB_STEPPING = 1 << 1;
        const SHAPES = 1 << 2;
        const JOINTS = 1 << 3;
        const AABBS = 1 << 4;
        // const CENTER_OF_MASSES = 1 << 7;
        const WIREFRAME = 1 << 8;
        const STATISTICS = 1 << 9;
    }
}

bitflags::bitflags! {
    #[derive(Copy, Clone, PartialEq, Eq, Debug)]
    pub struct DimensifyActionFlags: u32 {
        const EXAMPLE_CHANGED = 1 << 1;
        const RESTART = 1 << 2;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum RapierSolverType {
    #[default]
    TgsSoft,
    TgsSoftNoWarmstart,
    PgsLegacy,
}

pub type SimulationBuilders = Vec<(&'static str, fn(&mut Dimensify))>;

enum PendingAction {
    ResetWorldGraphicsEvent,
}

#[derive(Resource)]
pub struct DimensifyState {
    pending_actions: Vec<PendingAction>,
    pub running: RunMode,
    pub character_body: Option<RigidBodyHandle>,
    pub vehicle_controller: Option<DynamicRayCastVehicleController>,
    //    pub grabbed_object: Option<DefaultBodyPartHandle>,
    //    pub grabbed_object_constraint: Option<DefaultJointConstraintHandle>,
    pub grabbed_object_plane: (Point3<f32>, Vector3<f32>),
    pub prev_flags: DimensifyStateFlags,
    pub flags: DimensifyStateFlags,
    pub action_flags: DimensifyActionFlags,
    pub example_names: Vec<&'static str>,
    pub selected_example: usize,
    pub solver_type: RapierSolverType,
    pub snapshot: Option<PhysicsSnapshot>,
    pub nsteps: usize,
    camera_locked: bool, // Used so that the camera can remain the same before and after we change backend or press the restart button.
}

#[derive(Resource)]
struct SceneBuilders(SimulationBuilders);

#[derive(Resource)]
pub(crate) struct Plugins(pub(crate) Vec<Box<dyn DimensifyPlugin>>);

pub struct DimensifyGraphics<'a, 'b, 'c, 'd, 'e, 'f> {
    pub graphics: &'a mut GraphicsManager,
    pub commands: &'a mut Commands<'d, 'e>,
    pub meshes: &'a mut Assets<Mesh>,
    pub materials: &'a mut Assets<BevyMaterial>,
    pub material_handles: &'a mut Query<'b, 'f, &'c mut Handle<BevyMaterial>>,
    pub components: &'a mut Query<'b, 'f, &'c mut Transform>,
    pub camera_transform: GlobalTransform,
    pub camera: &'a mut OrbitCamera,
    pub camera_view: &'a Camera,
    pub window: Option<&'a Window>,
    pub keys: &'a ButtonInput<KeyCode>,
    pub mouse: &'a SceneMouse,
}

pub struct Dimensify<'a, 'b, 'c, 'd, 'e, 'f> {
    pub graphics: Option<DimensifyGraphics<'a, 'b, 'c, 'd, 'e, 'f>>,
    pub harness: &'a mut Harness,
    pub state: &'a mut DimensifyState,
    pub(crate) plugins: &'a mut Plugins,
}

pub struct DimensifyApp {
    builders: SceneBuilders,
    graphics: GraphicsManager,
    state: DimensifyState,
    harness: Harness,
    plugins: Plugins,
}

impl DimensifyApp {
    pub fn new_empty() -> Self {
        let graphics = GraphicsManager::new();
        let flags = DimensifyStateFlags::SLEEP;

        let state = DimensifyState {
            pending_actions: Vec::new(),
            running: RunMode::Stop,
            character_body: None,
            vehicle_controller: None,
            //            grabbed_object: None,
            //            grabbed_object_constraint: None,
            grabbed_object_plane: (Point3::origin(), na::zero()),
            snapshot: None,
            prev_flags: flags,
            flags,
            action_flags: DimensifyActionFlags::empty(),
            example_names: Vec::new(),
            selected_example: 0,
            solver_type: RapierSolverType::default(),
            nsteps: 1,
            camera_locked: false,
        };

        let harness = Harness::new_empty();

        DimensifyApp {
            builders: SceneBuilders(Vec::new()),
            plugins: Plugins(Vec::new()),
            graphics,
            state,
            harness,
        }
    }

    pub fn from_builders(default: usize, builders: SimulationBuilders) -> Self {
        let mut res = DimensifyApp::new_empty();
        res.state
            .action_flags
            .set(DimensifyActionFlags::EXAMPLE_CHANGED, true);
        res.state.selected_example = default;
        res.set_builders(builders);
        res
    }

    pub fn set_builders(&mut self, builders: SimulationBuilders) {
        self.state.example_names = builders.iter().map(|e| e.0).collect();
        self.builders = SceneBuilders(builders)
    }

    pub fn run(self) {
        self.run_with_init(|_| {})
    }

    pub fn run_with_init(mut self, mut init: impl FnMut(&mut App)) {
        let mut args = env::args();

        let cmds = [
            ("--help", Some("-h"), "Print this help message and exit."),
            ("--pause", None, "Do not start the simulation right away."),
            ("--bench", None, "Run benchmark mode without rendering."),
            (
                "--bench-iters <num:u32>",
                None,
                "Number of frames to run in benchmarking.",
            ),
        ];
        let usage = |exe_name: &str, err: Option<&str>| {
            println!("Usage: {} [OPTION] ", exe_name);
            println!();
            println!("Options:");
            for (long, s, desc) in cmds {
                let s_str = if let Some(s) = s {
                    format!(", {s}")
                } else {
                    String::new()
                };
                println!("    {long}{s_str} - {desc}",)
            }
            if let Some(err) = err {
                eprintln!("Error: {err}");
            }
        };

        if args.len() > 1 {
            let exname = args.next().unwrap();
            for arg in args {
                match arg.as_str() {
                    "--help" | "-h" => {
                        usage(&exname[..], None);
                        return;
                    }
                    "--pause" => {
                        self.state.running = RunMode::Stop;
                    }
                    // ignore extra arguments
                    _ => {}
                }
            }
        }

        {
            use crate::plugins::HighlightHoveredBodyPlugin;

            self.plugins
                .0
                .push(Box::new(HighlightHoveredBodyPlugin::default()));
            self.plugins
                .0
                .push(Box::new(DebugRenderDimensifyPlugin::default()));
        }

        {
            let title = "Dimensify".to_string();

            let window_plugin = WindowPlugin {
                primary_window: Some(Window {
                    title,
                    ..Default::default()
                }),
                ..Default::default()
            };

            let mut app = App::new();
            app.insert_resource(ClearColor(Color::from(Srgba::rgb(0.15, 0.15, 0.15))))
                .insert_resource(Msaa::Sample4)
                .insert_resource(AmbientLight {
                    brightness: 0.3,
                    ..Default::default()
                })
                .init_resource::<mouse::SceneMouse>()
                .add_plugins(DefaultPlugins.set(window_plugin))
                .add_plugins(ui::plugin)
                .add_plugins(OrbitCameraPlugin)
                // .add_plugins(WireframePlugin)
                // .add_plugins(draw_contact::plugin)
                .add_plugins(harness::snapshot_plugin)
                .add_plugins(graphics::plugin)
                .add_plugins(ui::main_ui::plugin)
                // .add_plugins(ui::plugin)
                // .add_plugins(bevy_egui::EguiPlugin)
                ;

            // #[cfg(target_arch = "wasm32")]
            // app.add_plugin(bevy_webgl2::WebGL2Plugin);

            for plugin in self.plugins.0.iter_mut() {
                plugin.init_plugin();
                plugin.build_bevy_plugin(&mut app);
            }

            app.add_systems(Startup, setup_graphics_environment)
                .insert_resource(self.graphics)
                .insert_resource(self.state)
                .insert_resource(self.harness)
                .insert_resource(self.builders)
                .insert_resource(self.plugins)
                .add_systems(Update, update_viewer)
                .add_systems(Update, track_mouse_state);

            init(&mut app);
            app.run();
        }
    }
}

impl<'a, 'b, 'c, 'd, 'e, 'f> DimensifyGraphics<'a, 'b, 'c, 'd, 'e, 'f> {
    pub fn set_body_color(&mut self, body: RigidBodyHandle, color: [f32; 3]) {
        self.graphics.set_body_color(self.materials, body, color);
    }

    pub fn add_body(
        &mut self,
        handle: RigidBodyHandle,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
    ) {
        self.graphics.add_body_colliders(
            &mut *self.commands,
            &mut *self.meshes,
            &mut *self.materials,
            handle,
            bodies,
            colliders,
        )
    }
    pub fn remove_body(&mut self, handle: RigidBodyHandle) {
        self.graphics.remove_body_nodes(&mut *self.commands, handle)
    }

    pub fn replace_body_collider(
        &mut self,
        new_collider: ColliderHandle,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
    ) {
        self.graphics.replace_body_collider(
            &mut *self.commands,
            &mut *self.meshes,
            &mut *self.materials,
            new_collider,
            bodies,
            colliders,
        )
    }

    #[deprecated]
    pub fn add_collider(&mut self, handle: ColliderHandle, colliders: &ColliderSet) {
        self.graphics.add_collider(
            &mut *self.commands,
            &mut *self.meshes,
            &mut *self.materials,
            handle,
            colliders,
        )
    }

    pub fn remove_collider(&mut self, handle: ColliderHandle, colliders: &ColliderSet) {
        if let Some(parent_handle) = colliders.get(handle).map(|c| c.parent()) {
            self.graphics
                .remove_collider_nodes(&mut *self.commands, parent_handle, handle)
        }
    }

    pub fn camera_fwd_dir(&self) -> Vector<f32> {
        (self.camera_transform * -Vec3::Z).normalize().into()
    }
}

impl<'a, 'b, 'c, 'd, 'e, 'f> Dimensify<'a, 'b, 'c, 'd, 'e, 'f> {
    pub fn set_number_of_steps_per_frame(&mut self, nsteps: usize) {
        self.state.nsteps = nsteps
    }

    pub fn set_character_body(&mut self, handle: RigidBodyHandle) {
        self.state.character_body = Some(handle);
    }

    pub fn set_vehicle_controller(&mut self, controller: DynamicRayCastVehicleController) {
        self.state.vehicle_controller = Some(controller);
    }

    pub fn integration_parameters_mut(&mut self) -> &mut IntegrationParameters {
        &mut self.harness.physics.integration_parameters
    }

    pub fn physics_state_mut(&mut self) -> &mut PhysicsState {
        &mut self.harness.physics
    }

    pub fn harness_mut(&mut self) -> &mut Harness {
        self.harness
    }

    pub fn set_world(
        &mut self,
        bodies: RigidBodySet,
        colliders: ColliderSet,
        impulse_joints: ImpulseJointSet,
        multibody_joints: MultibodyJointSet,
    ) {
        self.set_world_with_params(
            bodies,
            colliders,
            impulse_joints,
            multibody_joints,
            Vector::y() * -9.81,
            (),
        )
    }

    pub fn set_world_with_params(
        &mut self,
        bodies: RigidBodySet,
        colliders: ColliderSet,
        impulse_joints: ImpulseJointSet,
        multibody_joints: MultibodyJointSet,
        gravity: Vector<Real>,
        hooks: impl PhysicsHooks + 'static + Send + Sync,
    ) {
        self.harness.set_world_with_params(
            bodies,
            colliders,
            impulse_joints,
            multibody_joints,
            gravity,
            hooks,
        );

        self.state
            .pending_actions
            .push(PendingAction::ResetWorldGraphicsEvent);

        self.state.character_body = None;
        self.state.vehicle_controller = None;
    }

    pub fn set_graphics_shift(&mut self, shift: Vector<Real>) {
        if !self.state.camera_locked {
            if let Some(graphics) = &mut self.graphics {
                graphics.graphics.gfx_shift = shift;
            }
        }
    }

    pub fn look_at(&mut self, eye: Point3<f32>, at: Point3<f32>) {
        if !self.state.camera_locked {
            if let Some(graphics) = &mut self.graphics {
                graphics.camera.center.x = at.x;
                graphics.camera.center.y = at.y;
                graphics.camera.center.z = at.z;

                let view_dir = eye - at;
                graphics.camera.distance = view_dir.norm();

                if graphics.camera.distance > 0.0 {
                    graphics.camera.y = (view_dir.y / graphics.camera.distance).acos();
                    graphics.camera.x =
                        (-view_dir.z).atan2(view_dir.x) - std::f32::consts::FRAC_PI_2;
                }
            }
        }
    }

    pub fn set_initial_body_color(&mut self, body: RigidBodyHandle, color: [f32; 3]) {
        if let Some(graphics) = &mut self.graphics {
            graphics.graphics.set_initial_body_color(body, color);
        }
    }

    pub fn set_initial_collider_color(&mut self, collider: ColliderHandle, color: [f32; 3]) {
        if let Some(graphics) = &mut self.graphics {
            graphics
                .graphics
                .set_initial_collider_color(collider, color);
        }
    }

    pub fn set_body_wireframe(&mut self, body: RigidBodyHandle, wireframe_enabled: bool) {
        if let Some(graphics) = &mut self.graphics {
            graphics
                .graphics
                .set_body_wireframe(body, wireframe_enabled);
        }
    }

    pub fn add_callback<
        F: FnMut(Option<&mut DimensifyGraphics>, &mut PhysicsState, &PhysicsEvents, &RunState)
            + 'static
            + Send
            + Sync,
    >(
        &mut self,
        callback: F,
    ) {
        self.harness.add_callback(callback);
    }

    pub fn add_plugin(&mut self, mut plugin: impl DimensifyPlugin + 'static) {
        plugin.init_plugin();
        self.plugins.0.push(Box::new(plugin));
    }

    fn update_vehicle_controller(&mut self, events: &ButtonInput<KeyCode>) {
        if self.state.running == RunMode::Stop {
            return;
        }

        if let Some(vehicle) = &mut self.state.vehicle_controller {
            let mut engine_force = 0.0;
            let mut steering_angle = 0.0;

            for key in events.get_pressed() {
                match *key {
                    KeyCode::ArrowRight => {
                        steering_angle += -0.7;
                    }
                    KeyCode::ArrowLeft => {
                        steering_angle += 0.7;
                    }
                    KeyCode::ArrowUp => {
                        engine_force += 30.0;
                    }
                    KeyCode::ArrowDown => {
                        engine_force += -30.0;
                    }
                    _ => {}
                }
            }

            let wheels = vehicle.wheels_mut();
            wheels[0].engine_force = engine_force;
            wheels[0].steering = steering_angle;
            wheels[1].engine_force = engine_force;
            wheels[1].steering = steering_angle;

            vehicle.update_vehicle(
                self.harness.physics.integration_parameters.dt,
                &mut self.harness.physics.bodies,
                &self.harness.physics.colliders,
                &self.harness.physics.query_pipeline,
                QueryFilter::exclude_dynamic().exclude_rigid_body(vehicle.chassis),
            );
        }
    }

    fn update_character_controller(&mut self, events: &ButtonInput<KeyCode>) {
        if self.state.running == RunMode::Stop {
            return;
        }

        if let Some(character_handle) = self.state.character_body {
            let mut desired_movement = Vector::zeros();
            let mut speed = 0.1;

            {
                let (_, rot, _) = self
                    .graphics
                    .as_ref()
                    .unwrap()
                    .camera_transform
                    .to_scale_rotation_translation();
                let rot = na::Unit::new_unchecked(na::Quaternion::new(rot.w, rot.x, rot.y, rot.z));
                let mut rot_x = rot * Vector::x();
                let mut rot_z = rot * Vector::z();
                rot_x.y = 0.0;
                rot_z.y = 0.0;

                for key in events.get_pressed() {
                    match *key {
                        KeyCode::ArrowRight => {
                            desired_movement += rot_x;
                        }
                        KeyCode::ArrowLeft => {
                            desired_movement -= rot_x;
                        }
                        KeyCode::ArrowUp => {
                            desired_movement -= rot_z;
                        }
                        KeyCode::ArrowDown => {
                            desired_movement += rot_z;
                        }
                        KeyCode::Space => {
                            desired_movement += Vector::y() * 2.0;
                        }
                        KeyCode::ControlRight => {
                            desired_movement -= Vector::y();
                        }
                        KeyCode::ShiftLeft => {
                            speed /= 10.0;
                        }
                        _ => {}
                    }
                }
            }

            desired_movement *= speed;
            desired_movement -= Vector::y() * speed;

            let controller = KinematicCharacterController::default();
            let phx = &mut self.harness.physics;
            let character_body = &phx.bodies[character_handle];
            let character_collider = &phx.colliders[character_body.colliders()[0]];
            let character_mass = character_body.mass();

            let mut collisions = vec![];
            let mvt = controller.move_shape(
                phx.integration_parameters.dt,
                &phx.bodies,
                &phx.colliders,
                &phx.query_pipeline,
                character_collider.shape(),
                character_collider.position(),
                desired_movement.cast::<Real>(),
                QueryFilter::new().exclude_rigid_body(character_handle),
                |c| collisions.push(c),
            );
            if let Some(graphics) = &mut self.graphics {
                if mvt.grounded {
                    graphics.graphics.set_body_color(
                        graphics.materials,
                        character_handle,
                        [0.1, 0.8, 0.1],
                    );
                } else {
                    graphics.graphics.set_body_color(
                        graphics.materials,
                        character_handle,
                        [0.8, 0.1, 0.1],
                    );
                }
            }
            controller.solve_character_collision_impulses(
                phx.integration_parameters.dt,
                &mut phx.bodies,
                &phx.colliders,
                &phx.query_pipeline,
                character_collider.shape(),
                character_mass,
                &*collisions,
                QueryFilter::new().exclude_rigid_body(character_handle),
            );

            let character_body = &mut phx.bodies[character_handle];
            let pos = character_body.position();
            character_body.set_next_kinematic_translation(pos.translation.vector + mvt.translation);
            // character_body.set_translation(pos.translation.vector + mvt.translation, false);
        }
    }

    fn handle_common_events(&mut self, events: &ButtonInput<KeyCode>) {
        for key in events.get_just_released() {
            match *key {
                KeyCode::KeyT => {
                    if self.state.running == RunMode::Stop {
                        self.state.running = RunMode::Running;
                    } else {
                        self.state.running = RunMode::Stop;
                    }
                }
                KeyCode::KeyS => self.state.running = RunMode::Step,
                KeyCode::KeyR => self
                    .state
                    .action_flags
                    .set(DimensifyActionFlags::EXAMPLE_CHANGED, true),
                KeyCode::KeyC => {
                    // Delete 1 collider of 10% of the remaining dynamic bodies.
                    let mut colliders: Vec<_> = self
                        .harness
                        .physics
                        .bodies
                        .iter()
                        .filter(|e| e.1.is_dynamic())
                        .filter(|e| !e.1.colliders().is_empty())
                        .map(|e| e.1.colliders().to_vec())
                        .collect();
                    colliders.sort_by_key(|co| -(co.len() as isize));

                    let num_to_delete = (colliders.len() / 10).max(0);
                    for to_delete in &colliders[..num_to_delete] {
                        if let Some(graphics) = self.graphics.as_mut() {
                            graphics.remove_collider(to_delete[0], &self.harness.physics.colliders);
                        }
                        self.harness.physics.colliders.remove(
                            to_delete[0],
                            &mut self.harness.physics.islands,
                            &mut self.harness.physics.bodies,
                            true,
                        );
                    }
                }
                KeyCode::KeyD => {
                    // Delete 10% of the remaining dynamic bodies.
                    let dynamic_bodies: Vec<_> = self
                        .harness
                        .physics
                        .bodies
                        .iter()
                        .filter(|e| e.1.is_dynamic())
                        .map(|e| e.0)
                        .collect();
                    let num_to_delete = (dynamic_bodies.len() / 10).max(0);
                    for to_delete in &dynamic_bodies[..num_to_delete] {
                        if let Some(graphics) = self.graphics.as_mut() {
                            graphics.remove_body(*to_delete);
                        }
                        self.harness.physics.bodies.remove(
                            *to_delete,
                            &mut self.harness.physics.islands,
                            &mut self.harness.physics.colliders,
                            &mut self.harness.physics.impulse_joints,
                            &mut self.harness.physics.multibody_joints,
                            true,
                        );
                    }
                }
                KeyCode::KeyJ => {
                    // Delete 10% of the remaining impulse_joints.
                    let impulse_joints: Vec<_> = self
                        .harness
                        .physics
                        .impulse_joints
                        .iter()
                        .map(|e| e.0)
                        .collect();
                    let num_to_delete = (impulse_joints.len() / 10).max(0);
                    for to_delete in &impulse_joints[..num_to_delete] {
                        self.harness.physics.impulse_joints.remove(*to_delete, true);
                    }
                }
                KeyCode::KeyA => {
                    // Delete 10% of the remaining multibody_joints.
                    let multibody_joints: Vec<_> = self
                        .harness
                        .physics
                        .multibody_joints
                        .iter()
                        .map(|e| e.0)
                        .collect();
                    let num_to_delete = (multibody_joints.len() / 10).max(0);
                    for to_delete in &multibody_joints[..num_to_delete] {
                        self.harness
                            .physics
                            .multibody_joints
                            .remove(*to_delete, true);
                    }
                }
                KeyCode::KeyM => {
                    // Delete one remaining multibody.
                    let to_delete = self
                        .harness
                        .physics
                        .multibody_joints
                        .iter()
                        .next()
                        .map(|(_, _, _, link)| link.rigid_body_handle());
                    if let Some(to_delete) = to_delete {
                        self.harness
                            .physics
                            .multibody_joints
                            .remove_multibody_articulations(to_delete, true);
                    }
                }
                _ => {}
            }
        }
    }
}

fn setup_graphics_environment(mut commands: Commands) {
    commands.insert_resource(AmbientLight {
        brightness: 100.0,
        ..Default::default()
    });

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            shadows_enabled: false,
            ..Default::default()
        },
        transform: Transform {
            translation: Vec3::new(10.0, 2.0, 10.0),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_4),
            ..Default::default()
        },
        ..Default::default()
    });

    commands
        .spawn(Camera3dBundle {
            transform: Transform::from_matrix(
                Mat4::look_at_rh(
                    Vec3::new(-30.0, 30.0, 100.0),
                    Vec3::new(0.0, 10.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                )
                .inverse(),
            ),
            ..Default::default()
        })
        .insert(OrbitCamera {
            rotate_sensitivity: 0.05,
            ..OrbitCamera::default()
        })
        .insert(MainCamera);
}

use crate::mouse::{track_mouse_state, MainCamera, SceneMouse};
use bevy::window::PrimaryWindow;

#[allow(clippy::type_complexity)]
fn update_viewer<'a>(
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    // mut pipelines: ResMut<Assets<RenderPipelineDescriptor>>,
    mouse: Res<SceneMouse>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BevyMaterial>>,
    builders: ResMut<SceneBuilders>,
    mut graphics: ResMut<GraphicsManager>,
    mut state: ResMut<DimensifyState>,
    mut harness: ResMut<Harness>,
    mut plugins: ResMut<Plugins>,
    ui_context: EguiContexts,
    mut reset_graphic_event: EventWriter<ResetWorldGraphicsEvent>,
    (mut gfx_components, mut cameras, mut material_handles): (
        Query<&'a mut Transform>,
        Query<(&'a Camera, &'a GlobalTransform, &'a mut OrbitCamera)>,
        Query<&'a mut Handle<BevyMaterial>>,
    ),
    keys: Res<ButtonInput<KeyCode>>,
) {
    let meshes = &mut *meshes;
    let materials = &mut *materials;

    let mut camera: (&Camera, &GlobalTransform, Mut<'_, OrbitCamera>) = cameras.single_mut();
    graphics.toggle_wireframe_mode(
        &harness.physics.colliders,
        state.flags.contains(DimensifyStateFlags::WIREFRAME),
    );
    // Handle inputs
    let graphics_context = DimensifyGraphics {
        graphics: &mut graphics,
        commands: &mut commands,
        meshes: &mut *meshes,
        materials: &mut *materials,
        components: &mut gfx_components,
        material_handles: &mut material_handles,
        camera_view: camera.0,
        camera_transform: *camera.1,
        camera: &mut camera.2,
        window: windows.get_single().ok(),
        keys: &keys,
        mouse: &mouse,
    };

    let mut viewer = Dimensify {
        graphics: Some(graphics_context),
        state: &mut state,
        harness: &mut harness,
        plugins: &mut plugins,
    };

    // pass on any pending events. These are events that are generated outside bevy systems.
    // and we only get to send the actual events when we are inside a bevy system.
    viewer
        .state
        .pending_actions
        .drain(..)
        .for_each(|action| match action {
            PendingAction::ResetWorldGraphicsEvent => {
                reset_graphic_event.send(ResetWorldGraphicsEvent);
            }
        });

    // use crate::plugins::highlight_hovered_body::HighlightHoveredBodyPlugin;
    // viewer.add_plugin(HighlightHoveredBodyPlugin{});

    viewer.handle_common_events(&keys);
    viewer.update_character_controller(&keys);
    viewer.update_vehicle_controller(&keys);

    // Update UI
    {
        let harness = &mut *harness;

        for plugin in &mut plugins.0 {
            plugin.update_ui(
                &ui_context,
                harness,
                &mut graphics,
                &mut commands,
                &mut *meshes,
                &mut *materials,
                &mut gfx_components,
            );
        }
    }

    // Handle UI actions.
    {
        let restarted = state.action_flags.contains(DimensifyActionFlags::RESTART);
        if restarted {
            state.action_flags.set(DimensifyActionFlags::RESTART, false);
            state.camera_locked = true;
            state
                .action_flags
                .set(DimensifyActionFlags::EXAMPLE_CHANGED, true);
        }

        let example_changed = state
            .action_flags
            .contains(DimensifyActionFlags::EXAMPLE_CHANGED);
        if example_changed {
            state
                .action_flags
                .set(DimensifyActionFlags::EXAMPLE_CHANGED, false);
            clear(&mut commands, &mut graphics, &mut plugins);
            harness.clear_callbacks();
            for plugin in plugins.0.iter_mut() {
                plugin.clear_graphics(&mut graphics, &mut commands);
            }
            // plugins.0.clear();

            let selected_example = state.selected_example;
            let graphics = &mut *graphics;
            let meshes = &mut *meshes;

            let graphics_context = DimensifyGraphics {
                graphics: &mut *graphics,
                commands: &mut commands,
                meshes: &mut *meshes,
                materials: &mut *materials,
                material_handles: &mut material_handles,
                components: &mut gfx_components,
                camera_view: camera.0,
                camera_transform: *camera.1,
                camera: &mut camera.2,
                window: windows.get_single().ok(),
                keys: &keys,
                mouse: &mouse,
            };

            let mut viewer = Dimensify {
                graphics: Some(graphics_context),
                state: &mut state,
                harness: &mut harness,
                plugins: &mut plugins,
            };

            builders.0[selected_example].1(&mut viewer);

            state.camera_locked = false;
        }

        if example_changed
            || state.prev_flags.contains(DimensifyStateFlags::WIREFRAME)
                != state.flags.contains(DimensifyStateFlags::WIREFRAME)
        {
            graphics.toggle_wireframe_mode(
                &harness.physics.colliders,
                state.flags.contains(DimensifyStateFlags::WIREFRAME),
            )
        }

        if state.prev_flags.contains(DimensifyStateFlags::SLEEP)
            != state.flags.contains(DimensifyStateFlags::SLEEP)
        {
            if state.flags.contains(DimensifyStateFlags::SLEEP) {
                for (_, body) in harness.physics.bodies.iter_mut() {
                    body.activation_mut().normalized_linear_threshold =
                        RigidBodyActivation::default_normalized_linear_threshold();
                    body.activation_mut().angular_threshold =
                        RigidBodyActivation::default_angular_threshold();
                }
            } else {
                for (_, body) in harness.physics.bodies.iter_mut() {
                    body.wake_up(true);
                    body.activation_mut().normalized_linear_threshold = -1.0;
                }
            }
        }
    }

    state.prev_flags = state.flags;

    // for event in window.events().iter() {
    //     let event = handle_common_event(event);
    //     handle_special_event(window, event);
    // }

    if state.running != RunMode::Stop {
        let mut viewer_graphics = DimensifyGraphics {
            graphics: &mut graphics,
            commands: &mut commands,
            meshes: &mut *meshes,
            materials: &mut *materials,
            material_handles: &mut material_handles,
            components: &mut gfx_components,
            camera_view: camera.0,
            camera_transform: *camera.1,
            camera: &mut camera.2,
            window: windows.get_single().ok(),
            keys: &keys,
            mouse: &mouse,
        };
        for _ in 0..state.nsteps {
            harness.step_with_graphics(Some(&mut viewer_graphics));

            for plugin in &mut plugins.0 {
                plugin.step(&mut harness.physics)
            }

            for plugin in &mut plugins.0 {
                plugin.run_callbacks(&mut harness);
            }
        }
    }

    graphics.draw(
        &harness.physics.bodies,
        &harness.physics.colliders,
        &mut gfx_components,
        &mut *materials,
    );

    let graphics_context = DimensifyGraphics {
        graphics: &mut graphics,
        commands: &mut commands,
        meshes: &mut *meshes,
        materials: &mut *materials,
        components: &mut gfx_components,
        material_handles: &mut material_handles,
        camera_view: camera.0,
        camera_transform: *camera.1,
        camera: &mut camera.2,
        window: windows.get_single().ok(),
        keys: &keys,
        mouse: &mouse,
    };

    let mut plugin_args = DimensifyPluginDrawArgs {
        graphics: graphics_context,
        state: &mut state,
        harness: &mut harness,
    };

    for plugin in &mut plugins.0 {
        plugin.draw(&mut plugin_args);
    }

    if state.running == RunMode::Step {
        state.running = RunMode::Stop;
    }
}

pub(crate) fn clear(
    commands: &mut Commands,
    graphics: &mut GraphicsManager,
    plugins: &mut Plugins,
) {
    graphics.clear(commands);

    for plugin in plugins.0.iter_mut() {
        plugin.clear_graphics(graphics, commands);
    }
}
