# Legacy prototypes

Reference sources retained from earlier Dimensify prototypes. Nothing here is
a workspace member, compiled, linted or tested, and none of it is a supported
API. See [architecture](../docs/architecture.md) for the active packages.

| Path | Former location | Contents |
| --- | --- | --- |
| `transport` | `crates/transport` (prototype) | UDP/lightyear command transport |
| `examples` | (added as legacy) | Custom egui widget example for the excluded widgets crate |
| `dimensify-viewer` | `crates/dimensify/src` | Viewer/editor, robot physics, command stream, telemetry and services modules that no active module declared |
| `dimensify-ui` | `crates/ui/src` | egui layout, menu bar and pane widgets that no active module declared |
| `docs` | `docs` | API surface and state notes describing the command-stream prototype |

Revive a prototype only by establishing a supported contract with its own
focused checks, as described in the architecture guide.
