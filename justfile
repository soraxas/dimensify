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

# Help command: Display all available recipes
help:
    just --list




# Desktop Development
desktop-dev: ## ▶️  Run desktop version in development
    cargo run

desktop-dev-watch: ## ▶️  Run desktop version in development - watch mode
    cargo watch -q -c -x 'run'

desktop-dev-watch-dyn: ## ▶️  Run desktop version in development - watch mode
    cargo watch -q -c -x 'run --features bevy/dynamic_linking'

# Build desktop version
desktop-build: ## ⚙️  Build desktop version
    cargo build --release
    rm -rf ./target/release/assets
    mkdir ./target/release/assets
    cp -r ./assets ./target/release

# USAGE:
# just setup-wasm-envar-then XXXXX
[no-cd, positional-arguments]
setup-wasm-envar-then +args:
  #!/bin/bash
  set -euo pipefail
  WASM_SERVER_RUNNER_ADDRESS=0.0.0.0:3000
  just "$@"

# WebAssembly Development
__wasm-dev: ## ▶️  Run wasm version in development mode via wasm-server-runner
    @echo "Once started, to access the page with the wasm-bindgen bindings, open http://127.0.0.1:3000/dev.html"
    @echo ""
    cargo run --target wasm32-unknown-unknown
wasm-dev:
    just setup-wasm-envar-then __wasm-dev
###########################

__wasm-dev-watch: ## ▶️  Run wasm version in development mode (watch mode)
    @echo "Once started, to access the page with the wasm-bindgen bindings, open http://127.0.0.1:3000/dev.html"
    @echo ""
    cargo watch -q -c -x 'run --target wasm32-unknown-unknown'
wasm-dev-watch:
    just setup-wasm-envar-then __wasm-dev-watch
###########################

__wasm-dev-release: ## ▶️  Run wasm version in development mode (no debug mode - lighter bundle)
    @echo "Once started, to access the page with the wasm-bindgen bindings, open http://127.0.0.1:3000/dev.html"
    @echo ""
    cargo run --release --target wasm32-unknown-unknown
wasm-dev-release:
    just setup-wasm-envar-then __wasm-dev-release
###########################

# Port forwarding
forward-fallback: ## ▶️  Forwards port 3000 to localhost.run (to access from mobile)
    ssh -R 80:localhost:3000 localhost.run

forward: ## ▶️  Forwards port 3000 to ngrok (to access from mobile on a secure origin)
    @command -v ngrok &> /dev/null && ngrok http 3000 || echo "{_BOLD}ngrok could not be found{_END} - infos to install it are available here: https://ngrok.com\nIf you don't wish to install it, you can use {_BOLD}just forward-fallback{_END}"

# WebAssembly Build
wasm-build: ## ⚙️  Build wasm version
    cargo build --release --target wasm32-unknown-unknown
    wasm-bindgen --out-dir ./www/public/out --target web ./target/wasm32-unknown-unknown/release/dimensify.wasm

wasm-opt: ## 🔩 Optimize wasm file size
    wasm-opt -Os -o ./www/public/out/dimensify.wasm ./www/public/out/dimensify.wasm

wasm-build-opt: ## ⚙️  Build wasm version with optimized file size
    just wasm-build
    just wasm-opt

# Website Build and Development
www-dev: ## ▶️  Build wasm and launch website dev server via vite
    just wasm-build
    just www-dev-only

www-dev-only: ## ▶️  Launch vite dev server (doesn't build wasm)
    cd www && npm run dev -- --host --port 3000

www-build: ## ⚙️  Build wasm and build website
    just wasm-build
    just www-build-only

www-build-only: ## ⚙️  Build vite bundle (doesn't build wasm)
    cd www && npm run build

www-build-opt: ## ⚙️  Build wasm (optimized wasm file size) and build website
    just wasm-build-opt
    just www-build-only

www-preview: ## ▶️  Preview website's build
    cd www && npm run preview -- --host --port 3000

# Mark recipes as .PHONY (not actual files)
