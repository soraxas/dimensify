use serde::{Deserialize, Serialize};

use crate::prelude::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Log path, Rerun-style (e.g. "robot/imu/accel").
    pub path: String,
    /// Timeline and timestamp.
    pub time: TelemetryTime,
    /// Typed payload.
    pub payload: TelemetryPayload,
    /// Optional metadata (unit, description).
    pub metadata: Option<TelemetryMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryTime {
    /// Timeline name (e.g. "sim_time", "frame").
    pub timeline: String,
    /// Timeline value (seconds, frame index, etc).
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TelemetryPayload {
    Scalar { value: f64 },
    Vec2 { value: Vec2 },
    Vec3 { value: Vec3 },
    Vec4 { value: Vec4 },
    Text { value: String },
    Blob { mime: Option<String>, data: Vec<u8> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryMetadata {
    /// Optional unit string (e.g. "m/s", "deg").
    pub unit: Option<String>,
    /// Optional long description.
    pub description: Option<String>,
}
