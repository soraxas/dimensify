use bevy_2d_line::LineRenderingPlugin;

// #[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
// enum SketchState {
//     None,
//     Sketching,
// }

use crate::util::ray_intersection;
use crate::util::traits::LinearParameterisedTrait;

#[derive(Debug)]
struct Sketch {
    vertices: Vec<Ray3d>,
}

const SHOW_DEBUG_RAYCAST: bool = false;

#[derive(Component)]
struct LineTargetTransition {
    // Should contains same number of points as the line
    target_line_pos: Vec<Vec3>,
    polyline_handle: Handle<Polyline>,
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

    fn len(&self) -> usize {
        self.vertices.len()
    }

    fn step_size(length: usize) -> f32 {
        1. / (length - 1) as f32
    }

    /// Returns the intersecting line of this line with another line
    fn intersected_line_with_sampled(&self, other: &Self) -> (Vec<Vec3>, Vec<Vec3>, Vec<Vec3>) {
        // get the longer length of the two splines
        let max_size = std::cmp::max(self.len(), other.len());
        let step_size = Self::step_size(max_size);

        let mut vec1 = Vec::with_capacity(max_size);
        let mut vec2 = Vec::with_capacity(max_size);
        let mut combined_vertices = Vec::with_capacity(max_size);
        for i in 0..max_size {
            let t = i as f32 * step_size;
            let ray1 = self.vertices.sample(t);
            let ray2 = other.vertices.sample(t);

            if let Some(v) = ray_intersection(ray1, ray2, true) {
                vec1.push(ray1.origin);
                vec2.push(ray2.origin);
                combined_vertices.push(v);
            }
        }

        (combined_vertices, vec1, vec2)

        // FIXME this should not have direction.
        // Some(Self {
        //     vertices: combined_vertices,
        //     direction: self.direction,
        // })
    }

    /// Returns the intersecting line of the two lines
    fn intersected_line(&self, other: &Self) -> Vec<Vec3> {
        self.intersected_line_with_sampled(other).0
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

#[derive(Component)]
struct OnScreenSketching;

pub fn plugin(app: &mut App) {
    app.add_plugins(LineRenderingPlugin)
        .add_plugins(PolylinePlugin)
        .add_systems(Startup, setup_2d_cam)
        .add_systems(
            Update,
            my_cursor_system.run_if(resource_exists::<ActiveSketchingLine>),
        )
        .add_systems(Update, mouse_click_event)
        .add_systems(Update, line_transition_to_target)
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

fn line_transition_to_target(
    time: Res<Time>,
    mut commands: Commands,
    mut polylines: ResMut<Assets<Polyline>>,

    mut lines_with_transition: Query<(Entity, &LineTargetTransition)>,
) {
    for (e, line_transition) in lines_with_transition.iter_mut() {
        let line = polylines.get_mut(&line_transition.polyline_handle).unwrap();

        assert!(line.vertices.len() == line_transition.target_line_pos.len());

        let mut had_transition = false;
        for i in 0..line.vertices.len() {
            let target_pos = line_transition.target_line_pos[i];
            let current_pos = line.vertices[i];

            let diff = target_pos - current_pos;
            let movement = f32::min(1e-5, diff.length());
            // let diff_len = 1.;
            // let diff_len = diff.length();

            if movement > 1e-6 {
                let new_pos = current_pos + diff * time.delta_seconds();
                line.vertices[i] = new_pos;

                had_transition = true;
            }
        }

        if !had_transition {
            // remove the transition component
            commands.entity(e).despawn();
            polylines.remove(&line_transition.polyline_handle);
        }
    }
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
        for entity in sketch_line_parts.iter() {
            commands.entity(entity).despawn();
        }
    }
}

fn mouse_click_event(
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

    q_sktech: Query<Entity, With<OnScreenSketching>>,
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
    let (overlay_camera, camera_transform): (_, _) = q_overlay_cam.single_mut();

    let (main_camera, mut panorb_cam, main_camera_transform) = q_main_camera.single_mut();

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

                if let Some(line) = line_storage.lines.last_mut() {
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
                if event.state != ButtonState::Pressed {
                    continue;
                }
                if line_storage.lines.len() >= 2 {
                    let line2 = line_storage.lines.pop().unwrap();
                    let line1 = line_storage.lines.pop().unwrap();

                    let (new_line, sampled_line1, sampled_line2) =
                        line1.intersected_line_with_sampled(&line2);

                    for sampled_line in [sampled_line1, sampled_line2] {
                        // this line is the normalised line (same length as both lines)
                        let polyline_handle = polylines.add(Polyline {
                            vertices: sampled_line,
                        });

                        commands.spawn((
                            PolylineBundle {
                                polyline: polyline_handle.clone(),
                                material: polyline_materials.add(PolylineMaterial {
                                    width: (32.),
                                    color: Color::srgb(0.5, 0.2, 0.9).to_linear(),
                                    perspective: true,
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                            Name::new("Combined 3d line"),
                        ));

                        // This line transition will animate the line to the intersecting line
                        commands.spawn(LineTargetTransition {
                            target_line_pos: new_line.clone(),
                            polyline_handle,
                        });
                    }
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

        //////////////////////////
        // remove all existing on-screen sketching lines
        for entity in q_sktech.iter() {
            commands.entity(entity).despawn_recursive();
        }
        // remove the active sketching line as well
        line_storage.lines.clear();
        //////////////////////////

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
            commands.spawn(OnScreenSketching).with_children(|parent| {
                parent
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
            });
        }

        // create the storage
        line_storage.lines.push(Sketch::new());
    }
    if *alt_pressed && buttons.just_released(MouseButton::Left) {
        // Left Button was released
        commands.remove_resource::<ActiveSketchingLine>();

        let window = q_window.single();

        if let Some(world_position) = window
            .cursor_position()
            .and_then(|cursor| overlay_camera.viewport_to_world(camera_transform, cursor))
            .map(|ray| ray.origin.truncate())
        {
            commands.entity(q_sktech.single()).with_children(|parent| {
                // Circle mesh (end point)
                parent
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
            });
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
    core_pipeline::{bloom::BloomSettings, tonemapping::Tonemapping},
    input::{keyboard::KeyboardInput, ButtonState},
    render::{camera::CameraOutputMode, render_resource::BlendState},
    sprite::MaterialMesh2dBundle,
};
use bevy_2d_line::Line;

fn setup_2d_cam(
    mut commands: Commands,

    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<ColorMaterial>>,
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

use bevy::{math::VectorSpace, prelude::*};

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
    sketching_line: Res<ActiveSketchingLine>,
    mut line_storage: ResMut<Sketched3dLines>,
    mut lines: Query<&mut Line>,
    // query to get the window (so we can read the current cursor position)
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    q_overlay_cam: Query<(&Camera, &GlobalTransform), With<WindowOverlayCamera>>,

    mut q_main_camera: Query<
        (&mut Camera, &mut PanOrbitCamera, &GlobalTransform),
        (With<MainCamera>, Without<WindowOverlayCamera>),
    >,
) {
    if let Ok(mut line) = lines.get_mut(sketching_line.line2d_entity) {
        // get the camera info and transform
        // assuming there is exactly one main camera entity, so Query::single() is OK
        let (camera, camera_transform) = q_overlay_cam.single();

        let (main_camera, panorb_cam, main_camera_transform) = q_main_camera.single_mut();

        // There is only one primary window, so we can similarly get it from the query:
        let window = q_window.single();

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
                    line.points.push(world_position);
                    // line.points.push(world_position);
                    // line.colors.push(LinearRgba::RED);

                    line.colors = generate_gradient_vec(
                        vec![LinearRgba::RED, LinearRgba::GREEN, LinearRgba::BLUE],
                        line.points.len(),
                    );

                    line_storage
                        .lines
                        .last_mut()
                        .expect("no active line")
                        .add_vertex(r);
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

    for c_idx in 1..input_colors.len() {
        let range = input_colors_at_i[c_idx] - input_colors_at_i[c_idx - 1];
        for i in 0..range {
            let t = i as f32 / (range - 1) as f32;

            colors.push(input_colors[c_idx - 1].lerp(input_colors[c_idx], t));
        }
    }

    colors
}
