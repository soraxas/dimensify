# Dimensify

[![CI](https://github.com/soraxas/dimensify/actions/workflows/ci.yml/badge.svg)](https://github.com/soraxas/dimensify/actions/workflows/ci.yml)
[![CPU performance](https://github.com/soraxas/dimensify/actions/workflows/performance.yml/badge.svg?branch=main)](https://github.com/soraxas/dimensify/actions/workflows/performance.yml)
[![Bencher CPU results](https://img.shields.io/badge/Bencher-CPU%20results-0073b7)](https://bencher.dev/perf/dimensify)

[**Try the live example gallery**](https://cs.tinyiu.com/dimensify/)

Dimensify provides reusable viewer plugins, an optional native and browser
URDF kinematic viewer, and headless collaboration for application-defined documents.

- [Architecture](docs/architecture.md): active packages, application boundaries and planned simulation APIs.
- [Collaboration](docs/collaboration.md): rooms, invitations, document synchronization,
  presence, native/WASM clients and a standalone WebSocket/WebTransport host.
- [RoboSim](#run-robosim): local or URL robot loading, joint controls and forward kinematics,
  natively or in the browser gallery.
- [Example gallery](https://cs.tinyiu.com/dimensify/): browser, native and headless demonstrations.
- [Build and CI](docs/ci.md): pinned tools, Rust cache, local checks and GitHub Pages.
- [Splat performance](docs/performance.md): CPU/LOD benchmarks, GPU and browser replays;
  [Bencher results](https://bencher.dev/perf/dimensify) include CPU and native GPU
  histories, with browser observations on the separate `browser-diagnostics` branch.
- [Examples and tests](docs/examples.md): one runnable consumer per active crate
  and a two-peer Dimensify viewer walkthrough.
- [Migration checklist](docs-dev/migration-checklist.md): active boundaries and remaining work.

Physics dynamics, browser-local robot simulation and the Python launch/attach
API are planned. The older editor, UDP and Python prototypes remain dormant.

## Run RoboSim

From this directory, with the repository Rust toolchain and `just` installed:

```sh
just robosim
just robosim /absolute/path/to/robot.urdf
just robosim https://github.com/Daniella1/urdf_files_dataset/blob/main/urdf_files/oems/xacro_generated/franka_emika/franka_description/robots/dual_panda/dual_panda.urdf
```

The default is an embedded demo arm, so no additional assets are needed.
`Tab` selects a joint, Left/Right moves it, and `R` resets joints within their
limits. Drag with the left mouse button to orbit, right-drag or Shift-left-drag
to pan, scroll to zoom, `F` to frame the robot, and Escape to quit (native only). Camera
manipulation uses `dimensify_camera/inspect`; losing focus cancels pending motion.
The initial robot framing and explicit `F` framing preserve their existing margin.

For headless validation or an offscreen render:

```sh
just robosim --check
just robosim /absolute/path/to/robot.urdf --check
just robosim --screenshot /tmp/robosim.png
```

The underlying command is `cargo run -p dimensify --features robot --bin robosim`.
The optional `robot` feature adds the standalone kinematic viewer.
It supports fixed, revolute, continuous, prismatic and mimic joints, URDF primitives, and
STL/OBJ/Collada meshes. Mesh paths are relative to the URDF; `package://` paths
are searched under its ancestor directories. Materials use colors; texture
maps and OBJ material files from URLs are not displayed. Unsupported joint types
and missing meshes fail before a native window opens.

URL sources accept HTTP(S) URDFs and GitHub file pages, which are read as raw
content. A `package://name/...` mesh resolves first from a URL directory named
`name`, then from each parent directory; `--package name=https://base/` overrides
one package. Each unique mesh downloads once, with size limits per file and in
total. The browser build in the [example gallery](https://cs.tinyiu.com/dimensify/)
reads the same options from `?urdf=` and repeated `&package=` query parameters;
mesh hosts must allow CORS. Robot controls there need a keyboard.

This is a kinematic viewer, without dynamics or collision simulation. The old
egui editor, IK and UDP/Python integrations are not restored; the old
`transport_udp,robot` recipe no longer applies.

## View Gaussian splats

The optional [PLY and SOG viewer](docs/splat-viewer.md) supports Open File,
drag-and-drop, HTTP(S) files and hosted hierarchy manifests. It starts in Fly
at the origin, preserves the view on load, and provides Reset, explicit Frame
and an Orbit switch. It runs natively or as a standalone WebGPU website.

```sh
cargo run --locked -p dimensify --features splat --example splat_viewer
```
