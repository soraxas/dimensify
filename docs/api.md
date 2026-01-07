# API Surface

!!! note
    This is the minimal planned API. Keep it stable and additive.

## Python viewer API (dimensify-py)

- `DataSource.local()`
- `DataSource.file(path)`
- `DataSource.db(addr)`
- `ViewerClient(source)`
- `log_line_3d(points, color=None, width=None)`
- `log_line_2d(points, color=None, width=None)`
- `log_text_3d(text, position, color=None)`
- `log_text_2d(text, position, color=None)`
- `log_mesh_3d(name, position, scale=None)`
- `log_rect_2d(position, size, rotation=None, color=None)`
- `set_transform(entity, position, rotation, scale)`
- `save(path=None)`
- `clear()`

## Python transport API (dimensify-py)

!!! note
    Requires building `dimensify-py` with `--features transport_webtransport` (or websocket/udp).

```text
The Python client does not read environment variables; pass settings explicitly.
```

- `TransportClient(server_addr=None, mode=None, client_addr=None, cert_digest=None, tick_hz=None, connection=None, endpoint=None)`
- `apply_json(payload, timeout_ms=None)`
- `remove(name, timeout_ms=None)`
- `clear(timeout_ms=None)`
- `list(timeout_ms=None)`
- `transport_enabled()` / `transport_features()` for build-time feature checks.

`mode` accepts `webtransport`, `websocket`, or `udp`.
`connection` accepts `client` (default) or `server`; `endpoint` accepts `controller` (default) or `viewer`.

## Planned additions

- Entity IDs and stable object handles
- File replay + remote streaming in viewer
- Sprite/image commands for 2D

## Protocol notes

- Common primitives will map to WKT binary layouts for fast paths.
- Custom commands can be carried as opaque binary payloads with metadata.

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
    Transport requests are sent as `ViewerRequest` messages over the `StreamReliable` channel.

```text
WebTransport servers are native-only; wasm viewers must connect as clients to a native server (hub or a Python transport session running as `connection="server"`).
```

```text
Payloads are JSON (single command or JSON array of commands).
```

ViewerRequest JSON shape:

```json
{"ApplyJson":{"payload":"[{\"type\":\"Line3d\",\"points\":[[0,0,0],[1,1,1]],\"color\":[1,1,1,1],\"width\":1.0}]"}}
{"Remove":{"name":"cube"}}
{"List":{}}
{"Clear":{}}
```

`payload` is a JSON string containing either a single command or a JSON array of commands.

ViewerResponse JSON shape:

```json
{"Ack":{}}
{"Entities":{"names":["cube","sphere"]}}
{"Error":{"message":"unknown entity 'cube'"}}
```

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
