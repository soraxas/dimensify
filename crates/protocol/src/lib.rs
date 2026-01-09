use serde::{Deserialize, Serialize};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

mod components;
mod errors;

use crate::components::prelude::*;

pub use errors::TransportError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WktKind {
    Line3d = 1,
    Text3d = 2,
    Mesh3d = 3,
    Transform = 4,
    Line2d = 5,
    Text2d = 6,
    Rect2d = 7,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct WktHeader {
    pub kind: u32,
    pub version: u16,
    pub flags: u16,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Line3d {
    pub color: [f32; 4],
    pub width: f32,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Line2d {
    pub color: [f32; 4],
    pub width: f32,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Text3d {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Text2d {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Mesh3d {
    pub position: [f32; 3],
    pub scale: [f32; 3],
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Rect2d {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct Transform3d {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Serialize, Deserialize, FromBytes, Immutable, IntoBytes, KnownLayout,
)]
pub struct ByteSpan {
    pub offset: u32,
    pub len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Component {
    Name {
        value: String,
    },
    Line3d {
        #[serde(default)]
        name: Option<String>,
        points: Vec<[f32; 3]>,
        color: [f32; 4],
        width: f32,
    },
    Line2d {
        #[serde(default)]
        name: Option<String>,
        points: Vec<[f32; 2]>,
        color: [f32; 4],
        width: f32,
    },
    Text3d {
        #[serde(default)]
        name: Option<String>,
        text: String,
        position: [f32; 3],
        color: [f32; 4],
    },
    Text2d {
        #[serde(default)]
        name: Option<String>,
        text: String,
        position: [f32; 2],
        color: [f32; 4],
    },
    Mesh3d {
        #[serde(default)]
        name: Option<String>,
        position: [f32; 3],
        scale: [f32; 3],
    },
    Rect2d {
        #[serde(default)]
        name: Option<String>,
        position: [f32; 2],
        size: [f32; 2],
        rotation: f32,
        color: [f32; 4],
    },
    Transform3d {
        position: [f32; 3],
        rotation: [f32; 4],
        scale: [f32; 3],
    },
    Binary {
        header: WktHeader,
        payload: Vec<u8>,
        meta: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Telemetry {
    Tick { value: u64 },
}

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
    Vec2 { value: [f32; 2] },
    Vec3 { value: [f32; 3] },
    Vec4 { value: [f32; 4] },
    Text { value: String },
    Blob { mime: Option<String>, data: Vec<u8> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryMetadata {
    pub unit: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorldCommand {
    Spawn {
        components: Vec<Component>,
    },
    Insert {
        entity: String,
        components: Vec<Component>,
    },
    Update {
        entity: String,
        component: Component,
    },
    Remove {
        entity: String,
        component: ComponentKind,
    },
    Despawn {
        entity: String,
    },
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentKind {
    Name,
    Line3d,
    Line2d,
    Text3d,
    Text2d,
    Mesh3d,
    Rect2d,
    Transform3d,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SceneRequest {
    Apply { payload: String },
    Remove { name: String },
    List,
    Clear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerEntityKind {
    Mesh3d,
    Line3d,
    Line2d,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerEntityInfo {
    pub id: u64,
    pub name: Option<String>,
    pub components: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerResponse {
    Ack,
    Entities { entities: Vec<ViewerEntityInfo> },
    Error { message: String },
}

pub trait DimensifyComponent {
    fn to_component(&self) -> Component;
}
