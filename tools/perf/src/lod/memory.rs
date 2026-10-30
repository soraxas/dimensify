//! Current-process observations outside timed updates.
use serde_json::{Value, json};
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
pub struct ProcessUsage {
    system: System,
    pid: Pid,
    start: Instant,
    rows: Vec<Value>,
}
impl ProcessUsage {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            pid: Pid::from_u32(std::process::id()),
            start: Instant::now(),
            rows: Vec::new(),
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
        self.rows.push(json!({"phase":phase,"elapsed_ns":self.start.elapsed().as_nanos(),
            "rss_bytes":process.map(|p|p.memory()), "virtual_memory_bytes":process.map(|p|p.virtual_memory()),
            "accumulated_cpu_ms":process.map(|p|p.accumulated_cpu_time())}));
    }
    pub fn report(self, gpu: Vec<Value>) -> Value {
        json!({"schema_version":1,"checkpoints":self.rows,"gpu_checkpoints":gpu,
            "method":{"process":"sysinfo 0.38.4 current-process observations outside timed updates", "gpu_buffers":"Production SogGpuTelemetry inventories; each raw snapshot retains IDs and actual capacities"},
            "capabilities":{"process_checkpoint_sampling":sysinfo::IS_SUPPORTED_SYSTEM,"driver_residency":false},
            "peak_rss_bytes":null,"gpu_resident_bytes":null,"total_gpu_allocation_bytes":null,
            "unavailable":{"peak_rss_bytes":"External process supervisor supplies high-water RSS separately", "gpu_resident_bytes":"Portable driver residency measurement unavailable", "total_gpu_allocation_bytes":"Tracked buffers exclude driver, pipelines and unobserved allocations"}})
    }
}
