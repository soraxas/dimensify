set dotenv-load := true

# List standalone commands.
@help:
    just --list

# Run the native URDF kinematic viewer.
[positional-arguments]
robosim *args:
    cargo run --locked -p dimensify --features robot --bin robosim -- "$@"

# Run workspace tests with native server support.
tests:
    cargo test --locked --workspace --features dimensify_session/server,dimensify_transport/server

# Headless shared-cube authority.
collab-server:
    cargo run --locked -p dimensify_hub --bin dimensify-room-server

# Two-client invite/edit/presence/resume acceptance against a running authority.
collab-check endpoint="ws://127.0.0.1:5002":
    cargo run --locked -p dimensify_session --example session_check -- {{endpoint}}

collab-test:
    cargo test --locked -p dimensify_transport -p dimensify_session -p dimensify_hub --features dimensify_session/server,dimensify_transport/server

# Create a room in a Dimensify viewer, or pass the printed room/invitation to join.
[positional-arguments]
collab-peer *args:
    cargo run --locked -p dimensify_session --features example-dimensify --example shared_cube -- "$@"

# Local PLY/SOG viewer with file dialog and drag-and-drop.
[positional-arguments]
splat-viewer *args:
    cargo run --locked -p dimensify --features splat --example splat_viewer -- "$@"

# Build the standalone browser viewer; serve splat-web/ over localhost or HTTPS.
splat-web-build:
    cargo build --locked -p dimensify --features splat --example splat_viewer --target wasm32-unknown-unknown
    mise exec -- wasm-bindgen --target web --out-dir splat-web/pkg target/wasm32-unknown-unknown/debug/examples/splat_viewer.wasm

# Discover all examples, build browser targets and refresh the static gallery.
[positional-arguments]
showcase-build *args:
    python3 scripts/showcase.py "$@"

# Refresh example cards without compiling WASM.
showcase-catalog:
    python3 scripts/showcase.py --catalog

# Measure CPU operations (no GPU/FPS claims); choose a fresh output directory.
perf-cpu output="dist/performance":
    python3 scripts/perf.py run --output {{quote(output)}}

# Production loaders/streaming CPU workloads; no GPU timing claims.
perf-pipeline output="dist/performance-pipeline":
    python3 scripts/perf.py run --suite pipeline --output {{quote(output)}}

# Real offscreen GPU completion on a local GPU, separate from hosted CPU CI.
[positional-arguments]
perf-gpu *args:
    cargo run --locked --profile bench -p dimensify_perf --features gpu --bin gpu -- "$@"

# Production packed LOD completion; disk-resolved hierarchy and explicit resource scopes.
[positional-arguments]
perf-lod *args:
    cargo run --locked --profile bench -p dimensify_perf --features lod --bin lod -- "$@"

# Browser viewer replay; serve an instrumented release viewer first.
[positional-arguments]
perf-browser *args:
    node scripts/perf-browser.cjs "$@"

# Install the pinned Python validator used by saved performance reports.
perf-python:
    python3 -m pip install --disable-pip-version-check --quiet --requirement scripts/requirements-perf.txt

# Verified external fixtures, repeated runs, and separate Bencher publication.
[positional-arguments]
perf-scenes *args: perf-python
    python3 scripts/perf_scenes.py "$@"

# Run all 22 portable offscreen GPU correctness cases, serially with count guards.
visual-gpu-tests output="dist/visual-gpu-tests":
    python3 scripts/visual-gpu.py --out {{quote(output)}}

# The same analytic Gaussian scenes run natively and in WASM.
visual-native output="dist/visual-native":
    cargo run --locked --profile ci -p dimensify_perf --features visual --bin visual -- --output {{quote(output)}}

visual-web-build output="dist/visual-web":
    python3 scripts/visual-web.py --out {{quote(output)}}

[positional-arguments]
visual-browser *args:
    node scripts/visual-browser.cjs "$@"

[positional-arguments]
visual-compare *args:
    node scripts/visual-compare.cjs "$@"

ci-visual-native:
    cargo clippy --locked --profile ci -p dimensify_perf --features visual --bin visual --no-deps -- -D warnings
    cargo test --locked --profile ci -p dimensify_perf --features visual --bin visual

ci-visual-wasm:
    cargo clippy --locked --profile ci -p dimensify_perf --features visual --bin visual --target wasm32-unknown-unknown --no-deps -- -D warnings

# Validate benchmark inputs and reporting without contacting Bencher.
ci-perf:
    cargo test --locked -p dimensify_perf --lib
    cargo clippy --locked -p dimensify_perf --all-targets --no-deps -- -D warnings
    python3 -m unittest discover -s scripts -p 'test_perf.py'

# Render-enabled CPU fixtures use a separate cache lane, without a GPU.
ci-perf-pipeline:
    cargo test --locked --profile ci -p dimensify_perf --features lod --lib --bins
    cargo test --locked --profile ci -p dimensify_splat --features test-support --lib test_support::benchmarks
    cargo clippy --locked --profile ci -p dimensify_perf --features lod --all-targets --no-deps -- -D warnings
    python3 -m unittest discover -s scripts -p 'test_perf.py'

###########################
# Standalone checks and publishing
###########################

# Check first-party formatting without rewriting vendored crates.
ci-format:
    cargo fmt --check -p dimensify -p dimensify_ui -p dimensify_transport -p dimensify_session -p dimensify_hub -p dimensify_splat -p dimensify_camera -p dimensify_interpolation -p dimensify_perf -p sog_codec

# Test metadata discovery, packaging, and the checked-in gallery catalog.
ci-scripts: perf-python
    python3 -m unittest discover -s scripts -p 'test_*.py'
    node --test scripts/test_perf_browser.cjs scripts/test_perf_browser_memory.cjs scripts/test_perf_browser_gpu.cjs scripts/perf-browser-cpu.test.cjs scripts/perf-device.test.cjs
    bun test crates/splat/src/streaming/runtime/codec_jobs.test.ts crates/splat/src/streaming/runtime/sog-decode-worker.test.ts
    node --test scripts/visual-compare.test.cjs scripts/visual-browser.test.cjs
    python3 scripts/showcase.py --check
    node --check splat-web/gallery/gallery.mjs
    node --check scripts/check-gallery.cjs

ci-workflows:
    actionlint
    zizmor --offline .github

# Headless libraries, examples and real networking tests; no viewer feature union.
ci-headless: ci-headless-lint ci-headless-test ci-export

ci-headless-lint:
    cargo clippy --locked -p dimensify_transport -p dimensify_session -p dimensify_hub -p dimensify_camera -p dimensify_interpolation -p dimensify_splat -p sog_codec --no-default-features --features dimensify_transport/server,dimensify_transport/client,dimensify_session/server,dimensify_session/client,dimensify_camera/inspect --all-targets --no-deps -- -D warnings

ci-headless-test:
    cargo test --locked -p dimensify_transport -p dimensify_session -p dimensify_hub -p dimensify_camera -p dimensify_interpolation -p dimensify_splat -p sog_codec --no-default-features --features dimensify_transport/server,dimensify_transport/client,dimensify_session/server,dimensify_session/client,dimensify_camera/inspect

ci-export:
    cargo fetch --locked
    python3 scripts/verify-standalone.py

# Native rendering, robot validation and interactive example compilation.
ci-native: ci-native-lint ci-native-test ci-robosim ci-native-examples

ci-native-lint:
    cargo clippy --locked --profile ci -p dimensify -p dimensify_ui -p dimensify_splat -p dimensify_camera -p dimensify_session --features dimensify/robot,dimensify/splat,dimensify/playground,dimensify_session/example-browser,dimensify_session/server --all-targets --no-deps -- -D warnings

ci-native-test:
    cargo test --locked --profile ci -p dimensify -p dimensify_splat -p dimensify_camera --features dimensify/robot,dimensify/splat,dimensify/playground,dimensify_splat/test-support --lib --tests

ci-robosim:
    cargo run --locked --profile ci -p dimensify --features robot --bin robosim -- --check

ci-native-examples:
    cargo build --locked --profile ci -p dimensify_session --features example-browser --examples

# Check browser examples and the opt-in performance library itself.
ci-wasm:
    cargo clippy --locked --profile ci -p sog_codec --features worker --target wasm32-unknown-unknown --no-deps -- -D warnings
    cargo clippy --locked -p dimensify_session --features example-browser --example browser_collaboration --target wasm32-unknown-unknown --no-deps -- -D warnings
    cargo clippy --locked -p dimensify --features playground --example camera_playground --target wasm32-unknown-unknown --no-deps -- -D warnings
    cargo clippy --locked -p dimensify --features splat --example splat_viewer --target wasm32-unknown-unknown --no-deps -- -D warnings
    cargo clippy --locked -p dimensify_splat --features viewer-perf --lib --target wasm32-unknown-unknown --no-deps -- -D warnings

# Build all live examples and stage a clean, complete site (output must be new).
showcase-release output="dist/showcase":
    python3 scripts/showcase.py --profile release --output {{quote(output)}}
