//! Asset-free room fixture for the renderer's X-ray (dollhouse) wall clipping.
use super::*;
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology};
use renderer::{DollhouseSettings, MAX_DOLLHOUSE_PLANES};

const HALF_X: f32 = 4.;
const HALF_Z: f32 = 3.;
const HEIGHT: f32 = 2.5;
const STEP: f32 = 0.06;

/// Room shell planes as (inward normal, a point on the wall). The floor is never clipped.
const WALLS: [(Vec3, Vec3); 5] = [
    (Vec3::NEG_X, Vec3::new(HALF_X, HEIGHT / 2., 0.)),
    (Vec3::X, Vec3::new(-HALF_X, HEIGHT / 2., 0.)),
    (Vec3::NEG_Z, Vec3::new(0., HEIGHT / 2., HALF_Z)),
    (Vec3::Z, Vec3::new(0., HEIGHT / 2., -HALF_Z)),
    (Vec3::NEG_Y, Vec3::new(0., HEIGHT, 0.)),
];

/// Whether walls between the camera and the room interior are cut away.
#[derive(Resource)]
pub(super) struct Xray {
    pub(super) enabled: bool,
}

pub(super) fn source() -> LoadPointcloud {
    let mut source = LoadPointcloud::mesh(room_mesh());
    source.settings.splats.point_size_mode = renderer::PointSizeMode::WorldSpace;
    source.settings.splats.point_size = 0.07;
    source.settings.splats.adaptive_point_size = false;
    source.settings.splats.min_point_size = None;
    source.settings.splats.max_point_size = None;
    source
}

pub(super) fn install(app: &mut App) {
    app.insert_resource(Xray { enabled: true })
        .add_systems(Update, (toggle, clip).chain());
}

fn toggle(keys: Res<ButtonInput<KeyCode>>, config: Res<NavigationConfig>, mut xray: ResMut<Xray>) {
    if !config.blocked && keys.just_pressed(KeyCode::KeyX) {
        xray.enabled = !xray.enabled;
    }
}

/// Hide each wall whose outer side faces the camera. Planes are in world space, so
/// this relies on the fixture's identity root transform.
fn clip(
    xray: Res<Xray>,
    camera: Query<&GlobalTransform, With<ViewerCamera>>,
    mut clouds: Query<&mut DollhouseSettings>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    let eye = camera.translation();
    let mut settings = DollhouseSettings {
        enabled: xray.enabled,
        plane_count: WALLS.len() as u32,
        thickness: STEP * 2.,
        ..default()
    };
    for (i, (normal, point)) in WALLS.iter().enumerate().take(MAX_DOLLHOUSE_PLANES) {
        let offset = -normal.dot(*point);
        settings.planes[i] = normal.extend(offset);
        let outside = normal.dot(eye) + offset < 0.;
        settings.centroids_hidden[i] = point.extend(f32::from(u8::from(outside)));
    }
    for mut cloud in &mut clouds {
        // Avoid change detection churn once the camera settles.
        if cloud.enabled != settings.enabled
            || cloud.plane_count != settings.plane_count
            || cloud.thickness != settings.thickness
            || cloud.planes != settings.planes
            || cloud.centroids_hidden != settings.centroids_hidden
        {
            *cloud = settings;
        }
    }
}

fn room_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut add = |p: Vec3, c: [f32; 4]| {
        positions.push(p.to_array());
        colors.push(c);
    };
    let range = |half: f32| {
        let n = (2. * half / STEP).round() as i32;
        (0..=n).map(move |i| -half + i as f32 * STEP)
    };
    let up = |i: i32| i as f32 * STEP;
    let rows = (HEIGHT / STEP).round() as i32;
    for x in range(HALF_X) {
        for z in range(HALF_Z) {
            let checker = ((x / 0.5).floor() + (z / 0.5).floor()).rem_euclid(2.) * 0.08;
            add(
                Vec3::new(x, 0., z),
                [0.45 + checker, 0.4 + checker, 0.36, 1.],
            );
            add(Vec3::new(x, HEIGHT, z), [0.85, 0.85, 0.82, 1.]);
        }
    }
    for y in (0..=rows).map(up) {
        for z in range(HALF_Z) {
            add(Vec3::new(HALF_X, y, z), [0.75, 0.45, 0.4, 1.]);
            add(Vec3::new(-HALF_X, y, z), [0.4, 0.6, 0.75, 1.]);
        }
        for x in range(HALF_X) {
            add(Vec3::new(x, y, HALF_Z), [0.55, 0.7, 0.45, 1.]);
            add(Vec3::new(x, y, -HALF_Z), [0.75, 0.7, 0.4, 1.]);
        }
    }
    // Interior furniture that is only visible once the walls are cut away.
    let table = Vec3::new(-1., 0.75, 0.);
    for x in range(0.9) {
        for z in range(0.6) {
            add(table + Vec3::new(x, 0., z), [0.55, 0.35, 0.2, 1.]);
        }
    }
    let ball = Vec3::new(1.8, 0.6, -0.8);
    let n = 2_000;
    for i in 0..n {
        // Fibonacci sphere: evenly spaced points without a mesh.
        let y = 1. - 2. * (i as f32 + 0.5) / n as f32;
        let r = (1. - y * y).sqrt();
        let a = i as f32 * std::f32::consts::PI * (3. - 5f32.sqrt());
        add(
            ball + 0.6 * Vec3::new(r * a.cos(), y, r * a.sin()),
            [0.95, 0.75, 0.2, 1.],
        );
    }
    let mut mesh = Mesh::new(PrimitiveTopology::PointList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}
