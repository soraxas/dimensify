# Protocol Design

!!! note
    Dimensify is viewer-first. The protocol is action-based and backend-agnostic.

## Principles

- Protocol types are portable and Bevy-free.
- Actions describe intent; components carry data.
- Transport payload format is an implementation detail.
- Stream is canonical; transport is optional.

## Mental model

Dimensify keeps two independent logs:

- **Command log**: authoritative actions (`WorldCommand`) for replay and collaboration.
- **Telemetry/state log**: high-rate state events (Rerun-like) for inspection and plotting.

Replication is an optional live transport that emits new command entries; it does not replace the
command log or telemetry log.

## Telemetry (TODO)

We plan a Rerun-like telemetry stream for high-rate state events.

- **Viewer ECS** stays as the current state used for rendering.
- **Telemetry store** lives outside ECS (Arrow/Parquet or similar) for replay and queries.
- **TODO**: define timeline semantics (`sim_time`, `frame`, and custom log paths).

### Proposed telemetry envelope

We follow Rerun-style log paths and timelines.

```text
TelemetryEvent { path, time, payload, metadata }
TelemetryTime { timeline, value }
TelemetryPayload { Scalar | Vec2 | Vec3 | Vec4 | Text | Blob }
```

This keeps telemetry POD-friendly and decoupled from Bevy types.

## User flow

```mermaid
flowchart LR
    A[Python/Rust/WASM client] --> B{Intent}
    B -->|Modify scene| C["WorldCommand <br> Spawn/Insert/Update/Remove/Despawn"]
    B -->|Stream data| D[TelemetryEvent <br> log path + timeline + payload]
    C --> E[ProtoRequest::Apply]
    D --> F[Telemetry store <br> Rerun/Arrow]
    E --> G["Viewer (Bevy ECS)"]
    F --> G
    G --> H[Rendered scene + plots]
```

## Core types

`WorldCommand` expresses actions:

- `Spawn { components }`
- `Insert { entity, components }`
- `Update { entity, component }`
- `Remove { entity, component }`
- `Despawn { entity }`
- `Clear`

`Component` carries data (examples):

- `Name { value }`
- `Mesh3d { name, position, scale }`
- `Line3d { name, points, color, width }`
- `Transform3d { transform }`
- `Rect2d { name, position, size, rotation, color }`

`ComponentKind` is used by `Remove` to target a component type.

## POD guidance

Protocol payloads should remain POD-like and stable:

- Use `[f32; N]` for vectors/quaternions.
- Use `String`/`Vec<u8>` for identifiers and blobs.
- Avoid Bevy render types (e.g., `Mesh`) in the protocol.
- Prefer references (`MeshRef`/URI) or `Blob` payloads for heavy assets.
- POD vector types (`Vec2`/`Vec3`/`Vec4`/`Quat`) serialize as JSON arrays for easy interop.

## Bevy adapters (recommended)

Define protocol types as POD and add feature-gated adapters for Bevy types in a Bevy-only crate
or module. This avoids leaking Bevy render types to Python while keeping native ergonomics.

```rust
#[cfg(feature = "bevy")]
impl From<bevy::prelude::Transform> for Transform {
    fn from(t: bevy::prelude::Transform) -> Self {
        Self {
            position: Vec3([t.translation.x, t.translation.y, t.translation.z]),
            rotation: Quat([t.rotation.x, t.rotation.y, t.rotation.z, t.rotation.w]),
            scale: Vec3([t.scale.x, t.scale.y, t.scale.z]),
        }
    }
}
```

## Transport requests

Transport uses `ProtoRequest::Apply { payload }` with JSON payloads (single `WorldCommand` or an array).
`ProtoRequest` also supports `List` and `Clear` control requests.

```mermaid
sequenceDiagram
    participant Client as Client (Rust/WASM/Python)
    participant Transport as dimensify_protocol
    participant Viewer as Viewer (dimensify)
    participant Stream as Stream Log

    Client->>Transport: ProtoRequest::Apply { payload }
    Transport->>Viewer: ProtoRequest
    Viewer->>Viewer: Decode WorldCommand(s)
    Viewer->>Stream: Append WorldCommand(s)
    Viewer-->>Client: ProtoResponse::Ack
```

## Example payloads

Spawn a cube and a line in a single command:

```json
{
  "Spawn": {
    "components": [
      { "type": "Name", "value": "demo_cube" },
      {
        "type": "Mesh3d",
        "name": "demo_cube",
        "position": [0.0, 0.0, 0.0],
        "scale": [1.0, 1.0, 1.0]
      },
      {
        "type": "Line3d",
        "points": [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
        "color": [1.0, 1.0, 1.0, 1.0],
        "width": 1.0
      }
    ]
  }
}
```

Update a transform:

```json
{
  "Update": {
    "entity": "demo_cube",
    "component": {
      "type": "Transform3d",
      "transform": {
        "position": [0.2, 0.4, 0.1],
        "rotation": [0.0, 0.0, 0.0, 1.0],
        "scale": [1.0, 1.0, 1.0]
      }
    }
  }
}
```

## Bevy wrapper pattern

Use `DimensifyComponent` to map wrapper components to protocol `Component` variants.

```rust
use bevy::prelude::*;
use dimensify_component_derive::DimensifyComponent;

#[derive(Component, Clone)]
pub struct Line3dComponent {
    pub name: Option<String>,
    pub points: Vec<[f32; 3]>,
    pub color: [f32; 4],
    pub width: f32,
}

#[derive(DimensifyComponent, Clone)]
#[dimensify(command = "Line3d")]
pub struct Line3dWrapper {
    pub name: Option<String>,
    pub points: Vec<[f32; 3]>,
    pub color: [f32; 4],
    pub width: f32,
}
```

Use `#[dimensify(into)]` on fields that should call `.into()` during conversion.
