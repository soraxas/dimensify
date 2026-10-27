# Color definitions
_GRAY := "\\033[0;30m"
_GREEN := "\\033[0;32m"
_END := "\\033[m"
_BOLD := "\\x1b[1m"
_BLUE := "\\033[36m"
_INFO := "{_BLUE}{_BOLD}"
_INFO_LIGHT := "{_BLUE}"
_SUCCESS := "{_GREEN}{_BOLD}"
_SUCCESS_LIGHT := "{_GREEN}"

set dotenv-load := true

# Help command: Display all available recipes
@help:
    just --list


py-shell:
    #!/bin/bash
    cd python && uv run python -i <(echo 'import dimensify as d; w = d.World(server_addr="127.0.0.1:6210", mode="udp"); print("d = dimensify, w = world")')

# ▶️ Build/install Python bindings with uv (local venv)
python-dev-setup +features='-F transport_udp':
    @# if we dont have .venv/, create it
    @# some distro, eg mac or even ubuntu ships their own sitecustomize, which
    @# blocks (takes priority) than maturin's hook
    @# see https://github.com/PyO3/maturin-import-hook/discussions/26
    @cd python && [ -d .venv ] || uv venv --managed-python
    # setup hooks to auto re-compile when rs source changes are detected
    cd python && uv run -m maturin_import_hook site install --force --args '{{features}}'
    cd python && uv run -m maturin develop {{features}} --uv

@_transport-demo_py:
    @# run python controller script
    @cd python && dimensify_protocol_MODE=udp \
      dimensify_protocol_CONNECTION=client \
      dimensify_protocol_ENDPOINT=controller \
      dimensify_protocol_SERVER_ADDR=127.0.0.1:6210 \
      uv run python examples/example_transport.py

@_transport-demo_rust:
    @# build viewer ahead of time so the controller doesn't race compilation
    cargo build --features transport_udp
    @# run viewer in the background
    @DIMENSIFY_VIEWER_MODE=3d \
      dimensify_protocol_MODE=udp \
      dimensify_protocol_CONNECTION=server \
      dimensify_protocol_ENDPOINT=viewer \
      dimensify_protocol_SERVER_ADDR=127.0.0.1:6210 \
      cargo run --features transport_udp

[positional-arguments]
robosim *args:
    cargo run -p dimensify --features robot --bin robosim -- "$@"

# ▶️ Run a transport demo (viewer + python controller)
[parallel]
transport-demo: _transport-demo_rust _transport-demo_py

# ▶️ Run Rust tests
tests:
    cargo test --locked --workspace --features dimensify_session/server,dimensify_transport/server


###########################
# Desktop Development
###########################

# ▶️ Run desktop version in development
desktop-dev:
    cargo run --features gsplat

# ▶️ Run desktop version in development - watch mode
desktop-dev-watch:
    cargo watch -q -c -x 'run'

# ▶️ Run desktop version in development - watch mode
desktop-dev-watch-dyn:
    cargo watch -q -c -x 'run --features bevy/dynamic_linking'

# ⚙️ Build desktop version
desktop-build: #
    cargo build --release
    rm -rf ./target/release/assets
    mkdir ./target/release/assets
    cp -r ./assets ./target/release

###########################
### WebAssembly Development
###########################

# ▶️ Run wasm version in development mode via wasm-server-runner
wasm-dev:
    @echo "Once started, to access the page with the wasm-bindgen bindings, open http://127.0.0.1:3000/dev.html"
    @echo ""
    cargo run --target wasm32-unknown-unknown


# ▶️ Run wasm version in development mode (watch mode)
wasm-dev-watch:
    @echo "Once started, to access the page with the wasm-bindgen bindings, open http://127.0.0.1:3000/dev.html"
    @echo ""
    cargo watch -q -c -x 'run --target wasm32-unknown-unknown'


### ▶️ Run wasm version in development mode (no debug mode - lighter bundle)
wasm-dev-release:
    @echo "Once started, to access the page with the wasm-bindgen bindings, open http://127.0.0.1:3000/dev.html"
    @echo ""
    cargo run --release --target wasm32-unknown-unknown

###########################
# Port forwarding
###########################
# ▶️ Forwards port 3000 to localhost.run (to access from mobile)
forward-fallback:
    ssh -R 80:localhost:3000 localhost.run

# ▶️ Forwards port 3000 to ngrok (to access from mobile on a secure origin)
forward:
    @command -v ngrok &> /dev/null && ngrok http 3000 || echo "{_BOLD}ngrok could not be found{_END} - infos to install it are available here: https://ngrok.com\nIf you don't wish to install it, you can use {_BOLD}just forward-fallback{_END}"

###########################
# WebAssembly Build
###########################
# ⚙️ Build wasm version
wasm-build:
    cargo build --release --target wasm32-unknown-unknown
    wasm-bindgen --out-dir ./www/public/out --target web ./target/wasm32-unknown-unknown/release/dimensify.wasm

# 🔩Optimize wasm file size
wasm-opt:
    wasm-opt -Os -o ./www/public/out/dimensify.wasm ./www/public/out/dimensify.wasm

# ⚙️ Build wasm version with optimized file size
wasm-build-opt:
    just wasm-build
    just wasm-opt

###########################
### Website Build and Development
###########################

# ▶️ Build wasm and launch website dev server via vite
www-dev:
    just wasm-build
    just www-dev-only

# ▶️ Launch vite dev server (doesn't build wasm)
www-dev-only:
    cd www && npm run dev -- --host --port 3000

# ⚙️ Build wasm and build website
www-build:
    just wasm-build
    just www-build-only

# ⚙️ Build vite bundle (doesn't build wasm)
www-build-only:
    cd www && npm run build

# ⚙️ Build wasm (optimized wasm file size) and build website
www-build-opt:
    just wasm-build-opt
    just www-build-only

# ▶️ Preview website's build
www-preview:
    cd www && npm run preview -- --host --port 3000

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

###########################
# Standalone checks and publishing
###########################

# Check first-party formatting without rewriting vendored crates.
ci-format:
    cargo fmt --check -p dimensify -p dimensify_ui -p dimensify_transport -p dimensify_session -p dimensify_hub -p dimensify_splat -p dimensify_camera -p dimensify_interpolation

# Test metadata discovery, packaging, and the checked-in gallery catalog.
ci-scripts:
    python3 -m unittest discover -s scripts -p 'test_*.py'
    python3 scripts/showcase.py --check
    node --check splat-web/gallery/gallery.mjs
    node --check scripts/check-gallery.cjs

ci-workflows:
    actionlint
    zizmor --offline .github

# Headless libraries, examples and real networking tests; no viewer feature union.
ci-headless:
    cargo clippy --locked -p dimensify_transport -p dimensify_session -p dimensify_hub -p dimensify_camera -p dimensify_interpolation -p dimensify_splat --no-default-features --features dimensify_transport/server,dimensify_transport/client,dimensify_session/server,dimensify_session/client,dimensify_camera/inspect --all-targets --no-deps -- -D warnings
    cargo test --locked -p dimensify_transport -p dimensify_session -p dimensify_hub -p dimensify_camera -p dimensify_interpolation -p dimensify_splat --no-default-features --features dimensify_transport/server,dimensify_transport/client,dimensify_session/server,dimensify_session/client,dimensify_camera/inspect
    cargo fetch --locked
    python3 scripts/verify-standalone.py

# Native rendering, robot validation and interactive example compilation.
ci-native:
    cargo clippy --locked -p dimensify -p dimensify_ui -p dimensify_splat -p dimensify_camera -p dimensify_session --features dimensify/robot,dimensify/splat,dimensify/playground,dimensify_session/example-browser,dimensify_session/server --all-targets --no-deps -- -D warnings
    cargo test --locked -p dimensify -p dimensify_splat -p dimensify_camera --features dimensify/robot,dimensify/splat,dimensify/playground,dimensify_splat/test-support --lib --tests
    cargo run --locked -p dimensify --features robot --bin robosim -- --check
    cargo build --locked -p dimensify_session --features example-browser --examples

# Check the exact feature sets used for each browser example.
ci-wasm:
    cargo clippy --locked -p dimensify_session --features example-browser --example browser_collaboration --target wasm32-unknown-unknown --no-deps -- -D warnings
    cargo clippy --locked -p dimensify --features playground --example camera_playground --target wasm32-unknown-unknown --no-deps -- -D warnings
    cargo clippy --locked -p dimensify --features splat --example splat_viewer --target wasm32-unknown-unknown --no-deps -- -D warnings

# Build all live examples and stage a clean, complete site (output must be new).
showcase-release output="dist/showcase":
    python3 scripts/showcase.py --profile release --output {{quote(output)}}
