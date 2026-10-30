# Build, checks and example hosting

## Local setup

Install [mise](https://mise.jdx.dev/), then from the repository root:

```sh
mise trust
mise install
mise exec -- just ci-format ci-scripts ci-workflows
mise exec -- just ci-headless
mise exec -- just ci-native
mise exec -- just ci-wasm
```

`mise.toml` and `mise.lock` pin Rust, Mr Boxington, wasm-bindgen, Just,
Python, Node and workflow linters. Rust must match `rust-toolchain.toml`;
wasm-bindgen must match `Cargo.lock`. Use `mise exec --` in noninteractive
shells so the Rust wrapper and matching CLI tools are selected. The showcase packager invokes
wasm-bindgen through mise explicitly, so a globally installed CLI cannot shadow
the pinned schema version in its child processes.
`just ci-scripts` and `just perf-scenes` install the pinned Python report validator
from `scripts/requirements-perf.txt` into the mise-selected Python environment.
For direct Python invocations, first run `mise exec -- just perf-python`.

Mr Boxington shares compiler results locally; `mise exec -- mbx stats`
shows its measured activity. Do not share writable target directories.

On Ubuntu, native viewers require these platform headers:

```sh
sudo apt-get install libasound2-dev libudev-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev libxkbcommon-dev libwayland-dev
```

## Build the website

```sh
mise exec -- just showcase-release
mise exec -- python3 -m http.server 8000 --directory dist/showcase
```

Open `http://localhost:8000/`. The root opens the gallery; the splat viewer
is at `viewer.html`. This builds all three live examples and the SOG worker
with the release profile, removing debugging names and producer metadata from
the published WASM. Development builds retain names for debugging.
Native/headless examples have local run commands.
The output directory must be new; choose another with
`mise exec -- just showcase-release dist/another-showcase` for a subsequent build.
To repackage existing builds without compiling, use
`mise exec -- python3 scripts/showcase.py --package-only --output dist/repacked`.

Browser targets are opted in through `package.metadata.showcase` in their
owning manifests. New examples are discovered automatically. Run
`mise exec -- just showcase-catalog` after metadata changes and commit the
updated catalog. Packaging requires every browser target's JS/WASM and any
SOG worker assets, validates WASM headers,
and copies only the catalog's assets. Build output stays ignored by Git.

!!! note "Collaboration hosting"
    GitHub Pages serves static files. The collaboration example needs a
    separately running [room server](collaboration.md). Enter its `wss://`
    endpoint in Connection when using the HTTPS website. No public authority
    or third-party endpoint is configured by this repository.

## Continuous integration

The [visual correctness workflow](../.github/workflows/visual-correctness.yml)
executes native GPU lifecycle/pixel tests and the shared WASM renderer testbed.
See [Gaussian visual testing](visual-testing.md) for reviewed references, local
commands, optional Pixel Eagle reports and physical-device capture setup.

CPU operation benchmarks run in a separate workflow with Bencher history and a
20% regression gate for same-repository PRs. See [performance tracking](performance.md)
for workloads, local commands, project setup and required-check activation.

[CI](../.github/workflows/ci.yml) runs on pull requests, pushes to `main`,
and manual dispatch. Documentation-only edits skip compilation; mixed changes,
assets, manifests, scripts, workflows and unknown files run the checks.

- **Quality:** first-party Rustfmt, gallery discovery/packaging tests, catalog
  freshness, JavaScript syntax, Actionlint and Zizmor. Vendored formatting is
  excluded so upstream formatting differences are not rewritten.
- **Headless:** strict Clippy, library and real networking tests, and the isolated
  source export/dependency boundary check. Rendering features stay disabled.
  `cargo fetch --locked` primes the export check's offline metadata resolution;
  downloading dependencies does not compile the rendering graph.
- **Native:** strict Clippy, viewer/streaming/camera/robot tests, RoboSim's
  headless validation and native collaboration example builds. These use the
  unoptimized `ci` profile, including dependencies, so verification does not
  pay for the interactive development profile's optimized Bevy builds.
- **Gallery:** strict WASM Clippy for the three browser targets, one release
  website build, then a headless Chromium check of filters, mobile layout,
  browser links and JS loader imports (including codec snippets) under a
  `/dimensify/` prefix. The exact tested
  directory becomes the Pages artifact on PRs and main.

The static gallery check does not establish GPU rendering or touch-device
compatibility. The existing `check-splat-browser.cjs` and
`check-showcase-browser.cjs` provide separate rendering/interaction acceptance
with a suitable WebGPU browser and local room server.

Native/headless lint, tests, RoboSim validation, example builds and export
verification appear as separate named CI steps. The local `ci-native` and
`ci-headless` recipes run those same steps in order.

## Cache policy

The SHA-pinned Mr Boxington action stores isolated compiler object bundles in
GitHub Actions cache. Headless, native and WASM jobs have separate generations;
the generated keys also include compiler identity. Native link caching is off.
Default-branch pushes and manual runs save successful results; same-repository
PRs save to their isolated PR scope, and forks remain restore-only. No extra
credential or cache server is needed. Cache misses build normally.

Object bundles do not contain Cargo registry/Git downloads. Cargo may download
dependencies again, and a fresh checkout may still be marked dirty even when
its compiler results are reusable. Inspect mbx hit/miss/bypass counts and compiler
activity; a restored archive alone does not establish compilation reuse.

## GitHub Pages

[Live examples](https://cs.tinyiu.com/dimensify/) are deployed by the final
CI job on `main` or manual dispatch of `main`, after all four checks succeed.
PRs upload a build artifact but cannot deploy. Only the deploy job receives
`pages: write` and `id-token: write`; checkouts do not persist Git credentials.
Actions use commit pins. Dependabot groups all routine action updates into one
weekly PR, including major versions, with at most one open version-update PR.
Security fixes use a separate group. Cargo updates are not enabled.

The repository's **Settings > Pages > Source** must be **GitHub Actions**.
For a new repository, an administrator can enable it once with:

```sh
gh api --method POST repos/soraxas/dimensify/pages -f build_type=workflow
```

The deploy job publishes the already tested artifact without rebuilding.
Changes to prose do not redeploy; use the CI manual dispatch when needed.
