use bevy_2d_line::LineRenderingPlugin;

// #[derive(States, Debug, Clone, PartialEq, Eq, Hash)]
// enum SketchState {
//     None,
//     Sketching,
// }

#[derive(Resource)]
struct SketchingLine {
    entity: Entity,
    points: Vec<Vec2>,
}

pub fn plugin(app: &mut App) {
    app.add_plugins(LineRenderingPlugin)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            my_cursor_system.run_if(resource_exists::<SketchingLine>),
        )
        .add_systems(Update, mouse_click_event);
}

fn mouse_click_event(
    buttons: Res<ButtonInput<MouseButton>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut commands: Commands,

    mut materials: ResMut<Assets<ColorMaterial>>,
    // query to get the window (so we can read the current cursor position)
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform), With<WindowOverlayCamera>>,
) {
    if buttons.just_pressed(MouseButton::Left) {
        // Left button was pressed

        let entity = commands
            .spawn(Line {
                points: Vec::new(),
                colors: Vec::new(),
                thickness: 9.0,
            })
            .id();
        commands.insert_resource::<SketchingLine>(SketchingLine {
            entity,
            points: vec![],
        });

        // get the camera info and transform
        // assuming there is exactly one main camera entity, so Query::single() is OK
        let (camera, camera_transform) = q_camera.single();

        // There is only one primary window, so we can similarly get it from the query:
        let window = q_window.single();

        dbg!(window.cursor_position());

        // check if the cursor is inside the window and get its position
        // then, ask bevy to convert into world coordinates, and truncate to discard Z
        if let Some(world_position) = window
            .cursor_position()
            .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor))
            .map(|ray| ray.origin.truncate())
        {
            // Circle mesh
            commands.spawn(MaterialMesh2dBundle {
                mesh: meshes.add(Circle::new(18.)).into(),
                // 4. Put something bright in a dark environment to see the effect
                material: materials.add(Color::srgb(7.5, 0.0, 7.5)),
                transform: Transform::from_translation(Vec3::new(
                    world_position.x,
                    world_position.y,
                    1.,
                )),
                ..default()
            });
        }
    }
    if buttons.just_released(MouseButton::Left) {
        // Left Button was released
        commands.remove_resource::<SketchingLine>();
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
    render::{camera::CameraOutputMode, render_resource::BlendState},
    sprite::MaterialMesh2dBundle,
};
use bevy_2d_line::Line;

fn setup(
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
                clear_color: ClearColorConfig::None,
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
    commands.spawn(MaterialMesh2dBundle {
        mesh: meshes.add(Rectangle::default()).into(),
        transform: Transform::default().with_scale(Vec3::splat(128.)),
        material: materials.add(Color::from(LinearRgba::RED)),
        ..default()
    });
}

use bevy::{
    color::palettes::css::{BLUE, GREEN, PURPLE, RED, YELLOW},
    math::VectorSpace,
    prelude::*,
};

use bevy::window::PrimaryWindow;

#[derive(Component)]
struct WindowOverlayCamera;

fn my_cursor_system(
    line_storage: Res<SketchingLine>,
    mut lines: Query<&mut Line>,
    // query to get the window (so we can read the current cursor position)
    q_window: Query<&Window, With<PrimaryWindow>>,
    // query to get camera transform
    q_camera: Query<(&Camera, &GlobalTransform), With<WindowOverlayCamera>>,
) {
    if let Ok(mut line) = lines.get_mut(line_storage.entity) {
        // get the camera info and transform
        // assuming there is exactly one main camera entity, so Query::single() is OK
        let (camera, camera_transform) = q_camera.single();

        // There is only one primary window, so we can similarly get it from the query:
        let window = q_window.single();

        dbg!(window.cursor_position());

        // check if the cursor is inside the window and get its position
        // then, ask bevy to convert into world coordinates, and truncate to discard Z
        if let Some(world_position) = window
            .cursor_position()
            .and_then(|cursor| camera.viewport_to_world(camera_transform, cursor))
            .map(|ray| ray.origin.truncate())
        {
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
                // line.colors.push(LinearRgba::RED);

                line.colors = generate_gradient_vec(
                    vec![LinearRgba::RED, LinearRgba::GREEN, LinearRgba::BLUE],
                    line.points.len(),
                );
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
