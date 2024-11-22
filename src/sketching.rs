use std::ops::{Add, Div, Mul, Sub};

use bevy::ecs::world;
use bevy::pbr::NotShadowCaster;
use bevy::transform::commands;
use bevy_2d_line::LineRenderingPlugin;

// #[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
// enum SketchState {
//     None,
//     Sketching,
// }

use bevy_mod_raycast::cursor::{CursorRay, CursorRayPlugin};
use bevy_mod_raycast::prelude::Raycast;
use nalgebra::{vector, UnitVector3, Vector3};
use splines::{impl_Interpolate, key, Interpolation, Key, Spline};

#[derive(Debug)]
struct Sketch {
    vertices: Vec<Ray3d>,
}

const SHOW_DEBUG_RAYCAST: bool = false;

/////
/////
/////
/////

fn lerp_ray3d(ray: &Ray3d, other: &Ray3d, t: f32) -> Ray3d {
    Ray3d {
        origin: ray.origin.lerp(other.origin, t),
        direction: Dir3::new(ray.direction.lerp(*other.direction, t)).unwrap(),
    }
}

#[derive(Debug, Clone)]
pub struct LerpRay3dList {
    rays: Vec<Ray3d>,
}

impl LerpRay3dList {
    // Constructor to create LerpRay3dList from a vector of Ray3d
    pub fn new(rays: Vec<Ray3d>) -> Self {
        assert!(rays.len() >= 2, "The list must contain at least two rays.");
        LerpRay3dList { rays }
    }

    // Sample a point in the list based on t (0.0 to 1.0)
    pub fn sample(&self, t: f32) -> Ray3d {
        assert!(t >= 0.0 && t <= 1.0, "t must be between 0.0 and 1.0.");

        let num_rays = self.rays.len();
        if t == 0.0 {
            return self.rays[0]; // Return the first ray for t == 0.0
        } else if t == 1.0 {
            return self.rays[num_rays - 1]; // Return the last ray for t == 1.0
        }

        // For intermediate values, we perform linear interpolation
        let index = (t * (num_rays as f32 - 1.0)).floor() as usize;
        let next_index = (index + 1).min(num_rays - 1);

        let lerp_t = (t * (num_rays as f32 - 1.0)) - index as f32;
        lerp_ray3d(&self.rays[index], &self.rays[next_index], lerp_t)
    }
}

/// Function to calculate ray intersection
fn ray_intersection(ray1: Ray3d, ray2: Ray3d, enforce_positive_dir: bool) -> Option<Vec3> {
    let cross_product = ray1.direction.cross(*ray2.direction);

    // If the directions are parallel (cross product is zero), the rays do not intersected_line
    let cross_product_norm_squared = cross_product.norm_squared();
    if cross_product_norm_squared < 1e-6 {
        return None;
    }

    let origin_diff = ray2.origin - ray1.origin;

    // Calculate the parameters t and s that minimize the distance
    let t = origin_diff.cross(*ray2.direction).dot(cross_product) / cross_product_norm_squared;
    let s = origin_diff.cross(*ray1.direction).dot(cross_product) / cross_product_norm_squared;

    if enforce_positive_dir && (t < 0.0 || s < 0.0) {
        return None;
    }

    // Calculate the closest points on the two rays
    let closest_point_on_ray1 = ray1.origin + t * ray1.direction; // Convert UnitVector to Vector3
    let closest_point_on_ray2 = ray2.origin + s * ray2.direction; // Convert UnitVector to Vector3

    // The intersection point is the average of the closest points on both rays
    Some((closest_point_on_ray1 + closest_point_on_ray2) / 2.0)
}

impl Sketch {
    fn new() -> Self {
        Self {
            vertices: Default::default(),
        }
    }

    fn add_vertex(&mut self, vertex: Ray3d) {
        self.vertices.push(vertex);
    }

    fn get_bevy_vertices(&self) -> Vec<Vec3> {
        self.vertices.iter().map(|v| v.origin).collect()
    }

    fn len(&self) -> usize {
        self.vertices.len()
    }

    fn step_size(length: usize) -> f32 {
        1. / (length - 1) as f32
    }

    fn to_spline(&self) -> LerpRay3dList {
        assert!(self.len() >= 2);

        let step_size = Self::step_size(self.len());

        LerpRay3dList::new(self.vertices.clone())

        // create a spline from the vertices with mapping from 0. to 1.
        // Spline::from_vec(
        //     self.vertices
        //         .iter()
        //         .enumerate()
        //         .map(|(i, vertex)| {
        //             Key::new(
        //                 i as f32 * step_size,
        //                 BevySplineVec3(vertex.origin),
        //                 Interpolation::Linear,
        //             )
        //         })
        //         .collect(),
        // )
    }

    fn intersected_line(&self, other: &Self) -> Vec<Vec3> {
        let spline1 = self.to_spline();
        let spline2 = other.to_spline();

        // get the longer length of the two splines
        let max_size = std::cmp::max(self.len(), other.len());
        let step_size = Self::step_size(max_size);

        let mut combined_vertices = Vec::with_capacity(max_size);
        for i in 0..max_size {
            let t = i as f32 * step_size;
            let point1 = spline1.sample(t);
            let point2 = spline2.sample(t);

            if let Some(v) = ray_intersection(point1, point2, true) {
                combined_vertices.push(v);
            }
        }

        combined_vertices

        // FIXME this should not have direction.
        // Some(Self {
        //     vertices: combined_vertices,
        //     direction: self.direction,
        // })
    }
}

/// Represents a line that is currently being sketched by the user.
#[derive(Resource)]
struct ActiveSketchingLine {
    line2d_entity: Entity,
}

#[derive(Resource, Default, Debug)]
struct Sketched3dLines {
    lines: Vec<Sketch>,
}

#[derive(Event, Clone, Debug)]
struct ScreenshotTaken {
    img: Image,
    transform: GlobalTransform,
}

#[derive(Component)]
struct SketchingEndPoint;

pub fn plugin(app: &mut App) {
    app.add_plugins(LineRenderingPlugin)
        .add_plugins(PolylinePlugin)
        .add_systems(Startup, setup_2d_cam)
        .add_systems(
            Update,
            my_cursor_system.run_if(resource_exists::<ActiveSketchingLine>),
        )
        .add_systems(Update, mouse_click_event)
        //    .add_plugins(CursorRayPlugin)
        //     .add_systems(Update, |            cursor_ray: Res<CursorRay>, mut raycast: Raycast, mut gizmos: Gizmos,| {
        //         if let Some(cursor_ray) = **cursor_ray {
        //             raycast.debug_cast_ray(cursor_ray, &default(), &mut gizmos);
        //         }
        //     })
        .add_crossbeam_event::<ScreenshotTaken>()
        .add_systems(Update, handle_screenshot_taken_event)
        .init_resource::<Sketched3dLines>();
}

use bevy::render::view::screenshot::ScreenshotManager;

// ScreenshotReceiver

/// when receiving a screenshot event, spawn a new entity with the screenshot as a texture
fn handle_screenshot_taken_event(
    mut screenshot_receiver: EventReader<ScreenshotTaken>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sketch_line_parts: Query<Entity, Or<(With<Line>, With<SketchingEndPoint>)>>,
    // sketch_lines: Query<Entity, With<Line>>,
    mut q_overlay_cam: Query<&mut Camera, With<WindowOverlayCamera>>,

    mut polyline_materials: ResMut<Assets<PolylineMaterial>>,
    mut polylines: ResMut<Assets<Polyline>>,
) {
    for event in screenshot_receiver.read() {
        // basic rectangle mesh for the image
        let base_img_width = 0.5;
        let aspect = event.img.height() as f32 / event.img.width() as f32;

        // spawn the entity with the image as a texture
        commands
            .spawn(PbrBundle {
                mesh: meshes.add(Rectangle::new(base_img_width, base_img_width * aspect)),
                // mesh: meshes.add(Cuboid::new(0.5, 0.35, 0.05).mesh()),
                material: materials.add(StandardMaterial {
                    base_color: Color::srgba(0.8, 0.8, 0.98, 0.8),
                    // base_color: Color::srgba(0.3, 0.3, 0.3, 0.8),
                    base_color_texture: Some(images.add(event.img.clone())),
                    alpha_mode: AlphaMode::Blend,
                    // Remove this if you want it to use the world's lighting.
                    unlit: true,
                    ..default()
                }),
                ..default()
            })
            .insert(Name::new("Screenshot mesh"))
            .insert(dbg!(event.transform.compute_transform()))
            // .insert(NotShadowCaster)
            ;

        ////////////////////
        // clear the on-screen sketch by despawning them
        dbg!("______________-despawning");
        for entity in sketch_line_parts.iter() {
            dbg!("despawning", entity);
            commands.entity(entity).despawn();
        }

        ////////////////////
        // commands.spawn(PolylineBundle {
        //     polyline: polylines.add(Polyline {
        //         vertices: vec![-Vec3::ONE, Vec3::ONE],
        //         // vertices: Vec::with_capacity(31),
        //     }),
        //     material: polyline_materials.add(PolylineMaterial {
        //         width: (1.),
        //         color: Color::hsl(0.5, 0.2, 0.3).to_linear(),
        //         perspective: true,
        //         ..Default::default()
        //     }),
        //     ..Default::default()
        // });
    }
}

use bevy::math::NormedVectorSpace;

fn raycast(cursor_ray: Res<CursorRay>, mut raycast: Raycast, mut gizmos: Gizmos) {
    if let Some(cursor_ray) = **cursor_ray {
        raycast.debug_cast_ray(cursor_ray, &default(), &mut gizmos);
    }
}

fn mouse_click_event(
    // cursor_ray: Res<CursorRay>, mut raycast: Raycast, mut gizmos: Gizmos,
    //     if let Some(cursor_ray) = **cursor_ray {
    //         raycast.debug_cast_ray(cursor_ray, &default(), &mut gizmos);
    //     }
    mut line_storage: ResMut<Sketched3dLines>,
    screenshot_event_sender: Res<CrossbeamEventSender<ScreenshotTaken>>,

    buttons: Res<ButtonInput<MouseButton>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,

    mut materials: ResMut<Assets<ColorMaterial>>,

    mut alt_pressed: Local<bool>,

    mut keyboard_input_events: EventReader<KeyboardInput>,

    mut q_main_camera: Query<
        (&mut Camera, &mut PanOrbitCamera, &GlobalTransform),
        (With<MainCamera>, Without<WindowOverlayCamera>),
    >,

    // query to get the window (so we can read the current cursor position)
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    mut q_overlay_cam: Query<(&mut Camera, &GlobalTransform), With<WindowOverlayCamera>>,

    mut screenshot_manager: ResMut<ScreenshotManager>,
    main_window: Query<Entity, With<PrimaryWindow>>,

    mut polyline_materials: ResMut<Assets<PolylineMaterial>>,
    mut polylines: ResMut<Assets<Polyline>>,
) {
    // for mut cam in q_overlay_cam.iter_mut() {
    //     dbg!(&cam);
    //     cam.0.is_active = true;
    // }

    // if let Some(cursor_ray) = **cursor_ray {
    //     raycast.debug_cast_ray(cursor_ray, &default(), &mut gizmos);
    // }

    // get the camera info and transform
    // assuming there is exactly one main camera entity, so Query::single() is OK
    let (mut overlay_camera, camera_transform): (_, _) = q_overlay_cam.single_mut();

    let (mut main_camera, mut panorb_cam, main_camera_transform) = q_main_camera.single_mut();

    for event in keyboard_input_events.read() {
        // if event.state != ButtonState::Pressed {
        //     continue;
        // }

        info!("{:?}", event);

        match event.key_code {
            KeyCode::AltLeft => {
                match event.state {
                    ButtonState::Pressed => {
                        *alt_pressed = true;
                        // FIXME use bevy state.
                        panorb_cam.enabled = false;

                        //         screenshot_manager
                        //                     .take_screenshot(main_window.single(), |image| {

                        // polyline_materials;

                        //                         // dbg!(image);

                        //                     });
                        // .save_screenshot_to_disk(, path)
                        // .unwrap();
                    }
                    ButtonState::Released => {
                        *alt_pressed = false;
                        panorb_cam.enabled = true;
                    }
                };
            }
            KeyCode::Enter => {
                if event.state != ButtonState::Pressed {
                    continue;
                }

                let screenshot_event_sender = screenshot_event_sender.clone();

                if let Some(mut line) = line_storage.lines.last_mut() {
                    // if let Some(mut line) = line_storage.lines.pop() {
                    let camera_transform = *main_camera_transform;
                    screenshot_manager
                        .take_screenshot(main_window.single(), move |image| {
                            screenshot_event_sender.send(ScreenshotTaken {
                                transform: camera_transform,
                                img: image,
                            });
                        })
                        .expect("cannot take screenshot");

                    // sketching_line.points

                    if SHOW_DEBUG_RAYCAST {
                        for ray in line.vertices.iter() {
                            commands.spawn(PolylineBundle {
                                polyline: polylines.add(Polyline {
                                    // FIXME no clone, just pop it
                                    vertices: vec![ray.origin, ray.origin + ray.direction * 500.1],
                                    // vertices: vec![-Vec3::ONE, Vec3::ONE],

                                    // vertices: Vec::with_capacity(31),
                                }),
                                material: polyline_materials.add(PolylineMaterial {
                                    width: 10.,
                                    // color: Color::srgb(0.8, 0.2, 0.3).to_linear(),
                                    color: Color::hsl(0.5, 0.2, 0.3).to_linear(),

                                    perspective: true,
                                    ..Default::default()
                                }),
                                ..Default::default()
                            });
                        }
                    }

                    // commands.spawn(PolylineBundle {
                    //     polyline: polylines.add(Polyline {
                    //         // FIXME no clone, just pop it
                    //         vertices: line.get_bevy_vertices(),
                    //         // vertices: vec![-Vec3::ONE, Vec3::ONE],

                    //         // vertices: Vec::with_capacity(31),
                    //     }),
                    //     material: polyline_materials.add(PolylineMaterial {
                    //         width: (2.),
                    //         color: Color::hsl(0.5, 0.2, 0.3).to_linear(),
                    //         perspective: true,
                    //         ..Default::default()
                    //     }),
                    //     ..Default::default()
                    // });
                }
            }
            KeyCode::Backspace => {
                if line_storage.lines.len() >= 2 {
                    dbg!(&line_storage.lines);

                    // intersected_line()

                    let n = line_storage.lines.len();

                    let new_line =
                        line_storage.lines[n - 2].intersected_line(&line_storage.lines[n - 1]);

                    commands.spawn((PolylineBundle {
                        polyline: polylines.add(Polyline {
                            // FIXME no clone, just pop it
                            vertices: new_line,
                        }),
                        material: polyline_materials.add(PolylineMaterial {
                            width: (32.),
                            color: Color::srgb(0.5, 0.2, 0.9).to_linear(),
                            perspective: true,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },

                    Name::new("Combined 3d line")
                )
                );
                }
            }
            _ => (),
        }

        // break;
    }

    ////////////////

    if *alt_pressed && buttons.just_pressed(MouseButton::Left) {
        // Left button was pressed

        // // ensure that overlay overlay_camera has clearcolor set to none
        // overlay_camera.clear_color = ClearColorConfig::None;

        let entity = commands
            .spawn(Line {
                points: Vec::new(),
                colors: Vec::new(),
                thickness: 9.0,
            })
            .id();
        commands.insert_resource::<ActiveSketchingLine>(ActiveSketchingLine {
            line2d_entity: entity,
        });

        // There is only one primary window, so we can similarly get it from the query:
        let window = q_window.single();

        dbg!(window.cursor_position());

        // check if the cursor is inside the window and get its position
        // then, ask bevy to convert into world coordinates, and truncate to discard Z
        if let Some(world_position) = window
            .cursor_position()
            .and_then(|cursor| overlay_camera.viewport_to_world(camera_transform, cursor))
            .map(|ray| ray.origin.truncate())
        {
            // Circle mesh (start point)
            commands
                .spawn(MaterialMesh2dBundle {
                    mesh: meshes.add(Circle::new(18.)).into(),
                    // 4. Put something bright in a dark environment to see the effect
                    material: materials.add(Color::srgb(7.5, 0.0, 7.5)),
                    transform: Transform::from_translation(Vec3::new(
                        world_position.x,
                        world_position.y,
                        1.,
                    )),
                    ..default()
                })
                .insert(SketchingEndPoint);
        }

        // create the storage
        line_storage.lines.push(Sketch::new());
    }
    if *alt_pressed &&  buttons.just_released(MouseButton::Left) {
        // Left Button was released
        commands.remove_resource::<ActiveSketchingLine>();

        let window = q_window.single();


        if let Some(world_position) = window
            .cursor_position()
            .and_then(|cursor| overlay_camera.viewport_to_world(camera_transform, cursor))
            .map(|ray| ray.origin.truncate())
        {
            // Circle mesh (end point)
            commands
                .spawn(MaterialMesh2dBundle {
                    mesh: meshes.add(Circle::new(18.)).into(),
                    // 4. Put something bright in a dark environment to see the effect
                    material: materials.add(Color::srgb(0.5, 7.5, 7.5)),
                    transform: Transform::from_translation(Vec3::new(
                        world_position.x,
                        world_position.y,
                        1.,
                    )),
                    ..default()
                })
                .insert(SketchingEndPoint);
        }
    }
    if buttons.pressed(MouseButton::Right) {
        // Right Button is being held down
    }
    // we can check multiple at once with `.any_*`
    if buttons.any_just_pressed([MouseButton::Left, MouseButton::Middle]) {
        // Either the left or the middle (wheel) button was just pressed
    }
}

use bevy::{
    core_pipeline::{
        bloom::{BloomCompositeMode, BloomSettings},
        tonemapping::Tonemapping,
    },
    input::{keyboard::KeyboardInput, ButtonState},
    render::{camera::CameraOutputMode, render_resource::BlendState},
    sprite::MaterialMesh2dBundle,
};
use bevy_2d_line::Line;

fn setup_2d_cam(
    mut commands: Commands,

    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Camera2dBundle {
            camera: Camera {
                hdr: true, // 1. HDR is required for bloom
                // name: Some("2d".to_string()),
                order: 1,
                // TODO look into https://github.com/bevyengine/bevy/pull/13419
                // clear_color: ClearColorConfig::None,
                ///////////
                // NOTE:
                // With ClearColorConfig::None the render target is not cleared, the only pixel that will be modified are the pixels that are effectively rendered.
                // With ClearColorConfig::Custom(Color::NONE) the render target is first cleared with a transparent pixel color, then the rendered pixel are added.
                // see https://github.com/bevyengine/bevy/issues/11844#issuecomment-1943534040
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ///////////
                output_mode: CameraOutputMode::Write {
                    blend_state: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    // blend_state: Some(BlendState{

                    //     color: BlendComponent::OVER,
                    //     // color: BlendComponent {
                    //     //     src_factor: BlendFactor::One,
                    //     //     dst_factor: BlendFactor::One,
                    //     //     operation: BlendOperation::Add,
                    //     // },
                    //     alpha: BlendComponent::OVER,
                    // }),
                    // blend_state: Some(BlendState::ALPHA_BLENDING),
                    // blend_state: None,
                    clear_color: ClearColorConfig::None,
                    // color_attachment_load_op: LoadOp::Load,
                    // clear_color: todo!(),
                },
                ..Default::default()
            },
            tonemapping: Tonemapping::TonyMcMapface, // 2. Using a tonemapper that desaturates to white is recommended
            ..default()
        },
        BloomSettings::default(), // 3. Enable bloom for the camera
        WindowOverlayCamera,
    ));
    // commands.spawn(MaterialMesh2dBundle {
    //     mesh: meshes.add(Rectangle::default()).into(),
    //     transform: Transform::default().with_scale(Vec3::splat(128.)),
    //     material: materials.add(Color::from(LinearRgba::RED)),
    //     ..default()
    // });
}

use bevy::{
    color::palettes::css::{BLUE, GREEN, PURPLE, RED, YELLOW},
    math::VectorSpace,
    prelude::*,
};

use bevy::window::PrimaryWindow;
use bevy_crossbeam_event::{CrossbeamEventApp, CrossbeamEventSender};
use bevy_panorbit_camera::PanOrbitCamera;
use bevy_polyline::{
    prelude::{Polyline, PolylineBundle, PolylineMaterial},
    PolylinePlugin,
};

use crate::camera::main_camera::MainCamera;

#[derive(Component)]
struct WindowOverlayCamera;

fn my_cursor_system(
    mut sketching_line: Res<ActiveSketchingLine>,
    mut line_storage: ResMut<Sketched3dLines>,
    mut lines: Query<&mut Line>,
    // query to get the window (so we can read the current cursor position)
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    q_overlay_cam: Query<(&Camera, &GlobalTransform), With<WindowOverlayCamera>>,

    mut commands: Commands,
    mut polyline_materials: ResMut<Assets<PolylineMaterial>>,
    mut polylines: ResMut<Assets<Polyline>>,

    mut q_main_camera: Query<
        (&mut Camera, &mut PanOrbitCamera, &GlobalTransform),
        (With<MainCamera>, Without<WindowOverlayCamera>),
    >,
) {
    if let Ok(mut line) = lines.get_mut(sketching_line.line2d_entity) {
        // get the camera info and transform
        // assuming there is exactly one main camera entity, so Query::single() is OK
        let (camera, camera_transform) = q_overlay_cam.single();

        let (mut main_camera, mut panorb_cam, main_camera_transform) = q_main_camera.single_mut();

        // There is only one primary window, so we can similarly get it from the query:
        let window = q_window.single();

        dbg!(window.cursor_position());

        // check if the cursor is inside the window and get its position
        // then, ask bevy to convert into world coordinates, and truncate to discard Z
        if let Some(cursor) = window.cursor_position() {
            if let Some(world_position) = camera
                .viewport_to_world(camera_transform, cursor)
                .map(|ray| ray.origin.truncate())
            {
                // // let mut viewport_pos = cursor_pos_screen;
                // if let Some(viewport) = &camera.viewport {
                //     viewport_pos -= viewport.physical_position.as_vec2() / window.scale_factor();
                // }
                // camera
                //     .viewport_to_world(camera_transform, viewport_pos)
                //     .map(Ray3d::from);

                pub fn ray_from_screenspace(
                    cursor_pos_screen: Vec2,
                    camera: &Camera,
                    camera_transform: &GlobalTransform,
                    window: &Window,
                ) -> Option<Ray3d> {
                    let mut viewport_pos = cursor_pos_screen;
                    if let Some(viewport) = &camera.viewport {
                        viewport_pos -=
                            viewport.physical_position.as_vec2() / window.scale_factor();
                    }
                    camera
                        .viewport_to_world(camera_transform, viewport_pos)
                        .map(Ray3d::from)
                }

                let r = ray_from_screenspace(
                    cursor,
                    main_camera.as_ref(),
                    main_camera_transform,
                    window,
                )
                .unwrap();


                // only push if differences is big or this is the first point
                let should_push = match line.points.last() {
                    Some(last_point) => (*last_point - world_position).length() > 2.0,
                    None => {
                        // this is the first point, do something special.

                        true
                    }
                };

                if should_push {
                    // mycoords.0 = world_position;
                    eprintln!("World coords: {}/{}", world_position.x, world_position.y);

                    line.points.push(world_position);
                    // line.points.push(world_position);
                    // line.colors.push(LinearRgba::RED);

                    line.colors = generate_gradient_vec(
                        vec![LinearRgba::RED, LinearRgba::GREEN, LinearRgba::BLUE],
                        line.points.len(),
                    );

                    let window = q_window.single();

                    dbg!("......................world_position");
                    let line_storage = line_storage.into_inner();

                    if let Some(world_position) = main_camera
                        .viewport_to_world(main_camera_transform, cursor)
                        .map(|ray| ray.origin)
                    {
                        dbg!(&world_position, camera_transform.forward());
                        line_storage
                            .lines
                            .last_mut()
                            .expect("no active line")
                            .add_vertex(r);
                        // .add_vertex(world_position);
                    }
                }
            }
        }
    }
}

/// Generate a gradient between a list of colors
fn generate_gradient_vec(input_colors: Vec<LinearRgba>, steps: usize) -> Vec<LinearRgba> {
    let mut colors = Vec::with_capacity(steps);

    if input_colors.len() < 2 {
        panic!("Colors must have at least 2 elements");
    }

    let mut input_colors_at_i = Vec::with_capacity(input_colors.len());
    input_colors_at_i.push(0);
    for i in 1..input_colors.len() {
        input_colors_at_i
            .push(((i as f32 / (input_colors.len() - 1) as f32) * steps as f32).ceil() as usize);
    }
    dbg!(&input_colors_at_i, steps);

    for c_idx in 1..input_colors.len() {
        let range = input_colors_at_i[c_idx] - input_colors_at_i[c_idx - 1];
        for i in 0..range {
            let t = i as f32 / (range - 1) as f32;

            colors.push(input_colors[c_idx - 1].lerp(input_colors[c_idx], t));
        }
    }

    colors
}

fn generate_gradient(
    start_color: LinearRgba,
    end_color: LinearRgba,
    steps: usize,
) -> Vec<LinearRgba> {
    let mut colors = Vec::with_capacity(steps);
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        colors.push(start_color.lerp(end_color, t));
    }
    colors
}
