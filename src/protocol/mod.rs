use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    Line3d {
        points: Vec<[f32; 3]>,
        color: [f32; 4],
        width: f32,
    },
    Text3d {
        text: String,
        position: [f32; 3],
        color: [f32; 4],
    },
    Mesh3d {
        name: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    Transform {
        entity: String,
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Telemetry {
    Tick { value: u64 },
}
