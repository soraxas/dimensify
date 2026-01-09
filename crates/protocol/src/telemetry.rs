use serde::{Deserialize, Serialize};

use crate::prelude::{Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub path: String,
    pub time: TelemetryTime,
    pub payload: TelemetryPayload,
    pub metadata: Option<TelemetryMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryTime {
    pub timeline: String,
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
    pub unit: Option<String>,
    pub description: Option<String>,
}
