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
    @cd python && DIMENSIFY_TRANSPORT_MODE=udp \
      DIMENSIFY_TRANSPORT_CONNECTION=client \
      DIMENSIFY_TRANSPORT_ENDPOINT=controller \
      DIMENSIFY_TRANSPORT_SERVER_ADDR=127.0.0.1:6210 \
      uv run python examples/example_transport.py

@_transport-demo_rust:
    @# build viewer ahead of time so the controller doesn't race compilation
    cargo build --features transport_udp
    @# run viewer in the background
    @DIMENSIFY_VIEWER_MODE=3d \
      DIMENSIFY_TRANSPORT_MODE=udp \
      DIMENSIFY_TRANSPORT_CONNECTION=server \
      DIMENSIFY_TRANSPORT_ENDPOINT=viewer \
      DIMENSIFY_TRANSPORT_SERVER_ADDR=127.0.0.1:6210 \
      cargo run --features transport_udp

# ▶️ Run a transport demo (viewer + python controller)
[parallel]
transport-demo: _transport-demo_rust _transport-demo_py

# ▶️ Run Rust tests
tests:
    cargo test -p dimensify


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
