//! Resource checkpoints run outside frame timing. Buffer sizes are not residency.

use serde_json::{Value, json};
use std::{collections::HashSet, hash::Hash};

fn vector_capacity<T>(name: &str, values: &Vec<T>) -> Value {
    json!({"name":name,"length":values.len(),"capacity":values.capacity(),
        "element_bytes":size_of::<T>(),"capacity_bytes":values.capacity() * size_of::<T>()})
}

struct BufferInventory<Id> {
    seen: HashSet<Id>,
    buffers: Vec<Value>,
    bytes: u64,
}

impl<Id> Default for BufferInventory<Id> {
    fn default() -> Self {
        Self {
            seen: HashSet::new(),
            buffers: Vec::new(),
            bytes: 0,
        }
    }
}

impl<Id: Eq + Hash> BufferInventory<Id> {
    fn record(&mut self, id: Id, category: &str, role: &str, bytes: u64) {
        if self.seen.insert(id) {
            self.bytes += bytes;
            self.buffers
                .push(json!({"category":category,"role":role,"size_bytes":bytes}));
        }
    }
    fn report(self, phase: &str) -> Value {
        json!({"phase":phase,"tracked_buffer_bytes":self.bytes,
            "unique_buffers":self.seen.len(),"buffers":self.buffers,"unavailable_reason":null})
    }
}

#[cfg(feature = "gpu")]
pub use native::*;

#[cfg(feature = "gpu")]
mod native {
    use super::*;
    use bevy::{
        prelude::App,
        render::{
            RenderApp,
            render_asset::RenderAssets,
            render_resource::{Buffer, BufferId},
        },
    };
    use bevy_gaussian_splatting::{
        Gaussian3d, PlanarGaussian3d, PlanarSync,
        sort::{GpuSortedEntry, radix::RadixSortBuffers},
    };
    use std::time::Instant;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    pub struct ResourceUsage {
        start: Instant,
        system: System,
        pid: Pid,
        checkpoints: Vec<Value>,
        gpu_checkpoints: Vec<Value>,
    }

    impl ResourceUsage {
        pub fn new() -> Self {
            Self {
                start: Instant::now(),
                system: System::new(),
                pid: Pid::from_u32(std::process::id()),
                checkpoints: Vec::new(),
                gpu_checkpoints: Vec::new(),
            }
        }

        pub fn checkpoint(&mut self, phase: &str) {
            self.system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[self.pid]),
                true,
                ProcessRefreshKind::nothing()
                    .with_memory()
                    .with_cpu()
                    .without_tasks(),
            );
            let process = self
                .system
                .process(self.pid)
                .filter(|_| sysinfo::IS_SUPPORTED_SYSTEM);
            self.checkpoints.push(json!({"phase":phase,"elapsed_ns":self.start.elapsed().as_nanos(),
                "rss_bytes":process.map(|p|p.memory()),"virtual_memory_bytes":process.map(|p|p.virtual_memory()),
                "accumulated_cpu_ms":process.map(|p|p.accumulated_cpu_time()),
                "unavailable_reason":process.is_none().then_some("current_process_query_unavailable")}));
        }

        pub fn gpu_checkpoint(&mut self, app: &App, phase: &str) {
            self.gpu_checkpoints.push(gpu_buffers(app, phase));
        }

        pub fn report(self, input: &Vec<u8>, decoded: Value) -> Value {
            json!({"schema_version":1,
                "method":{"process":"sysinfo 0.38.4 current-process RSS, virtual memory and accumulated CPU milliseconds; checkpoints outside timed frames, without per-frame sampling",
                    "gpu_buffers":"RenderApp flat-cloud planes, indirect draw, sorted entries and radix scratch; actual Buffer::size deduplicated by BufferId at view boundaries before screenshot capture",
                    "decoded":"Vec capacity times size_of of each exact element type; excludes allocator metadata and temporary decoder allocations"},
                "capabilities":{"process_checkpoint_sampling":sysinfo::IS_SUPPORTED_SYSTEM,"gpu_buffer_accounting":true,"driver_residency":false},
                "input":{"length_bytes":input.len(),"capacity_bytes":input.capacity()},"decoded":decoded,
                "checkpoints":self.checkpoints,"gpu_checkpoints":self.gpu_checkpoints,
                "peak_rss_bytes":null,"gpu_resident_bytes":null,
                "total_gpu_allocation_bytes":null,"total_cpu_allocation_bytes":null,
                "unavailable":{"peak_rss_bytes":"This binary samples checkpoints, not the process high-water mark; an external process supervisor may report peak RSS separately.",
                    "gpu_resident_bytes":"wgpu exposes no portable driver residency measurement; tracked bytes are buffer capacities, not VRAM usage.",
                    "total_gpu_allocation_bytes":"Inventory excludes textures, render targets, uniforms, staging, timestamp buffers, pipeline and driver allocations.",
                    "total_cpu_allocation_bytes":"Decoded capacity excludes other heap allocations and is not total process memory."}})
        }
    }

    pub fn decoded_capacity(cloud: &PlanarGaussian3d) -> Value {
        let planes = [
            vector_capacity("position_visibility", &cloud.position_visibility),
            vector_capacity("spherical_harmonic", &cloud.spherical_harmonic),
            vector_capacity("rotation", &cloud.rotation),
            vector_capacity("scale_opacity", &cloud.scale_opacity),
        ];
        let bytes: u64 = planes
            .iter()
            .map(|p| p["capacity_bytes"].as_u64().unwrap())
            .sum();
        json!({"scope":"decoded PlanarGaussian3d vector capacities","capacity_bytes":bytes,"planes":planes})
    }

    fn gpu_buffers(app: &App, phase: &str) -> Value {
        let unavailable = |reason| {
            json!({"phase":phase,"tracked_buffer_bytes":null,
            "unique_buffers":null,"buffers":[],"unavailable_reason":reason})
        };
        let Some(render) = app.get_sub_app(RenderApp) else {
            return unavailable("render_app_unavailable");
        };
        let world = render.world();
        let (Some(clouds), Some(sorted), Some(scratch)) = (
            world.get_resource::<RenderAssets<<Gaussian3d as PlanarSync>::GpuPlanarType>>(),
            world.get_resource::<RenderAssets<GpuSortedEntry>>(),
            world.get_resource::<RadixSortBuffers<Gaussian3d>>(),
        ) else {
            return unavailable("flat_cloud_sort_or_radix_resources_unavailable");
        };
        let mut inventory = BufferInventory::<BufferId>::default();
        let mut add = |category, role, buffer: &Buffer| {
            inventory.record(buffer.id(), category, role, buffer.size())
        };
        for (_, cloud) in clouds.iter() {
            add("cloud", "position_visibility", &cloud.position_visibility);
            add("cloud", "spherical_harmonic", &cloud.spherical_harmonic);
            add("cloud", "rotation", &cloud.rotation);
            add("cloud", "scale_opacity", &cloud.scale_opacity);
            add("cloud", "draw_indirect", &cloud.draw_indirect_buffer);
        }
        for (_, entries) in sorted.iter() {
            add("sort", "sorted_entries", &entries.sorted_entry_buffer);
        }
        for scratch in scratch.asset_map.values() {
            add("radix", "global", &scratch.sorting_global_buffer);
            add(
                "radix",
                "status_counters",
                &scratch.sorting_status_counter_buffer,
            );
            for buffer in &scratch.sorting_pass_buffers {
                add("radix", "pass", buffer);
            }
            add("radix", "entry_buffer_b", &scratch.entry_buffer_b);
            add(
                "radix",
                "dispatch_indirect",
                &scratch.dispatch_indirect_buffer,
            );
        }
        inventory.report(phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_capacity_includes_unused_reserved_storage() {
        let values = Vec::<[u32; 4]>::with_capacity(7);
        let row = vector_capacity("reserved", &values);
        assert_eq!(row["length"], 0);
        assert_eq!(row["element_bytes"], 16);
        assert_eq!(row["capacity_bytes"], values.capacity() * 16);
    }

    #[test]
    fn buffer_inventory_counts_aliases_only_once() {
        let mut inventory = BufferInventory::<u64>::default();
        inventory.record(1, "cloud", "plane", 64);
        inventory.record(1, "sort", "alias", 64);
        inventory.record(2, "sort", "scratch", 128);
        let report = inventory.report("after_view:test");
        assert_eq!(report["tracked_buffer_bytes"], 192);
        assert_eq!(report["unique_buffers"], 2);
        assert_eq!(report["buffers"].as_array().unwrap().len(), 2);
    }
}
