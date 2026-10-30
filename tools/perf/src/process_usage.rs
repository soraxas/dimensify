//! Current-process observations outside timed benchmark updates.
use serde_json::{Value, json};
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

pub struct ProcessSampler {
    system: System,
    pid: Pid,
    start: Instant,
}
impl Default for ProcessSampler {
    fn default() -> Self {
        Self {
            system: System::new(),
            pid: Pid::from_u32(std::process::id()),
            start: Instant::now(),
        }
    }
}
impl ProcessSampler {
    pub fn checkpoint(&mut self, phase: &str) -> Value {
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
        json!({"phase":phase,"elapsed_ns":self.start.elapsed().as_nanos(),
            "rss_bytes":process.map(|p|p.memory()), "virtual_memory_bytes":process.map(|p|p.virtual_memory()),
            "accumulated_cpu_ms":process.map(|p|p.accumulated_cpu_time())})
    }
}
