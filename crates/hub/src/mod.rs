//! Headless session hosting with application-selected schemas and configuration.
mod host;
mod http;
pub use host::{HostConfig, configure, run};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Legacy launch mode, retained for configuration compatibility.
pub enum HubMode {
    Server,
    Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Legacy mode-only configuration; use HostConfig for a running host.
pub struct HubConfig {
    pub mode: HubMode,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            mode: HubMode::Server,
        }
    }
}
