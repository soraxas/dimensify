# API Surface

!!! note
    This is the minimal planned API. Keep it stable and additive.

## Python viewer API (dimensify-py)

- `DataSource.local()`
- `DataSource.file(path)`
- `DataSource.tcp(addr)`
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

- `DIMENSIFY_WIDGET_SOURCE`: `local` | `file` | `tcp` | `db`
- `DIMENSIFY_WIDGET_FILE`: path to JSONL file (when `file`)
- `DIMENSIFY_WIDGET_TCP_ADDR`: TCP address (not yet implemented)
- `DIMENSIFY_WIDGET_DB_ADDR`: DB address (not yet implemented)

JSONL format (one command per line):

```json
{"type":"Label","id":"demo_label","text":"Hello"}
{"type":"Button","id":"demo_button","text":"Click me"}
{"type":"Checkbox","id":"demo_checkbox","text":"Toggle option","checked":true}
```

Example file:

- `dimensify/examples/widget_commands.jsonl`
