use bevy::{camera::primitives::Aabb, prelude::*};
#[cfg(not(target_arch = "wasm32"))]
use bevy_pointcloud::FileSource;
use bevy_pointcloud::{
    HttpSource, PointCloud, PointCloud3d, PointCloudChunk, PointCloudLoadFailed,
    PointCloudLoadTasks, PointCloudMaterial3d, PointCloudMeshLoader, PointCloudServer,
    PointCloudServerSettings, PointCloudServerSystems, PointCloudTopology, PointSizeMode,
    SplatSettings, StandardPointCloudMaterial,
    potree::{PotreeAssetSource, PotreeLoader},
};
use std::path::PathBuf;

/// Coordinate convention of a source, before the request's parent transform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointcloudUp {
    /// Coordinates already use Bevy's Y-up basis (including host-converted scenes).
    #[default]
    Y,
    /// Right-handed Z-up coordinates, converted once at the cloud entity.
    Z,
}

/// A reusable source independent of application site discovery and authorization.
#[derive(Clone, Debug)]
pub enum PointcloudSource {
    /// HTTP(S) Potree directory or metadata URL, including an optional query suffix.
    PotreeUrl(String),
    /// Native Potree directory, or its metadata.json file.
    PotreePath(PathBuf),
    /// In-memory vertices; useful for procedural clouds and deterministic examples.
    Mesh(Mesh),
}

/// Initial display settings; live edits use the renderer's components.
#[derive(Clone, Debug)]
pub struct PointcloudSettings {
    pub splats: SplatSettings,
    pub unlit: bool,
}

impl Default for PointcloudSettings {
    fn default() -> Self {
        Self {
            splats: SplatSettings {
                point_size_mode: PointSizeMode::ScreenPixels,
                point_size: 24.,
                min_point_size: Some(1.),
                max_point_size: Some(32.),
                radius: Some(0.5),
                ..default()
            },
            unlit: true,
        }
    }
}

/// Insert on an entity to load or replace its cloud. Removing the entity retires it.
#[derive(Component, Clone, Debug)]
pub struct LoadPointcloud {
    pub source: PointcloudSource,
    pub up: PointcloudUp,
    pub transform: Transform,
    pub settings: PointcloudSettings,
    pub max_concurrent_requests: Option<usize>,
    /// Deadline for initial metadata/hierarchy, independent of later refinement.
    pub metadata_timeout_secs: f32,
}

impl LoadPointcloud {
    /// Request a hosted Potree dataset; authorization and URL resolution stay with the host.
    pub fn potree(url: impl Into<String>) -> Self {
        Self::new(PointcloudSource::PotreeUrl(url.into()))
    }

    /// Request an in-memory cloud through the same loading and rendering path.
    pub fn mesh(mesh: Mesh) -> Self {
        Self::new(PointcloudSource::Mesh(mesh))
    }

    /// Construct a source request with identity axes and default display settings.
    pub fn new(source: PointcloudSource) -> Self {
        Self {
            source,
            up: PointcloudUp::Y,
            transform: Transform::IDENTITY,
            settings: default(),
            max_concurrent_requests: None,
            metadata_timeout_secs: 30.,
        }
    }

    /// Compose source-basis conversion with its placement, before the parent's transform.
    pub fn render_transform(&self) -> Transform {
        let rotation = match self.up {
            PointcloudUp::Y => Quat::IDENTITY,
            PointcloudUp::Z => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        };
        Transform::from_rotation(rotation).mul_transform(self.transform)
    }
}

/// CPU-side readiness, separate from streaming activity and GPU presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointcloudPhase {
    Metadata,
    Preparing,
    /// At least one nonempty chunk has a prepared mesh; this is not a presented-frame signal.
    Ready,
    Failed,
}

/// Per-cloud bounds, readiness, failure and live refinement counts.
#[derive(Component, Clone, Debug)]
pub struct PointcloudStatus {
    pub phase: PointcloudPhase,
    pub bounds: Option<Aabb>,
    pub active: usize,
    pub queued: usize,
    pub error: Option<String>,
    started: f32,
    timeout: f32,
}

impl PointcloudStatus {
    /// Initialize status for a new request, clearing any previous result.
    pub fn loading(started: f32, timeout: f32) -> Self {
        Self {
            phase: PointcloudPhase::Metadata,
            bounds: None,
            active: 0,
            queued: 0,
            error: None,
            started,
            timeout,
        }
    }
}

/// Shared loading/status systems. Hosts can order their adapters after this set.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub struct PointcloudSystems;

/// Install the renderer and common loading behavior without creating a camera or UI.
#[derive(Default)]
pub struct PointcloudPlugin {
    pub cache: PointCloudServerSettings,
}

impl Plugin for PointcloudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bevy_pointcloud::PointCloudPlugin::default())
            .insert_resource(self.cache.clone())
            .add_systems(PreUpdate, load_requests.in_set(PointcloudSystems))
            .add_systems(
                PostUpdate,
                update_status
                    .after(PointCloudServerSystems)
                    .after(bevy_pointcloud::process_point_cloud_load_tasks)
                    .in_set(PointcloudSystems),
            );
    }
}

pub(super) fn load_requests(
    mut commands: Commands,
    time: Res<Time>,
    server: Res<PointCloudServer>,
    mut materials: ResMut<Assets<StandardPointCloudMaterial>>,
    requests: Query<(Entity, &LoadPointcloud, Option<&Children>)>,
    render_chunks: Query<(), With<bevy_pointcloud::PointCloudChunk3d>>,
) {
    for (entity, request, children) in &requests {
        let mut status =
            PointcloudStatus::loading(time.elapsed_secs(), request.metadata_timeout_secs);
        let result = source_handle(request, &server);
        if let Some(children) = children {
            for child in children {
                if render_chunks.contains(*child) {
                    commands.entity(*child).despawn();
                }
            }
        }
        let mut entity = commands.entity(entity);
        // Replacing the handle releases the previous loader through normal renderer cleanup.
        // Invalid replacements also retire the old cloud, rather than showing stale content.
        entity.remove::<(
            LoadPointcloud,
            PointCloud3d,
            PointCloudMaterial3d<StandardPointCloudMaterial>,
        )>();
        entity.remove::<(bevy_pointcloud::PointCloudChunk3d, Mesh3d, Aabb)>();
        match result {
            Ok(handle) => {
                let material = materials.add(StandardPointCloudMaterial::from(StandardMaterial {
                    cull_mode: None,
                    unlit: request.settings.unlit,
                    ..default()
                }));
                entity.insert((
                    PointCloud3d(handle),
                    PointCloudMaterial3d(material),
                    request.settings.splats.clone(),
                    request.render_transform(),
                ));
            }
            Err(error) => {
                status.phase = PointcloudPhase::Failed;
                status.error = Some(error);
            }
        }
        entity.insert(status);
    }
}

fn source_handle(
    request: &LoadPointcloud,
    server: &PointCloudServer,
) -> Result<Handle<PointCloud>, String> {
    match &request.source {
        PointcloudSource::PotreeUrl(url) => {
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return Err("Potree sources require an HTTP(S) URL".into());
            }
            let source = PotreeAssetSource::<HttpSource>::from_url_with_octree_concurrency(
                url,
                request.max_concurrent_requests,
            )
            .map_err(|error| error.to_string())?;
            Ok(server.load::<PotreeLoader<_>>(source))
        }
        PointcloudSource::PotreePath(path) => {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = path;
                Err("Local Potree directories require the native viewer; use an HTTP(S) URL in the browser".into())
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let directory = if path.file_name().is_some_and(|name| name == "metadata.json") {
                    path.parent().unwrap_or(path)
                } else {
                    path.as_path()
                };
                let source = PotreeAssetSource::<FileSource>::from_path(directory)
                    .map_err(|error| error.to_string())?;
                Ok(server.load::<PotreeLoader<_>>(source))
            }
        }
        PointcloudSource::Mesh(mesh) => Ok(server.load::<PointCloudMeshLoader>(mesh.clone())),
    }
}

pub(super) fn update_status(
    mut commands: Commands,
    time: Res<Time>,
    clouds: Res<Assets<PointCloud>>,
    chunks: Res<Assets<PointCloudChunk>>,
    tasks: Res<PointCloudLoadTasks>,
    mut failures: MessageReader<PointCloudLoadFailed>,
    mut query: Query<(Entity, Option<&PointCloud3d>, &mut PointcloudStatus)>,
) {
    let failures: Vec<_> = failures.read().collect();
    for (entity, handle, mut status) in &mut query {
        if status.phase == PointcloudPhase::Failed {
            continue;
        }
        let Some(handle) = handle else {
            continue;
        };
        let id = handle.id();
        status.active = tasks
            .hierarchy_in_flight
            .iter()
            .chain(&tasks.chunk_in_flight)
            .filter(|key| key.id == id)
            .count();
        status.queued = tasks
            .hierarchy_heap
            .iter()
            .chain(&tasks.chunk_heap)
            .filter(|task| task.0.asset_id == id)
            .count();
        if let Some(failure) = failures.iter().find(|failure| failure.id == id) {
            status.phase = PointcloudPhase::Failed;
            status.error = Some(failure.error.clone());
            commands.entity(entity).remove::<PointCloud3d>();
            continue;
        }
        let Some(cloud) = clouds.get(id) else {
            if time.elapsed_secs() - status.started > status.timeout {
                status.phase = PointcloudPhase::Failed;
                status.error = Some("Initial point-cloud metadata/hierarchy load timed out".into());
                commands.entity(entity).remove::<PointCloud3d>();
            }
            continue;
        };
        status.bounds = cloud.aabb;
        let prepared = |chunk: &Handle<PointCloudChunk>| {
            chunks
                .get(chunk)
                .is_some_and(|chunk| chunk.mesh_handle.is_some() && chunk.vertex_buffer_size > 0)
        };
        let ready = match &cloud.topology {
            PointCloudTopology::Flat(chunk) => prepared(chunk),
            PointCloudTopology::Octree(tree) => tree
                .nodes
                .iter()
                .any(|(_, node)| node.chunk.as_ref().is_some_and(&prepared)),
            PointCloudTopology::Empty => false,
        };
        status.phase = if ready {
            PointcloudPhase::Ready
        } else {
            PointcloudPhase::Preparing
        };
    }
}
