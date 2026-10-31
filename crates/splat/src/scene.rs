use super::DecodedSplat;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_gaussian_splatting::{CloudSettings, PlanarGaussian3d, PlanarGaussian3dHandle};

/// Coordinate frame of the source file; conversion is applied once on its root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceUp {
    /// Right-handed Z-up source coordinates.
    #[default]
    Z,
    /// Right-handed positive Y-up source coordinates.
    Y,
    /// Right-handed positive X-up source coordinates.
    X,
    /// Right-handed negative X-up source coordinates.
    NegativeX,
    /// Right-handed negative Y-up source coordinates.
    NegativeY,
    /// Right-handed negative Z-up source coordinates.
    NegativeZ,
}
impl SourceUp {
    /// Signed source direction treated as vertical.
    pub fn label(self) -> &'static str {
        match self {
            Self::X => "+X",
            Self::NegativeX => "-X",
            Self::Y => "+Y",
            Self::NegativeY => "-Y",
            Self::Z => "+Z",
            Self::NegativeZ => "-Z",
        }
    }

    /// Return the single source-to-renderer basis transform.
    pub fn transform(self) -> Transform {
        match self {
            Self::Z => {
                Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            }
            Self::Y => Transform::IDENTITY,
            Self::X => Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
            Self::NegativeX => {
                Transform::from_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2))
            }
            Self::NegativeY => {
                Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::PI))
            }
            Self::NegativeZ => {
                Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            }
        }
    }
}

/// Request a replacement cloud from local bytes.
#[derive(Message)]
pub struct LoadSplat {
    /// Filename used to select the PLY or SOG decoder.
    pub name: String,
    /// Contents of the selected local file.
    pub bytes: std::sync::Arc<[u8]>,
    /// Source coordinate frame.
    pub up: SourceUp,
}

/// Change the coordinate basis of the current cloud and any pending decode.
#[derive(Message)]
pub struct SetSplatUp(pub SourceUp);

#[derive(Component)]
pub(crate) struct SourceGeometry {
    pub(crate) bounds: (Vec3, Vec3),
    pub(crate) support: f32,
}

impl SourceGeometry {
    /// Return the source support bounds after one coordinate conversion.
    pub(crate) fn transformed_bounds(&self, transform: Transform) -> (Vec3, Vec3) {
        let (min, max) = self.bounds;
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for x in [min.x, max.x] {
            for y in [min.y, max.y] {
                for z in [min.z, max.z] {
                    let point = transform.transform_point(Vec3::new(x, y, z));
                    low = low.min(point);
                    high = high.max(point);
                }
            }
        }
        (
            low - Vec3::splat(self.support),
            high + Vec3::splat(self.support),
        )
    }
}

/// Observable loading state; failures retain the previous scene.
#[derive(Clone, Debug, Default)]
pub enum LoadPhase {
    /// No file has been requested.
    #[default]
    Empty,
    /// A file is being decoded.
    Loading(String),
    /// The latest file loaded successfully.
    Ready { name: String, count: usize },
    /// The latest file could not be loaded.
    Failed(String),
}

/// Current visible cloud and its bounds in renderer coordinates.
#[derive(Clone)]
pub struct LoadedSplat {
    /// Root entity of the visible cloud.
    pub entity: Entity,
    /// Conservative Gaussian support bounds after basis conversion.
    pub bounds: (Vec3, Vec3),
}

/// Scene lifecycle state owned by the splat loader.
#[derive(Resource, Default)]
pub struct SplatScene {
    /// Most recent request status.
    pub phase: LoadPhase,
    /// Current visible cloud, retained during loading and failures.
    pub loaded: Option<LoadedSplat>,
    generation: u64,
}

/// Notification emitted after a successful cloud replacement.
#[derive(Message)]
pub struct SplatLoaded {
    /// Loaded cloud bounds in renderer coordinates.
    pub bounds: (Vec3, Vec3),
}

/// Decode and replace clouds; can run without a window or render device.
pub struct SplatScenePlugin;
impl Plugin for SplatScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SplatScene>()
            .init_resource::<Assets<PlanarGaussian3d>>()
            .add_message::<LoadSplat>()
            .add_message::<SetSplatUp>()
            .add_message::<SplatLoaded>()
            .init_resource::<PendingLoad>()
            .add_systems(Update, (request_load, reorient, poll_load).chain());
    }
}

struct Pending {
    generation: u64,
    name: String,
    up: SourceUp,
    task: Task<Result<DecodedSplat, String>>,
}

#[derive(Resource, Default)]
pub(crate) struct PendingLoad(Option<Pending>);

impl PendingLoad {
    #[cfg(feature = "viewer")]
    pub(crate) fn is_pending(&self) -> bool {
        self.0.is_some()
    }

    /// Retire a whole-file decode when a newer streamed scene is selected.
    #[cfg(feature = "viewer")]
    pub(crate) fn cancel(&mut self, scene: &mut SplatScene) {
        self.0 = None;
        scene.generation += 1;
    }
}

/// Begin the latest whole-file decode and retire any older pending generation.
pub(crate) fn request_load(
    mut requests: MessageReader<LoadSplat>,
    mut pending: ResMut<PendingLoad>,
    mut scene: ResMut<SplatScene>,
    device: Option<Res<bevy::render::renderer::RenderDevice>>,
) {
    let Some(request) = requests.read().last() else {
        return;
    };
    scene.generation += 1;
    scene.phase = LoadPhase::Loading(request.name.clone());
    let bytes = request.bytes.clone();
    let name = request.name.clone();
    let storage_limit = device
        .as_ref()
        .map(|device| device.limits().max_storage_buffer_binding_size);
    pending.0 = Some(Pending {
        generation: scene.generation,
        name: request.name.clone(),
        up: request.up,
        task: AsyncComputeTaskPool::get()
            .spawn(async move { super::decode::decode_for_device(&name, &bytes, storage_limit) }),
    });
}

fn reorient(
    mut requests: MessageReader<SetSplatUp>,
    mut pending: ResMut<PendingLoad>,
    mut scene: ResMut<SplatScene>,
    mut roots: Query<(&SourceGeometry, &mut Transform)>,
) {
    let Some(SetSplatUp(up)) = requests.read().last() else {
        return;
    };
    if let Some(load) = &mut pending.0 {
        load.up = *up;
    }
    if let Some(loaded) = &mut scene.loaded
        && let Ok((source, mut transform)) = roots.get_mut(loaded.entity)
    {
        *transform = up.transform();
        loaded.bounds = source.transformed_bounds(*transform);
    }
}

fn poll_load(mut commands: Commands, mut pending: ResMut<PendingLoad>) {
    let Some(load) = pending.0.as_mut() else {
        return;
    };
    let Some(result) = block_on(future::poll_once(&mut load.task)) else {
        return;
    };
    let load = pending.0.take().expect("completed load");
    commands
        .queue(move |world: &mut World| finish(world, load.generation, load.name, load.up, result));
}

fn finish(
    world: &mut World,
    generation: u64,
    name: String,
    up: SourceUp,
    result: Result<DecodedSplat, String>,
) {
    if generation != world.resource::<SplatScene>().generation {
        return;
    }
    let decoded = match result {
        Ok(decoded) => decoded,
        Err(error) => {
            world.resource_mut::<SplatScene>().phase =
                LoadPhase::Failed(format!("{name}: {error}"));
            return;
        }
    };
    if let Some(device) = world.get_resource::<bevy::render::renderer::RenderDevice>() {
        let largest_plane = decoded.cloud.spherical_harmonic.len()
            * std::mem::size_of::<bevy_gaussian_splatting::SphericalHarmonicCoefficients>();
        if largest_plane as u64 > device.limits().max_storage_buffer_binding_size {
            world.resource_mut::<SplatScene>().phase = LoadPhase::Failed(format!(
                "{name}: cloud exceeds this GPU's storage buffer limit"
            ));
            return;
        }
    }
    let transform = up.transform();
    let support = decoded
        .cloud
        .scale_opacity
        .iter()
        .take(decoded.count)
        .flat_map(|s| s.scale)
        .fold(0_f32, f32::max)
        * 3.;
    let source = SourceGeometry {
        bounds: decoded.bounds,
        support,
    };
    let (low, high) = source.transformed_bounds(transform);
    let handle = world
        .resource_mut::<Assets<PlanarGaussian3d>>()
        .add(decoded.cloud);
    let entity = world
        .spawn((
            PlanarGaussian3dHandle(handle),
            CloudSettings {
                raster_profile: bevy_gaussian_splatting::GaussianRasterProfile::NormalizedGaussian,
                ..default()
            },
            transform,
            source,
            Name::new(name.clone()),
        ))
        .id();
    if let Some(old) = world.resource_mut::<SplatScene>().loaded.take() {
        retire(world, old.entity);
    }
    let mut scene = world.resource_mut::<SplatScene>();
    scene.loaded = Some(LoadedSplat {
        entity,
        bounds: (low, high),
    });
    scene.phase = LoadPhase::Ready {
        name,
        count: decoded.count,
    };
    world.write_message(SplatLoaded {
        bounds: (low, high),
    });
}

/// Retire a scene root and release any whole-file asset it owns.
pub(crate) fn retire(world: &mut World, entity: Entity) {
    let asset = world
        .get::<PlanarGaussian3dHandle>(entity)
        .map(|h| h.0.id());
    world.despawn(entity);
    if let Some(asset) = asset {
        world
            .resource_mut::<Assets<PlanarGaussian3d>>()
            .remove(asset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_splat;
    const PLY: &[u8] = include_bytes!("../tests/fixtures/tiny.ply");

    fn world() -> World {
        let mut app = App::new();
        app.add_plugins(SplatScenePlugin);
        app.world_mut().resource_mut::<SplatScene>().generation = 1;
        std::mem::take(app.world_mut())
    }

    #[test]
    fn successful_replacement_uses_one_basis_transform_and_removes_old_entity() {
        let mut world = world();
        finish(
            &mut world,
            1,
            "tiny.ply".into(),
            SourceUp::Z,
            decode_splat("tiny.ply", PLY),
        );
        let first = world
            .resource::<SplatScene>()
            .loaded
            .as_ref()
            .unwrap()
            .entity;
        let transform = world.get::<Transform>(first).unwrap();
        assert!(
            (transform.transform_point(Vec3::new(1., 2., 3.)) - Vec3::new(1., 3., -2.)).length()
                < 1e-5
        );
        finish(
            &mut world,
            1,
            "tiny.ply".into(),
            SourceUp::Y,
            decode_splat("tiny.ply", PLY),
        );
        assert!(world.get_entity(first).is_err());
        let second = world
            .resource::<SplatScene>()
            .loaded
            .as_ref()
            .unwrap()
            .entity;
        assert_eq!(
            *world.get::<Transform>(second).unwrap(),
            Transform::IDENTITY
        );
        assert_eq!(world.resource::<Assets<PlanarGaussian3d>>().len(), 1);
    }

    #[test]
    fn framing_bounds_include_gaussian_support_beyond_center_positions() {
        let mut world = world();
        finish(
            &mut world,
            1,
            "tiny.ply".into(),
            SourceUp::Y,
            decode_splat("tiny.ply", PLY),
        );
        let (min, max) = world
            .resource::<SplatScene>()
            .loaded
            .as_ref()
            .unwrap()
            .bounds;
        assert!((min.x + 1.6693905).abs() < 1e-5, "{min:?}");
        assert!((max.z - 1.6693905).abs() < 1e-5, "{max:?}");
    }

    #[test]
    fn failed_replacement_retains_the_current_cloud() {
        let mut world = world();
        finish(
            &mut world,
            1,
            "tiny.ply".into(),
            SourceUp::Z,
            decode_splat("tiny.ply", PLY),
        );
        let first = world
            .resource::<SplatScene>()
            .loaded
            .as_ref()
            .unwrap()
            .entity;
        finish(
            &mut world,
            1,
            "broken.sog".into(),
            SourceUp::Z,
            Err("invalid archive".into()),
        );
        assert_eq!(
            world
                .resource::<SplatScene>()
                .loaded
                .as_ref()
                .unwrap()
                .entity,
            first
        );
        assert!(world.get_entity(first).is_ok());
        assert!(matches!(
            world.resource::<SplatScene>().phase,
            LoadPhase::Failed(_)
        ));
    }

    #[test]
    fn superseded_completion_cannot_replace_the_latest_scene() {
        let mut world = world();
        finish(
            &mut world,
            1,
            "latest.ply".into(),
            SourceUp::Y,
            decode_splat("latest.ply", PLY),
        );
        let first = world
            .resource::<SplatScene>()
            .loaded
            .as_ref()
            .unwrap()
            .entity;
        finish(
            &mut world,
            0,
            "old.ply".into(),
            SourceUp::Z,
            decode_splat("old.ply", PLY),
        );
        assert_eq!(
            world
                .resource::<SplatScene>()
                .loaded
                .as_ref()
                .unwrap()
                .entity,
            first
        );
        assert!(
            matches!(&world.resource::<SplatScene>().phase, LoadPhase::Ready { name, .. } if name == "latest.ply")
        );
    }
    #[test]
    fn all_signed_source_axes_map_to_renderer_up_without_reflection() {
        for (axis, direction) in [
            (SourceUp::X, Vec3::X),
            (SourceUp::NegativeX, Vec3::NEG_X),
            (SourceUp::Y, Vec3::Y),
            (SourceUp::NegativeY, Vec3::NEG_Y),
            (SourceUp::Z, Vec3::Z),
            (SourceUp::NegativeZ, Vec3::NEG_Z),
        ] {
            let transform = axis.transform();
            assert!(
                (transform.rotation * direction).abs_diff_eq(Vec3::Y, 1e-6),
                "{axis:?}"
            );
            assert!((Mat3::from_quat(transform.rotation).determinant() - 1.).abs() < 1e-6);
        }
    }

    #[test]
    fn source_axis_changes_reorient_loaded_data_and_bounds_without_reloading() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = world();
        let mut decoded = decode_splat("tiny.ply", PLY).unwrap();
        decoded.bounds = (Vec3::new(-1., -2., -3.), Vec3::new(2., 4., 6.));
        finish(&mut world, 1, "tiny.ply".into(), SourceUp::Z, Ok(decoded));
        let original = world
            .resource::<SplatScene>()
            .loaded
            .as_ref()
            .unwrap()
            .clone();
        let asset = world
            .get::<PlanarGaussian3dHandle>(original.entity)
            .unwrap()
            .0
            .id();
        world.write_message(SetSplatUp(SourceUp::Y));
        world.run_system_once(reorient).unwrap();
        let changed = world.resource::<SplatScene>().loaded.as_ref().unwrap();
        assert_eq!(changed.entity, original.entity);
        assert_eq!(
            *world.get::<Transform>(original.entity).unwrap(),
            Transform::IDENTITY
        );
        assert!((changed.bounds.1.y - original.bounds.1.y + 2.).abs() < 1e-5);
        assert!((changed.bounds.1.z - original.bounds.1.z - 4.).abs() < 1e-5);
        assert_eq!(
            world
                .get::<PlanarGaussian3dHandle>(original.entity)
                .unwrap()
                .0
                .id(),
            asset
        );
        assert_eq!(world.resource::<Assets<PlanarGaussian3d>>().len(), 1);
        world.resource_mut::<Messages<SetSplatUp>>().clear();
        world.write_message(SetSplatUp(SourceUp::Z));
        world.run_system_once(reorient).unwrap();
        let restored = world.resource::<SplatScene>().loaded.as_ref().unwrap();
        assert!(restored.bounds.0.abs_diff_eq(original.bounds.0, 1e-5));
        assert!(restored.bounds.1.abs_diff_eq(original.bounds.1, 1e-5));
        assert!(
            world
                .get::<Transform>(original.entity)
                .unwrap()
                .rotation
                .abs_diff_eq(SourceUp::Z.transform().rotation, 1e-6)
        );
    }

    #[test]
    fn axis_changes_during_decode_apply_to_the_pending_completion() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, SplatScenePlugin));
        app.world_mut().write_message(LoadSplat {
            name: "tiny.ply".into(),
            bytes: PLY.into(),
            up: SourceUp::Z,
        });
        app.world_mut().write_message(SetSplatUp(SourceUp::Y));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            app.update();
            if let Some(loaded) = &app.world().resource::<SplatScene>().loaded {
                assert_eq!(
                    *app.world().get::<Transform>(loaded.entity).unwrap(),
                    Transform::IDENTITY
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "decode did not finish"
            );
            std::thread::yield_now();
        }
    }
}
