use super::*;
use bevy::{mesh::VertexAttributeValues, prelude::*, render::view::NoIndirectDrawing};
use dimensify_camera::{
    CameraPose, FitExtent, OrbitState, fit_orbit, scene_bounding_sphere,
    viewer::{
        CameraAction, NavigationConfig, NavigationMode, ViewerCamera, ViewerNavigationPlugin,
    },
};
use renderer::PointCloudVisibilitySettings;

mod settings;

#[derive(Resource)]
struct ViewerSource(LoadPointcloud);
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct FrameWhenLoaded;

/// Run with a generated fixture or `--source <directory-or-url> [--z-up]`.
/// Browser sources use `?source=<url>&up=z`.
pub fn run() {
    run_source(source_options());
}

/// Run a deterministic, asset-free fixture through the shared streaming renderer.
pub fn run_fixture() {
    run_source(LoadPointcloud::mesh(fixture_mesh()));
}

fn source_options() -> LoadPointcloud {
    let mut source = None;
    let mut up = PointcloudUp::Y;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--source" => source = args.next(),
                "--z-up" => up = PointcloudUp::Z,
                "--y-up" => up = PointcloudUp::Y,
                _ => panic!("Usage: --source <Potree-directory-or-URL> [--z-up|--y-up]"),
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()
            .and_then(|window| window.location().search().ok())
            .unwrap_or_default();
        for (key, value) in url::form_urlencoded::parse(search.trim_start_matches('?').as_bytes()) {
            match key.as_ref() {
                "source" => source = Some(value.into_owned()),
                "up" if value == "z" => up = PointcloudUp::Z,
                _ => {}
            }
        }
    }
    let mut request = match source {
        Some(source) if source.starts_with("http://") || source.starts_with("https://") => {
            LoadPointcloud::potree(source)
        }
        Some(source) => LoadPointcloud::new(PointcloudSource::PotreePath(source.into())),
        None => LoadPointcloud::mesh(fixture_mesh()),
    };
    request.up = up;
    request
}

fn fixture_mesh() -> Mesh {
    let mut mesh = Sphere::new(2.).mesh().uv(64, 32);
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        unreachable!()
    };
    let colors: Vec<[f32; 4]> = positions
        .iter()
        .map(|p| [0.25 + (p[0] + 2.) * 0.18, 0.3 + (p[1] + 2.) * 0.15, 0.8, 1.])
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn run_source(source: LoadPointcloud) {
    viewer_app(
        source,
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Dimensify point-cloud viewer".into(),
                resolution: (1000, 760).into(),
                #[cfg(target_arch = "wasm32")]
                canvas: Some("#dimensify-canvas".into()),
                #[cfg(target_arch = "wasm32")]
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }),
    )
    .run();
}

fn viewer_app(source: LoadPointcloud, plugins: impl bevy::app::PluginGroup) -> App {
    let mut app = App::new();
    app.insert_resource(settings::Settings::new(source.settings.splats.clone()))
        .insert_resource(ViewerSource(source))
        .insert_resource(ClearColor(Color::srgb(0.025, 0.035, 0.055)))
        .add_plugins(plugins)
        .add_plugins(bevy::feathers::FeathersPlugins)
        .insert_resource(bevy::feathers::theme::UiTheme(
            bevy::feathers::dark_theme::create_dark_theme(),
        ))
        .add_plugins((PointcloudPlugin::default(), ViewerNavigationPlugin))
        .insert_resource(NavigationMode::Orbit)
        .add_systems(Startup, setup)
        .add_systems(Update, (status_text, keyboard))
        .add_systems(
            PostUpdate,
            frame_loaded
                .after(PointcloudSystems)
                .after(bevy::transform::TransformSystems::Propagate),
        );
    settings::install(&mut app);
    app
}

fn setup(mut commands: Commands, source: Res<ViewerSource>, mut config: ResMut<NavigationConfig>) {
    let transform = Transform::from_xyz(0., 1.5, 8.).looking_at(Vec3::ZERO, Vec3::Y);
    config.home = CameraPose::from_rotation(
        transform.translation,
        transform.rotation,
        8.,
        std::f32::consts::FRAC_PI_4,
    );
    commands.spawn((
        Camera3d::default(),
        ViewerCamera,
        IsDefaultUiCamera,
        Msaa::Off,
        NoIndirectDrawing,
        transform,
        PointCloudVisibilitySettings {
            point_budget: Some(5_000_000),
            min_radius: Some(6.),
            ..default()
        },
    ));
    commands.spawn((Name::new("Point cloud"), source.0.clone(), FrameWhenLoaded));
    commands.spawn((
        StatusText,
        Text::new("Loading point cloud"),
        TextFont::from_font_size(FontSize::Px(16.)),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            right: px(12),
            ..default()
        },
    ));
}

fn frame_loaded(
    mut commands: Commands,
    clouds: Query<(Entity, &PointcloudStatus, &GlobalTransform), With<FrameWhenLoaded>>,
    windows: Query<&Window>,
    mut config: ResMut<NavigationConfig>,
    mut actions: MessageWriter<CameraAction>,
) {
    for (entity, status, transform) in &clouds {
        let Some(bounds) = status.bounds else {
            continue;
        };
        let extent = FitExtent {
            center: bounds.center.into(),
            half_extents: bounds.half_extents.into(),
        };
        let Some((center, radius)) = scene_bounding_sphere([(&extent, transform)]) else {
            continue;
        };
        let viewport = windows
            .iter()
            .next()
            .map_or(Vec2::new(1000., 760.), |w| Vec2::new(w.width(), w.height()));
        let mut orbit = OrbitState::default();
        fit_orbit(&mut orbit, center, radius, viewport);
        config.home = orbit.requested_pose();
        config.bounds = Some((center - Vec3::splat(radius), center + Vec3::splat(radius)));
        actions.write(CameraAction::Reset);
        commands.entity(entity).remove::<FrameWhenLoaded>();
    }
}

fn keyboard(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<NavigationConfig>,
    mut actions: MessageWriter<CameraAction>,
) {
    if config.blocked {
        return;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        actions.write(CameraAction::ToggleMode);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        actions.write(CameraAction::Reset);
    }
}

fn status_text(clouds: Query<&PointcloudStatus>, mut text: Query<&mut Text, With<StatusText>>) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let Some(status) = clouds.iter().next() else {
        return;
    };
    let detail = status.error.clone().unwrap_or_else(|| {
        format!(
            "{:?} | {} active, {} queued",
            status.phase, status.active, status.queued
        )
    });
    text.0 = format!("{detail}\nDrag: orbit | Scroll: zoom | F: frame | M: orbit/fly | R: reset");
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
