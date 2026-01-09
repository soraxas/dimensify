# API Surface

!!! note
    This is the minimal planned API. Keep it stable and additive.

## Python viewer API (dimensify-py)

Local command recorder used to write JSONL files for replay.

- `DataSource.local()`
- `DataSource.file(path)`
- `DataSource.db(addr)` (not implemented)
- `ViewerClient(source)`
- `save(path=None)` → writes JSONL replay
- `clear()`

## Python transport API (dimensify-py)

!!! note
    Requires building `dimensify-py` with `--features transport_webtransport` (or websocket/udp).

```text
The Python client does not read environment variables; pass settings explicitly.
```

- `TransportClient(server_addr=None, mode=None, client_addr=None, cert_digest=None, tick_hz=None, connection=None, endpoint=None)`
- `list(timeout_ms=None)` → list of `EntityInfo { id, name, components }`
- `transport_enabled()` / `transport_features()` / `system_info()` for build-time feature checks.

!!! note
    `TransportClient.apply()` is deprecated; use `World` and `Component` for typed commands.

## Python telemetry API (dimensify-py)

!!! note
    Telemetry writes JSONL events for replay/inspection. Viewer ingestion is file-based for now.

- `TelemetryClient(path)`
- `log_scalar(path, time, value, timeline=None, unit=None, description=None)`
- `log_vec3(path, time, value, timeline=None, unit=None, description=None)`
- `log_text(path, time, value, timeline=None, unit=None, description=None)`

## Python world-style API (components)

Bevy-like `World` and component objects that serialize to scene commands.

```python
from dimensify import World, Component, Shape3d, Vec3, Quat

world = World(server_addr="127.0.0.1:6210", mode="udp")
entity = world.spawn(
    Component.name("cube"),
    Component.mesh_3d(Shape3d.cuboid(half_size=Vec3(0.5, 0.5, 0.5))),
    Component.material_from_color(0.2, 0.6, 1.0, 1.0),
    Component.transform(
        translation=Vec3(0.0, 0.0, 0.0),
        rotation=Quat(0.0, 0.0, 0.0, 1.0),
        scale=Vec3(1.0, 1.0, 1.0),
    ),
)
print(world.list())
```

!!! note
    `World` uses the transport client (remote viewer). Local viewer bootstrapping is planned.

`World.spawn()` accepts any mix of `Component` instances and returns the created entity when
`wait=True` (default).

`mode` accepts `webtransport`, `websocket`, or `udp`.
`connection` accepts `client` (default) or `server`; `endpoint` accepts `controller` (default) or `viewer`.

Primitive helpers: `Vec2`, `Vec3`, `Vec4`, `Quat`, `Dir2`, `Dir3`, `Dir4`.

`Component` helpers: `name`, `transform`, `mesh_3d`, `material_from_color`.

## Planned additions

- Entity IDs and stable object handles
- File replay + remote streaming in viewer
- Sprite/image commands for 2D

## Protocol notes

- POD primitives (`Vec2`/`Vec3`/`Vec4`/`Quat`) serialize as JSON arrays for easy interop.
- Transport uses Lightyear message serialization; file replay uses JSONL.

!!! note
    Bevy wrappers can derive `DimensifyComponent` to map a component to a protocol
    `Component` variant and send it via transport. Use `#[dimensify(command = "...")]`
    to select the component variant and `#[dimensify(into)]` to call `Into` on a field.

## Widget command stream (viewer UI)

!!! note
    Widget commands are a separate stream from scene commands. The viewer reads a JSONL file and registers widgets dynamically. An egui context must be active (e.g., dev UI enabled).

- `DIMENSIFY_WIDGET_SOURCE`: `local` | `file` | `db`
- `DIMENSIFY_WIDGET_FILE`: path to JSONL file (when `file`)
- `DIMENSIFY_WIDGET_DB_ADDR`: DB address (not yet implemented)

JSONL format (one command per line):

```json
{"type":"Label","id":"demo_label","text":"Hello"}
{"type":"Button","id":"demo_button","text":"Click me"}
{"type":"Checkbox","id":"demo_checkbox","text":"Toggle option","checked":true}
```

Example file:

- `dimensify/examples/widget_commands.jsonl`

## Transport commands (lightyear)

!!! note
    Transport requests are sent as `ProtoRequest` messages over the `StreamReliable` channel.

```text
WebTransport servers are native-only; wasm viewers must connect as clients to a native server (hub or a Python transport session running as `connection="server"`).
```

ProtoRequest JSON shape:

```json
{"ApplyCommand":{"Spawn":{"components":[{"Name":"cube"}]}}}
{"ApplyCommand":{"Remove":{"entity":123456,"component":42}}}
{"List":{}}
```

`Remove` uses the component id from `ProtoResponse::Entities`.

## Telemetry (planned transport)

Telemetry is currently file-based (JSONL) via `TelemetryClient`. A streaming
telemetry layer (Impeller-like or Rerun) is planned for:

- high-rate streaming
- schema discovery
- history queries (`latest_at`, time-range)

Telemetry events use Rerun-style log paths and timelines.

ProtoResponse JSON shape:

```json
{"Ack":{}}
{"CommandResponseEntity":123456789}
{"Entities":{"entities":[{"id":123,"name":"cube","components":[{"id":42,"name":"bevy_transform::components::transform::Transform"}]}]}}
{"Error":{"message":"unknown entity 'cube'"}}
```

### Telemetry environment variables

Used by the viewer when loading telemetry from a file.

- `DIMENSIFY_TELEMETRY_SOURCE`: `local` | `file`
- `DIMENSIFY_TELEMETRY_FILE`: path to telemetry JSONL (when `file`)

### Transport environment variables

Used by the viewer (server) and the controller client when defaults are not overridden.

- `DIMENSIFY_TRANSPORT_MODE`: `webtransport` | `websocket` | `udp`
- `DIMENSIFY_TRANSPORT_CONNECTION`: `server` | `client`
- `DIMENSIFY_TRANSPORT_ENDPOINT`: `viewer` | `controller`
- `DIMENSIFY_TRANSPORT_SERVER_ADDR`: `host:port`
- `DIMENSIFY_TRANSPORT_CLIENT_ADDR`: `host:port` (udp only)
- `DIMENSIFY_TRANSPORT_CERT_DIGEST`: hex SHA-256 (webtransport client)
- `DIMENSIFY_TRANSPORT_CERT_PATH`: path to `cert.pem` (webtransport server)
- `DIMENSIFY_TRANSPORT_CERT_KEY_PATH`: path to `key.pem` (webtransport server)
- `DIMENSIFY_TRANSPORT_TICK_HZ`: tick rate (float)
