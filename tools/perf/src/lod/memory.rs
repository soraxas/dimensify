//! Current-process observations outside timed updates.
use dimensify_perf::process_usage::ProcessSampler;
use serde_json::{Value, json};
pub struct ProcessUsage {
    process: ProcessSampler,
    rows: Vec<Value>,
}
impl ProcessUsage {
    pub fn new() -> Self {
        Self {
            process: ProcessSampler::default(),
            rows: Vec::new(),
        }
    }
    pub fn checkpoint(&mut self, phase: &str) {
        self.rows.push(self.process.checkpoint(phase));
    }
    pub fn report(self, gpu: Vec<Value>) -> Value {
        json!({"schema_version":1,"checkpoints":self.rows,"gpu_checkpoints":gpu,
            "method":{"process":"sysinfo 0.38.4 current-process observations outside timed updates", "gpu_buffers":"Production SogGpuTelemetry inventories; each raw snapshot retains IDs and actual capacities"},
            "capabilities":{"process_checkpoint_sampling":sysinfo::IS_SUPPORTED_SYSTEM,"driver_residency":false},
            "peak_rss_bytes":null,"gpu_resident_bytes":null,"total_gpu_allocation_bytes":null,
            "unavailable":{"peak_rss_bytes":"External process supervisor supplies high-water RSS separately", "gpu_resident_bytes":"Portable driver residency measurement unavailable", "total_gpu_allocation_bytes":"Tracked buffers exclude driver, pipelines and unobserved allocations"}})
    }
}
