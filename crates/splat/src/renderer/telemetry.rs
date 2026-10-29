//! Opt-in production LOD pass attribution and GPU buffer capacity observations.
//! Observers retain IDs and sizes only, never allocation handles. These scoped
//! capacities exclude driver residency, textures and in-flight retired owners.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

use bevy::{
    prelude::*,
    render::{
        Extract, RenderApp,
        render_asset::RenderAssets,
        render_resource::{Buffer, BufferId},
    },
};
use bevy_gaussian_splatting::{
    Gaussian3d, GaussianCamera, PlanarGaussian3dHandle,
    gaussian::formats::planar_3d::PlanarStorageGaussian3d,
    gpu_timing::{GaussianGpuTimingPlugin, GaussianTimingSource, GaussianTimingView},
    render::compact::CompactCloud,
    sort::{GpuSortedEntry, radix::RadixSortBuffers},
};
use serde::Serialize;

/// Optional caller identity for controlled single-scene experiments. Set a new
/// selection for every originating main-world frame, before `Last` runs. Delayed
/// GPU results keep this selection; it must never be inferred from arrival time.
#[derive(Resource, Clone, Copy)]
pub struct SogGpuTimingIdentity {
    pub scene: u64,
    pub selection: u64,
}

/// Installs observation independently of timestamp support. Timestamp queries
/// are opt-in and use the actual device's enabled features without forcing an
/// unsupported feature into device creation.
pub struct SogGpuTelemetryPlugin {
    pub timestamps: bool,
}

impl Default for SogGpuTelemetryPlugin {
    fn default() -> Self {
        Self { timestamps: true }
    }
}

impl Plugin for SogGpuTelemetryPlugin {
    fn build(&self, app: &mut App) {
        if self.timestamps {
            app.add_plugins(GaussianGpuTimingPlugin);
        }
        let bridge = SogGpuTelemetry::default();
        app.insert_resource(bridge.clone())
            .add_systems(Last, tag_frame.in_set(SogGpuTelemetrySystems));
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(bridge)
                .add_systems(ExtractSchedule, observe_buffers);
        }
    }
}

/// Run consumers after this set to publish the same originating frame identity.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SogGpuTelemetrySystems;

#[derive(Resource, Clone, Default)]
pub struct SogGpuTelemetry(Arc<Mutex<TelemetryReport>>);

impl SogGpuTelemetry {
    /// Bounded metadata, never handles. Before extraction the inventory is null.
    pub fn report(&self) -> serde_json::Value {
        self.0
            .lock()
            .ok()
            .and_then(|report| serde_json::to_value(&*report).ok())
            .unwrap_or_else(
                || serde_json::json!({"unavailable_reason": "GPU telemetry lock poisoned"}),
            )
    }
}

#[derive(Serialize)]
struct TelemetryReport {
    attribution: Attribution,
    buffers: Option<BufferReport>,
    buffers_unavailable_reason: Option<&'static str>,
}

impl Default for TelemetryReport {
    fn default() -> Self {
        Self {
            attribution: default(),
            buffers: None,
            buffers_unavailable_reason: Some("awaiting render extraction"),
        }
    }
}

#[derive(Default, Serialize)]
struct Attribution {
    // Strings preserve entity and selection IDs beyond JavaScript's safe integer.
    current_frame: String,
    sources: Vec<SourceIdentity>,
    sources_truncated: usize,
}

#[derive(Serialize)]
struct SourceIdentity {
    entity: String,
    scene: String,
    selection: String,
}

fn tag_frame(
    mut commands: Commands,
    telemetry: Res<SogGpuTelemetry>,
    identity: Option<Res<SogGpuTimingIdentity>>,
    cameras: Query<Entity, With<GaussianCamera>>,
    clouds: Query<(Entity, Option<&ChildOf>), With<PlanarGaussian3dHandle>>,
    mut sequence: Local<u64>,
) {
    *sequence = sequence.saturating_add(1);
    let frame = identity.as_ref().map_or(*sequence, |id| id.selection);
    let mut attribution = Attribution {
        current_frame: frame.to_string(),
        ..default()
    };
    for entity in &cameras {
        commands.entity(entity).insert(GaussianTimingView);
    }
    for (entity, parent) in &clouds {
        let scene = identity.as_ref().map_or_else(
            || parent.map_or(entity, ChildOf::parent).to_bits(),
            |id| id.scene,
        );
        commands.entity(entity).insert(GaussianTimingSource {
            scene,
            selection: frame,
        });
        if attribution.sources.len() < 64 {
            attribution.sources.push(SourceIdentity {
                entity: entity.to_bits().to_string(),
                scene: scene.to_string(),
                selection: frame.to_string(),
            });
        } else {
            attribution.sources_truncated += 1;
        }
    }
    if let Ok(mut report) = telemetry.0.lock() {
        report.attribution = attribution;
    }
}

#[derive(Default)]
pub(super) struct BufferInventory(HashMap<BufferId, BufferEntry>);

#[derive(Clone, Serialize)]
struct BufferEntry {
    id: String,
    bytes: u64,
    categories: BTreeSet<&'static str>,
}

impl BufferInventory {
    pub(super) fn add(&mut self, buffer: &Buffer, category: &'static str) {
        self.add_id(buffer.id(), buffer.size(), category);
    }

    fn add_compact(&mut self, cloud: &CompactCloud) {
        for buffer in cloud.storage_buffers() {
            self.add(buffer, "compact_selection");
        }
        self.add(cloud.draw_indirect_buffer(), "compact_selection");
        if let Some(buffer) = cloud.projection_buffer() {
            self.add(buffer, "projection");
        }
    }

    fn add_id(&mut self, id: BufferId, bytes: u64, category: &'static str) {
        let entry = self.0.entry(id).or_insert_with(|| BufferEntry {
            id: format!("{id:?}"),
            bytes,
            categories: default(),
        });
        entry.categories.insert(category);
    }
}

#[derive(Default)]
struct Observations {
    previous: HashMap<BufferId, u64>,
    observed_bytes_total: u64,
    observed_buffers_total: u64,
    peak_live_bytes: u64,
}

#[derive(Serialize)]
struct BufferReport {
    originating_frame: String,
    scope: &'static str,
    live_bytes: u64,
    live_buffers: usize,
    peak_live_bytes: u64,
    observed_bytes_total: u64,
    observed_buffers_total: u64,
    categories: BTreeMap<&'static str, Category>,
    entries: Vec<BufferEntry>,
    entries_truncated: usize,
    packed_source_payload_bytes: Option<u64>,
    packed_source_payload_method: &'static str,
    resources: BTreeMap<&'static str, bool>,
    missing: [&'static str; 5],
}

#[derive(Default, Serialize)]
struct Category {
    bytes: u64,
    buffers: usize,
}

impl Observations {
    fn observe(
        &mut self,
        inventory: BufferInventory,
        frame: String,
        packed_bytes: Option<u64>,
    ) -> BufferReport {
        let mut categories = BTreeMap::<_, Category>::new();
        let live_bytes = inventory.0.values().map(|entry| entry.bytes).sum();
        self.peak_live_bytes = self.peak_live_bytes.max(live_bytes);
        for (id, entry) in &inventory.0 {
            if !self.previous.contains_key(id) {
                self.observed_bytes_total = self.observed_bytes_total.saturating_add(entry.bytes);
                self.observed_buffers_total = self.observed_buffers_total.saturating_add(1);
            }
            for category in &entry.categories {
                let category = categories.entry(*category).or_default();
                category.bytes += entry.bytes;
                category.buffers += 1;
            }
        }
        self.previous = inventory
            .0
            .iter()
            .map(|(id, entry)| (*id, entry.bytes))
            .collect();
        let mut entries: Vec<_> = inventory.0.into_values().collect();
        let live_buffers = entries.len();
        entries.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        entries.truncate(4096);
        BufferReport {
            originating_frame: frame,
            scope: "deduplicated observed BufferId capacities; main resident slots/components plus previous render compact/planar/sort/radix owners; category totals overlap",
            live_bytes,
            live_buffers,
            peak_live_bytes: self.peak_live_bytes,
            observed_bytes_total: self.observed_bytes_total,
            observed_buffers_total: self.observed_buffers_total,
            categories,
            entries,
            entries_truncated: live_buffers.saturating_sub(4096),
            packed_source_payload_bytes: packed_bytes,
            packed_source_payload_method: "resident packed source/upload payload accounting including shared palettes; textures and metadata; separate from BufferId sum, excludes driver granularity",
            resources: default(),
            missing: [
                "actual texture allocation inventory",
                "driver VRAM residency and allocation granularity",
                "queue-retained resources after their last observed owner retires",
                "transient upload/bake uniforms and staging buffers",
                "camera/UI/render targets and timestamp query readback buffers",
            ],
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn observe_buffers(
    gpu: Extract<Option<Res<super::gpu::GpuResident>>>,
    packed: Extract<Option<Res<super::packed::PackedResident>>>,
    main_compact: Extract<Query<&CompactCloud>>,
    render_compact: Query<&CompactCloud>,
    planar: Option<Res<RenderAssets<PlanarStorageGaussian3d>>>,
    sorted: Option<Res<RenderAssets<GpuSortedEntry>>>,
    scratch: Option<Res<RadixSortBuffers<Gaussian3d>>>,
    telemetry: Res<SogGpuTelemetry>,
    mut observations: Local<Observations>,
) {
    let resources = BTreeMap::from([
        ("expanded_resident", gpu.as_ref().is_some()),
        ("packed_resident", packed.as_ref().is_some()),
        ("prepared_planar", planar.is_some()),
        ("sorted_entries", sorted.is_some()),
        ("radix_scratch", scratch.is_some()),
    ]);
    if !resources.values().any(|available| *available) {
        if let Ok(mut report) = telemetry.0.lock() {
            report.buffers = None;
            report.buffers_unavailable_reason = Some("render buffer owner resources unavailable");
        }
        return;
    }
    let mut inventory = BufferInventory::default();
    if let Some(gpu) = gpu.as_ref() {
        gpu.observe_buffers(&mut inventory);
    }
    let packed_bytes = packed.as_ref().map(|packed| {
        packed.observe_buffers(&mut inventory);
        packed.observed_source_payload_bytes()
    });
    // Main-world cache removal does not retire already extracted generations.
    // Observe both ECS owners as well as the producer slots; stable BufferIds
    // deduplicate all aliases without retaining any GPU allocation handles.
    for cloud in &main_compact {
        inventory.add_compact(cloud);
    }
    for cloud in &render_compact {
        inventory.add_compact(cloud);
    }
    if let Some(planar) = planar {
        for (_, asset) in planar.iter() {
            for buffer in [
                &asset.position_visibility,
                &asset.spherical_harmonic,
                &asset.rotation,
                &asset.scale_opacity,
                &asset.draw_indirect_buffer,
            ] {
                inventory.add(buffer, "prepared_planar");
            }
        }
    }
    if let Some(sorted) = sorted {
        for (_, asset) in sorted.iter() {
            inventory.add(&asset.sorted_entry_buffer, "sorted_entries");
        }
    }
    if let Some(scratch) = scratch {
        for asset in scratch.asset_map.values() {
            for buffer in [
                &asset.sorting_global_buffer,
                &asset.sorting_status_counter_buffer,
                &asset.entry_buffer_b,
                &asset.dispatch_indirect_buffer,
            ]
            .into_iter()
            .chain(asset.sorting_pass_buffers.iter())
            {
                inventory.add(buffer, "radix_scratch");
            }
        }
    }
    if let Ok(mut report) = telemetry.0.lock() {
        let frame = report.attribution.current_frame.clone();
        let mut buffers = observations.observe(inventory, frame, packed_bytes);
        buffers.resources = resources;
        report.buffers = Some(buffers);
        report.buffers_unavailable_reason = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_deduplicates_shared_buffers_and_tracks_replacement_without_handles() {
        let shared = BufferId::new();
        let replacement = BufferId::new();
        let mut observations = Observations::default();
        let mut inventory = BufferInventory::default();
        inventory.add_id(shared, 128, "prepared_planar");
        inventory.add_id(shared, 128, "expanded_selection");
        let first = observations.observe(inventory, "1".into(), Some(1000));
        assert_eq!((first.live_bytes, first.live_buffers), (128, 1));
        assert_eq!(first.entries[0].categories.len(), 2);
        assert_eq!(first.packed_source_payload_bytes, Some(1000));
        let mut inventory = BufferInventory::default();
        inventory.add_id(shared, 128, "prepared_planar");
        inventory.add_id(replacement, 256, "projection");
        let second = observations.observe(inventory, "2".into(), None);
        assert_eq!(
            (second.observed_bytes_total, second.observed_buffers_total),
            (384, 2)
        );
        let retired = observations.observe(BufferInventory::default(), "3".into(), None);
        assert_eq!((retired.live_bytes, retired.live_buffers), (0, 0));
        assert_eq!(retired.peak_live_bytes, 384);
        assert!(observations.previous.is_empty());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    #[ignore = "requires native GPU; offscreen allocation ownership correctness only"]
    fn compact_buffer_aliases_survive_main_retirement_until_last_render_owner_retires() {
        use bevy::render::{
            MainWorld,
            render_resource::{BufferDescriptor, BufferUsages},
            settings::{Backends, WgpuSettings},
        };
        use bevy_gaussian_splatting::render::sog::work::CompactWork;

        let resources = bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
            Backends::all(),
            None,
            &WgpuSettings::default(),
        ));
        let device = resources.0;
        let allocate = |size, usage| {
            device.create_buffer(&BufferDescriptor {
                label: Some("compact telemetry ownership test"),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let mut compact = {
            let work = CompactWork::new(device.wgpu_device(), 32, 32 * 24).unwrap();
            let compact = CompactCloud::new(
                work.storage_buffers().map(Clone::clone),
                allocate(
                    16,
                    BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
                ),
                32,
                Entity::PLACEHOLDER,
            )
            .unwrap();
            assert_eq!(
                work.storage_buffers().map(Buffer::id),
                compact.storage_buffers().map(Buffer::id),
                "producer and generation must share actual allocation identities"
            );
            compact
        };
        compact
            .set_projection_cache(allocate(32 * 32, BufferUsages::STORAGE))
            .unwrap();
        let expected_bytes = 32 * 24 + 16 + 32 * 32;
        let mut app = App::new();
        app.insert_sub_app(RenderApp, bevy::app::SubApp::new());
        app.add_plugins(SogGpuTelemetryPlugin { timestamps: false });
        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render
            .init_resource::<MainWorld>()
            .init_resource::<RenderAssets<PlanarStorageGaussian3d>>();
        let main_owner = render
            .world_mut()
            .resource_mut::<MainWorld>()
            .spawn(compact.clone())
            .id();
        let first_render_owner = render.world_mut().spawn(compact.clone()).id();
        let last_render_owner = render.world_mut().spawn(compact).id();
        render.world_mut().run_schedule(ExtractSchedule);
        let first = app.world().resource::<SogGpuTelemetry>().report();
        assert_eq!(first["buffers"]["live_bytes"], expected_bytes);
        assert_eq!(first["buffers"]["live_buffers"], 5);
        assert_eq!(first["buffers"]["observed_buffers_total"], 5);
        assert_eq!(
            first["buffers"]["categories"]["compact_selection"]["buffers"],
            4
        );
        assert_eq!(first["buffers"]["categories"]["projection"]["buffers"], 1);

        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render
            .world_mut()
            .resource_mut::<MainWorld>()
            .despawn(main_owner);
        render.world_mut().despawn(first_render_owner);
        render.world_mut().run_schedule(ExtractSchedule);
        let retained = app.world().resource::<SogGpuTelemetry>().report();
        assert_eq!(
            retained["buffers"], first["buffers"],
            "last extracted generation still owns all five allocations"
        );

        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render.world_mut().despawn(last_render_owner);
        render.world_mut().run_schedule(ExtractSchedule);
        let retired = app.world().resource::<SogGpuTelemetry>().report();
        assert_eq!(retired["buffers"]["live_bytes"], 0);
        assert_eq!(retired["buffers"]["live_buffers"], 0);
        assert_eq!(retired["buffers"]["observed_bytes_total"], expected_bytes);
        assert_eq!(retired["buffers"]["observed_buffers_total"], 5);
        assert_eq!(retired["buffers"]["peak_live_bytes"], expected_bytes);
        assert!(retired["buffers"]["entries"].as_array().unwrap().is_empty());
    }

    #[test]
    fn missing_render_resources_are_unavailable_until_actual_owners_exist() {
        let mut app = App::new();
        app.insert_sub_app(RenderApp, bevy::app::SubApp::new());
        app.add_plugins(SogGpuTelemetryPlugin { timestamps: false });
        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render.init_resource::<bevy::render::MainWorld>();
        render.world_mut().run_schedule(ExtractSchedule);
        let report = app.world().resource::<SogGpuTelemetry>().report();
        assert!(report["buffers"].is_null());
        assert_eq!(
            report["buffers_unavailable_reason"],
            "render buffer owner resources unavailable"
        );
        let render = app.get_sub_app_mut(RenderApp).unwrap();
        render.init_resource::<RenderAssets<PlanarStorageGaussian3d>>();
        render.world_mut().run_schedule(ExtractSchedule);
        let report = app.world().resource::<SogGpuTelemetry>().report();
        assert_eq!(report["buffers"]["live_bytes"], 0);
        assert_eq!(report["buffers"]["resources"]["prepared_planar"], true);
        assert_eq!(report["buffers"]["resources"]["radix_scratch"], false);
        assert!(report["buffers_unavailable_reason"].is_null());
    }

    #[test]
    fn tags_actual_clouds_with_current_originating_frame_and_explicit_sample_override() {
        let mut app = App::new();
        app.add_plugins(SogGpuTelemetryPlugin { timestamps: false });
        let parent = app.world_mut().spawn_empty().id();
        let cloud = app
            .world_mut()
            .spawn((PlanarGaussian3dHandle::default(), ChildOf(parent)))
            .id();
        let camera = app.world_mut().spawn(GaussianCamera::default()).id();
        app.update();
        let tag = app.world().get::<GaussianTimingSource>(cloud).unwrap();
        assert_eq!((tag.scene, tag.selection), (parent.to_bits(), 1));
        assert!(app.world().get::<GaussianTimingView>(camera).is_some());
        app.insert_resource(SogGpuTimingIdentity {
            scene: 7,
            selection: u64::MAX,
        });
        app.update();
        let tag = app.world().get::<GaussianTimingSource>(cloud).unwrap();
        assert_eq!((tag.scene, tag.selection), (7, u64::MAX));
        let report = app.world().resource::<SogGpuTelemetry>().report();
        assert_eq!(report["attribution"]["current_frame"], u64::MAX.to_string());
        assert_eq!(
            report["attribution"]["sources"][0]["selection"],
            u64::MAX.to_string()
        );
        assert!(report["buffers"].is_null());
    }
}
