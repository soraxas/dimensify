use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HubMode {
    Server,
    Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
