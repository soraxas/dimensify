# Overview

Dimensify is a viewer-first collection of reusable Rust/Bevy packages, with
optional native/WebGPU viewer composition and headless collaboration.
Applications can use a camera or splat library without adopting a room server,
or host shared documents without installing a renderer.

## Available now

- **Splat viewer:** local PLY/bundled SOG files and HTTP(S) resources, plus hosted
  hierarchy manifests through the shared streaming pipeline. Native and browser
  use the same Feathers controls, with platform-specific file/input adapters.
- **Camera foundation:** poses, framing, transitions, motion helpers and optional
  inspect controls. Hosts retain input and final camera ownership.
- **Collaboration:** application-defined shared documents, rooms, invite codes,
  presence, optional camera presentation and private reconnect/resume credentials.
  A standalone host supports WebSocket/WebTransport without graphics dependencies.
- **Native robot viewer:** URDF loading, joint controls and forward kinematics.
  This does not yet provide physics dynamics or collision simulation.

## Choose an entry point

| Goal | Guide |
| --- | --- |
| Open a local or hosted splat | [Splat viewer](splat-viewer.md) |
| Run the native robot viewer | [RoboSim](../README.md#run-robosim) |
| Connect two viewers or inspect package examples | [Examples](examples.md) |
| Host a room or register a document | [Collaboration](collaboration.md) |
| Understand package ownership and application adapters | [Architecture](architecture.md) |
| See accepted extractions and remaining work | [Migration checklist](../docs-dev/migration-checklist.md) |

## Direction and limits

Physics backends, browser-local robot simulation and a Python API that can
launch or attach to a separate viewer are planned. Ordered simulation streams,
telemetry, discovery/matchmaking and WebRTC/P2P are also future work. Existing
room synchronization is a document protocol, not those simulation APIs.

!!! warning "Prototype configuration is not a current API"
    The excluded protocol/Python/legacy transport sources retain earlier
    command-stream and telemetry designs. Their `DIMENSIFY_DATA_SOURCE`,
    `DIMENSIFY_TELEMETRY_*` and `DIMENSIFY_TRANSPORT_*` settings are not supported
    launch instructions for the active packages. Use the guides above for
    current viewer commands and host configuration.
