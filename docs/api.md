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
