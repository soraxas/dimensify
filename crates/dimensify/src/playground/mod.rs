//! A small native/WASM consumer of shared navigation and retained Feathers controls.
use bevy::{
    feathers::{controls::FeathersButton, theme::ThemedText},
    picking::hover::Hovered,
    prelude::*,
    ui_widgets::Activate,
};
/// Shared camera APIs used by the example composition.
pub use dimensify_camera as navigation;
use dimensify_camera::{
    CameraPose,
    viewer::{
        CameraAction, NavigationConfig, NavigationMode, ViewerCamera, ViewerNavigationPlugin,
    },
};

/// Example composition with an optional built-in scene; navigation stays shared.
pub struct CameraPlaygroundPlugin {
    pub scene: bool,
}
impl Plugin for CameraPlaygroundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ViewerNavigationPlugin)
            .insert_resource(NavigationMode::Orbit)
            .add_systems(Startup, (camera, toolbar))
            .add_systems(Update, status);
        if self.scene {
            app.add_systems(Startup, scene);
        }
    }
}

/// Run the camera playground with the same native and browser UI.
pub fn run() {
    app("Dimensify camera playground")
        .add_plugins(CameraPlaygroundPlugin { scene: true })
        .run();
}

/// Create the example's native/WASM shell; callers compose its feature plugins.
pub fn app(title: &str) -> App {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.035, 0.05, 0.075)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: title.into(),
                resolution: (1000, 760).into(),
                #[cfg(target_arch = "wasm32")]
                canvas: Some("#dimensify-canvas".into()),
                #[cfg(target_arch = "wasm32")]
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(bevy::feathers::FeathersPlugins)
        .insert_resource(bevy::feathers::theme::UiTheme(
            bevy::feathers::dark_theme::create_dark_theme(),
        ));
    app
}

fn camera(mut commands: Commands, mut config: ResMut<NavigationConfig>) {
    let transform = Transform::from_xyz(0., 1.8, 8.).looking_at(Vec3::ZERO, Vec3::Y);
    config.home = CameraPose::from_rotation(
        transform.translation,
        transform.rotation,
        8.,
        std::f32::consts::FRAC_PI_4,
    );
    config.bounds = Some((Vec3::splat(-2.), Vec3::splat(2.)));
    commands.spawn((
        Camera3d::default(),
        ViewerCamera,
        IsDefaultUiCamera,
        Msaa::Off,
        transform,
    ));
}

fn scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (position, color) in [
        (-2., Color::srgb(0.2, 0.8, 0.65)),
        (0., Color::srgb(0.95, 0.55, 0.2)),
        (2., Color::srgb(0.35, 0.55, 0.95)),
    ] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::default())),
            MeshMaterial3d(materials.add(color)),
            Transform::from_xyz(position, 0., 0.),
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(12., 0.1, 12.))),
        MeshMaterial3d(materials.add(Color::srgb(0.12, 0.16, 0.22))),
        Transform::from_xyz(0., -0.6, 0.),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.,
            ..default()
        },
        Transform::from_xyz(4., 6., 5.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Flow-layout parent for optional example controls, kept above the scene.
#[derive(Component)]
pub struct ExampleChrome;

#[derive(Component)]
struct ModeLabel;
#[derive(Component)]
struct CameraStatus;
fn toolbar(mut commands: Commands) {
    info!("Shared camera playground ready");
    let root = commands
        .spawn((
            ExampleChrome,
            Node {
                position_type: PositionType::Absolute,
                top: px(16),
                left: px(16),
                right: px(16),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                ..default()
            },
        ))
        .id();
    commands
        .spawn((
            ChildOf(root),
            Node {
                padding: UiRect::all(px(8)),
                column_gap: px(8),
                row_gap: px(8),
                flex_wrap: FlexWrap::Wrap,
                ..default()
            },
            BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
            Hovered::default(),
        ))
        .with_children(|bar| {
            for (action, label) in [
                (CameraAction::Frame, "Frame"),
                (CameraAction::Reset, "Reset"),
                (CameraAction::ToggleMode, "Mode: Orbit"),
            ] {
                let parent = bar.target_entity();
                let mode = action == CameraAction::ToggleMode;
                bar.commands_mut()
                    .spawn_scene(
                        bsn! { @FeathersButton Node { min_width: px(100), height: px(44) } },
                    )
                    .insert(ChildOf(parent))
                    .observe(
                        move |_: On<Activate>, mut actions: MessageWriter<CameraAction>| {
                            actions.write(action);
                        },
                    )
                    .with_children(|button| {
                        let mut text = button.spawn((Text::new(label), ThemedText));
                        if mode {
                            text.insert(ModeLabel);
                        }
                    });
            }
        });
    commands.spawn((
        CameraStatus,
        Text::new("Camera playground"),
        TextFont::from_font_size(FontSize::Px(14.)),
        Node {
            position_type: PositionType::Absolute,
            left: px(16),
            right: px(16),
            bottom: px(16),
            padding: UiRect::all(px(12)),
            ..default()
        },
        BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
    ));
}
fn status(
    mode: Res<NavigationMode>,
    cameras: Query<&Transform, With<ViewerCamera>>,
    mut labels: Query<&mut Text, With<ModeLabel>>,
    mut status: Query<&mut Text, (With<CameraStatus>, Without<ModeLabel>)>,
) {
    let mode = *mode;
    if let Ok(mut label) = labels.single_mut() {
        label.0 = format!("Mode: {mode:?}");
    }
    if let (Ok(camera), Ok(mut label)) = (cameras.single(), status.single_mut()) {
        label.0 = format!(
            "{mode:?} | Position {:.2}, {:.2}, {:.2}\nDrag: orbit/look | Right drag: pan | Scroll: zoom/move | WASD + Q/E: fly\nTouch: one finger orbit/look; two fingers pan/pinch | F: frame",
            camera.translation.x, camera.translation.y, camera.translation.z
        );
    }
}
