# Gaussian visual testing

Visual assertions check rendered output alongside the CPU and completed-frame
benchmarks. Image correctness and performance have separate verdicts.

## Run locally

```sh
mise exec -- npm ci --ignore-scripts
# 21 existing LOD/packed/turn raster cases plus one resident-buffer case.
mise exec -- just visual-gpu-tests /tmp/splat-gpu-check
# Five small appearance cases, using the production Gaussian renderer.
mise exec -- just visual-native /tmp/splat-images
mise exec -- just visual-compare --expectations tests/visual/expectations.json \
  --actual /tmp/splat-images --out /tmp/splat-image-results
```

Output directories must be new. The GPU suite runs serially and asserts its
selected inventory and executed count. External real-scene bounds reproductions
need `SOG_REPLAY_DIR` and are excluded from the portable suite.

The appearance cases check anisotropic extent, alpha/depth order after camera
reversal, and view-dependent SH color. Fixed cameras/time/settings render into
256x256 offscreen targets. Capture requires actual sort/draw acknowledgements and
completed GPU readback. Region/color assertions run independently of references.
Existing integration tests cover LOD replacement, frustum/near-plane support,
residency, compaction, projection, compositing, camera isolation and teardown.

## Browser rendering

```sh
mise exec -- just visual-web-build /tmp/splat-visual-web
mise exec -- python3 -m http.server 8765 --bind 127.0.0.1 \
  --directory /tmp/splat-visual-web
```

In another terminal:

```sh
# On macOS/Windows, also pass --profile YOUR_DEVICE_DRIVER_PROFILE.
mise exec -- just visual-browser --url http://127.0.0.1:8765/ \
  --out /tmp/splat-browser-images
mise exec -- just visual-compare --expectations tests/visual/expectations.json \
  --actual /tmp/splat-browser-images --out /tmp/splat-browser-results
```

Local browser captures on macOS/Windows require a stable `--profile` such as
`--profile apple-m5-max-macos26-7`. Include device/GPU and driver or OS version;
change it when the hardware/driver changes. Browser privacy can hide the active
adapter, so the declared profile separates capture identities without claiming
that supplemental Chromium GPU inventory identifies the active WebGPU device.
Linux uses the forced SwiftShader software profile; remote mobile captures use
the explicitly supplied physical-device ID.

The browser runs the same production renderer/testbed as native. PNGs contain
Bevy screenshot readback bytes, excluding page UI and browser canvas composition.
The testbed is separate from the gallery's package/navigation tests. It does not
replace viewer-loading or interactive collaboration acceptance tests.

## Reference storage

Reference images live in Pixel Eagle, following Bevy's artifact-to-service
approach. Git contains scene definitions and analytic region/color expectations;
rendered PNGs and numeric assertion results are temporary CI artifacts with
seven-day retention. Do not commit captures, references or calibration images.

Analytic assertions catch blank output, missing clouds, wrong overlap order and
incorrect SH color without credentials or reference images. Pixel Eagle handles
image comparisons and visual review separately. A passed analytic check does not
claim that the service comparison passed.

## CI and Pixel Eagle

The `Visual correctness` workflow runs software Vulkan lifecycle/image tests and
WASM WebGPU on PRs. Main, weekly and manual runs also capture Metal and DX12;
maintainers can request them on a PR using the `visual-platforms` label.
The browser job installs Mesa Vulkan and uses Chromium under an Xvfb virtual
display. It runs unattended without a physical display or GPU. Software adapters
provide correctness evidence; their timings do not enter GPU
performance history. Separate cache lanes and read-only permissions are preserved.

Pixel Eagle stores the image references and produces comparison reports. Configure:

| Setting | Value |
| --- | --- |
| Secret `PIXEL_EAGLE_TOKEN` | Project-scoped token from project settings |
| Variable `PIXEL_EAGLE_PROJECT` | Project UUID from its URL |
| Variable `PIXEL_EAGLE_ENABLED` | `true` to enable reporting |
| Variable `PIXEL_EAGLE_BASELINES` | JSON mapping capture artifact names to reviewed integer service run IDs |

After the workflows are on the default branch, manually dispatch `Visual diff
report` with a completed source run ID and `upload_only=true` to upload candidate
references. Upload-only dispatch works while automatic reporting is disabled.
Inspect the linked runs, configure their fixed IDs, then set
`PIXEL_EAGLE_ENABLED=true`. Bootstrap
uploads explicitly report `passed:false` because no comparison occurred. Leave
project automatic baseline updates disabled. The reporter never approves images
or selects the latest main run automatically.

For CLI bootstrap after the reporting workflow reaches the default branch:

```sh
gh workflow run visual-report.yml -f source_run=SOURCE_RUN_ID -f upload_only=true
```

The reporter includes Linux native/browser captures and Metal/DX12 when those
platform artifacts are present. The workflow summary links the service runs, and
each `pixeleagle-*` artifact
contains `publication.json` with its numeric `runId`. After reviewing the images,
set `PIXEL_EAGLE_BASELINES` to their IDs and enable automatic reports:

```sh
gh variable set PIXEL_EAGLE_BASELINES --body '{"visual-native-linux":123,"visual-browser-linux":456}'
gh variable set PIXEL_EAGLE_ENABLED --body true
```

Replace the example IDs with the reviewed service run IDs. Manually dispatch
again with `upload_only=false` to verify a real comparison before relying on
future reports. Analytic assertions remain independent of service comparisons.

Automatic reporting also accepts failed source runs with complete captures, so
regressions can be reviewed. A service verdict never overrides failed local
assertions. Reporting executes default-branch code, validates source workflow/run/
commit and bounded image artifacts, and keeps trusted provenance separate from
PR-supplied rendering metadata. Fork tests need no service credentials.

Provider setup: [project tokens](https://pixel-eagle.com/docs/authentication/),
[comparison API](https://pixel-eagle.com/docs/comparison/) and
[current quotas](https://pixel-eagle.com/pricing/).

## Physical phones and tablets

Use a real remote browser connection and a device description:

```json
{"kind":"physical-mobile","id":"phone-01","model":"Pixel 9","os":"Android 16","provider":"local-usb"}
```

```sh
# Set DIMENSIFY_CDP_ENDPOINT or BROWSERSTACK_WS_ENDPOINT in your environment.
mise exec -- node scripts/visual-mobile.cjs --target device.json \
  --url https://YOUR_HOST/visual-testbed/ --out /tmp/phone-images
```

The manual `Physical mobile visual capture` workflow accepts the same description
and a publicly reachable testbed URL. Configure one of `VISUAL_MOBILE_CDP_ENDPOINT`
or `BROWSERSTACK_WS_ENDPOINT` as a secret. Android USB debugging with port
forwarding can support local CDP; a hosted runner needs a reachable remote browser.
iOS requires a provider exposing a compatible connection and WebGPU device.
Provider availability must be verified; the runner does not implement Appium or
an iOS Safari driver. Evaluate the [BrowserStack OSS program](https://www.browserstack.com/open-source)
before relying on it in required CI.

Device identity is operator-declared and cross-checked against observed OS,
screen and DPR. Treat the device ID as a stable device/OS/driver profile; use a
new ID after hardware, OS or driver changes before reviewing any new reference. It is not hardware attestation. Images use a fixed internal
resolution for correctness; shipping DPR, gestures, thermal behavior and sustained
frame performance remain separate [mobile benchmark work](../docs-dev/splat-performance-todo.md).
