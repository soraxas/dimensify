# Progressive PLY Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development for the
> independent decoder and GPU adapter tasks; integrate and verify in this session.

**Goal:** Opt-in rendering of remote Gaussian PLY before its download completes.
**Architecture:** Bounded incremental decoder and transport feed append-only GPU
planes through Bevy systems. Preserve the whole-file asset API and default path.
**Tech Stack:** Rust, Bevy, ply-rs, ehttp, WebGPU.
**Spec:** docs-dev/progressive-ply-design.md

## Global constraints

ASCII source/docs; native and WASM; no new dependencies; 256 MiB input cap;
whole-file defaults preserved; no baseline approvals or generated binaries.

## Task 1: Incremental decoder

Files: crates/splat/src/progressive/decoder.rs and decoder_tests.rs.
Interface: Decoder::new(Option<u64>); push(&[u8]) -> Result<Vec<Batch>, String>;
finish() -> Result<(), String>; Batch { cloud: PlanarGaussian3d, count: usize,
total: usize, bounds: (Vec3, Vec3), support: f32 }. Expose unsupported encoding
as a typed outcome before any batches, permitting whole-file fallback.

- [x] Add failing split-header/record, batch parity and malformed-input tests.
- [x] Run focused tests and observe missing progressive behavior.
- [x] Implement bounded header/record parsing, schema checks and conversion.
- [x] Compare every final attribute with whole-file decoding, including SH3.

## Task 2: Append GPU adapter

File: crates/splat/src/renderer/progressive.rs, register in renderer/mod.rs.
Interface: ProgressiveGpu::new(world: &mut World, total: usize) -> Result<Self,
String>; append(world, &Batch); entity(); retire(world). Adapter owns reserved
handles, four fixed-capacity planes, sorting buffer and transfer cancellation.
Use existing GPU publication/readiness; never upload earlier rows again.

- [x] Add failing native GPU tests for append preservation and retirement.
- [x] Allocate fixed capacity with invisible zero tail and checked limits.
- [x] Stage new plane bytes only, using synchronized extraction publication.
- [x] Run GPU readbacks and check upload byte counts and zero tail.

## Task 3: Transport and viewer lifecycle

Files: progressive/mod.rs, progressive/transport.rs, viewer/files.rs,
viewer/launch.rs, viewer/mod.rs, viewer/web.rs, scene.rs.

- [x] Add failing slow HTTP and cancellation/lifecycle tests.
- [x] Implement bounded queue, fallback and generation-owned task lifetime.
- [x] Integrate opt-in native/browser options and whole-file fallback.
- [x] Test successful completion, late failure rollback and supersession.

## Task 4: Acceptance and documentation

- [x] Update viewer docs, migration checklist and performance coverage gaps.
- [x] Run native suite, WASM check, formatting and Clippy.
- [x] Run analytic native captures and progressive GPU checks.
- [x] Review diff, address findings, report evidence and unavailable checks.
