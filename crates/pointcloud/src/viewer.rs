use super::*;
use bevy::{
    ecs::system::SystemParam, mesh::VertexAttributeValues, prelude::*,
    render::view::NoIndirectDrawing,
};
use dimensify_camera::{
    CameraPose, FitExtent, OrbitState, fit_orbit, scene_bounding_sphere,
    viewer::{
        CameraAction, DepthPickPlugin, NavigationConfig, NavigationMode, ViewerCamera,
        ViewerNavigationPlugin,
    },
};
use renderer::PointCloudVisibilitySettings;

mod open;
mod settings;
mod ui;
use dimensify_ui::viewer as style;

#[derive(Resource)]
struct ViewerSource(LoadPointcloud);
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct FrameWhenLoaded;
#[derive(Component)]
struct ViewerCloud;

/// Run with a generated fixture or `--source <directory-or-url> [--z-up]`.
/// Browser sources use `?source=<url>&up=z`.
pub fn run() {
    run_source(source_options());
}

/// Run a deterministic, asset-free fixture through the shared streaming renderer.
pub fn run_fixture() {
    run_source(fixture_source());
}

fn fixture_source() -> LoadPointcloud {
    let mut source = LoadPointcloud::mesh(fixture_mesh());
    source.settings.splats.point_size_mode = renderer::PointSizeMode::WorldSpace;
    source.settings.splats.point_size = 0.3;
    source.settings.splats.adaptive_point_size = false;
    source.settings.splats.min_point_size = None;
    source.settings.splats.max_point_size = None;
    source
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
                _ => {
                    panic!("Usage: --source <PLY-file-or-Potree-directory-or-URL> [--z-up|--y-up]")
                }
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
            open::request_url(&source, up).unwrap_or_else(|_| LoadPointcloud::potree(source))
        }
        Some(source)
            if std::path::Path::new(&source)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ply")) =>
        {
            LoadPointcloud::new(PointcloudSource::PlyPath(source.into()))
        }
        Some(source) => LoadPointcloud::new(PointcloudSource::PotreePath(source.into())),
        None => fixture_source(),
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
        .add_plugins((
            PointcloudPlugin::default(),
            ViewerNavigationPlugin,
            DepthPickPlugin,
            style::ViewerUiPlugin,
        ))
        .insert_resource(NavigationMode::Orbit)
        .add_systems(Startup, (setup, ui::setup))
        .add_observer(ui::activate)
        .add_systems(Update, ui::sync)
        .add_systems(Update, (status_text, keyboard))
        .add_systems(
            PostUpdate,
            frame_loaded
                .after(PointcloudSystems)
                .after(bevy::transform::TransformSystems::Propagate),
        );
    #[cfg(target_arch = "wasm32")]
    if web_sys::window()
        .and_then(|window| window.location().search().ok())
        .is_some_and(|search| {
            url::form_urlencoded::parse(search.trim_start_matches('?').as_bytes())
                .any(|(key, value)| key == "ui_check" && value == "1")
        })
    {
        app.add_systems(Last, ui::browser_locations);
    }
    settings::install(&mut app);
    open::install(&mut app);
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
            // Potree spacing is 1/111 of a node's radius at every level, so 55 px stops
            // refining once child points would land about 1 px apart.
            min_radius: Some(55.),
            ..default()
        },
    ));
    commands.spawn((
        Name::new("Point cloud"),
        ViewerCloud,
        source.0.clone(),
        FrameWhenLoaded,
    ));
}

fn frame_loaded(
    mut commands: Commands,
    clouds: Query<(Entity, &PointcloudStatus, &GlobalTransform), With<FrameWhenLoaded>>,
    pending: Query<(), With<LoadPointcloud>>,
    windows: Query<&Window>,
    mut config: ResMut<NavigationConfig>,
    mut actions: MessageWriter<CameraAction>,
) {
    for (entity, status, transform) in &clouds {
        // An Update submission is processed in the next PreUpdate. Until then,
        // the root still has its previous source's status and bounds.
        if pending.contains(entity) {
            continue;
        }
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
    settings: Option<ResMut<settings::Settings>>,
) {
    if config.blocked {
        return;
    }
    if keys.just_pressed(KeyCode::KeyO)
        && let Some(mut settings) = settings
    {
        settings.pivot = !settings.pivot;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        actions.write(CameraAction::ToggleMode);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        actions.write(CameraAction::Reset);
    }
}

/// Selection and residency figures behind the current level of detail.
#[derive(SystemParam)]
struct Lod<'w> {
    assets: Res<'w, Assets<renderer::PointCloud>>,
    resident: Res<'w, renderer::PointCloudTotalSize>,
    server: Res<'w, renderer::PointCloudServerSettings>,
    /// Live chunk and mesh assets track resident nodes once evicted memory is freed.
    chunks: Res<'w, Assets<renderer::PointCloudChunk>>,
    meshes: Res<'w, Assets<Mesh>>,
}

impl Lod<'_> {
    fn summary(
        &self,
        view: &renderer::VisiblePointCloudOctreeEntities,
        settings: &PointCloudVisibilitySettings,
    ) -> String {
        let (mut nodes, mut points, mut capped) = (0, 0, 0);
        for cloud in view.entities.values() {
            let tree = self
                .assets
                .get(cloud.asset_id)
                .and_then(|asset| asset.topology.as_octree());
            nodes += cloud.node_entities.len();
            points += cloud
                .node_entities
                .iter()
                .filter_map(|node| tree?.get_node(node.id))
                .map(|node| node.point_count)
                .sum::<usize>();
            capped += cloud.refinement_candidates.iter().filter(|c| c.1).count();
        }
        let budget = settings.point_budget.unwrap_or(0);
        let millions = |points: usize| points as f64 / 1e6;
        let mb = |bytes: usize| bytes / (1024 * 1024);
        format!(
            "Drawn {nodes}/{} nodes, {:.2}M/{:.1}M pts, {capped} capped\n\
             Resident {}/{} MB, {:.2}M/{:.1}M pts, {} chunks, {} meshes",
            renderer::MAX_NODES,
            millions(points),
            millions(budget),
            mb(self.resident.resident_bytes()),
            mb(self.server.max_size),
            millions(self.resident.resident_points()),
            millions(budget.saturating_mul(2)),
            self.chunks.len(),
            self.meshes.len(),
        )
    }
}

fn status_text(
    clouds: Query<&PointcloudStatus>,
    files: Res<open::files::Files>,
    views: Query<
        (
            &renderer::VisiblePointCloudOctreeEntities,
            &PointCloudVisibilitySettings,
        ),
        With<ViewerCamera>,
    >,
    lod: Lod,
    mut text: Query<&mut Text, With<StatusText>>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    let Some(status) = clouds.iter().next() else {
        return;
    };
    let detail = status.error.clone().unwrap_or_else(|| {
        let lod = views
            .single()
            .map(|(view, settings)| format!("\n{}", lod.summary(view, settings)))
            .unwrap_or_default();
        format!(
            "{:?} | {} active, {} queued{lod}",
            status.phase, status.active, status.queued
        )
    });
    let input = files
        .notice
        .as_deref()
        .unwrap_or("Drop a PLY file or Potree folder | Open: files and URLs");
    text.0 = format!(
        "{detail}\n{input}\nDrag: orbit | Scroll: zoom | F: frame | M: orbit/fly | R: reset | O: pivot"
    );
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
