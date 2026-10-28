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
mise exec -- python3 scripts/visual-check.py \
  --actual /tmp/splat-images --out /tmp/splat-image-results --require-golden
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
mise exec -- python3 scripts/visual-check.py \
  --actual /tmp/splat-browser-images --out /tmp/splat-browser-results --require-golden
```

Local browser captures on macOS/Windows require a stable `--profile` such as
`--profile apple-m5-max-macos26-7`. Include device/GPU and driver or OS version;
change it when the hardware/driver changes. Browser privacy can hide the active
adapter, so the declared profile separates reference identities without claiming
that supplemental Chromium GPU inventory identifies the active WebGPU device.
Linux uses the forced SwiftShader software profile; remote mobile captures use
the explicitly supplied physical-device ID. Repeat calibration ignores only
Chromium's GPU-process `initializationTime` observation.

The browser runs the same production renderer/testbed as native. PNGs contain
Bevy screenshot readback bytes, excluding page UI and browser canvas composition.
The testbed is separate from the gallery's package/navigation tests. It does not
replace viewer-loading or interactive collaboration acceptance tests.

## Reviewed references

CI requires matching reviewed references. The baseline registry is
`tests/visual/baselines/index.json`. Metadata includes backend, adapter, driver,
browser, target, dimensions and scene/settings hashes. A changed identity fails
comparison and needs explicit review. Reference deletion must not disable the gate.
Expected, actual, diff PNGs and numeric error/region results accompany comparisons.

To prepare an intentional reference change, build from a clean committed tree
and capture twice into distinct directories on the intended lane. Inspect all
images and semantic results, then create a new reference directory explicitly:

```sh
mise exec -- just visual-baseline \
  --actual /tmp/images-a --repeat /tmp/images-b \
  --expectations tests/visual/expectations.json \
  --out tests/visual/baselines/new-reviewed-reference \
  --revision FULL_COMMIT_SHA --review 'Review evidence and reason for this change'
```

The creator requires matching clean build revisions, identical decoded repeat
images and successful semantic assertions. It records hashes and two-run
calibration with zero-error thresholds. It does not update the registry or a
service. Review the reference/registry diff together. Any relaxed tolerance needs
measured lane evidence and explicit review; do not widen it merely to make CI green.

!!! note
    A new platform initially fails for a missing reviewed reference while retaining
    its captures. Download both capture artifacts, inspect them, create/register
    the reference, and rerun. Explicit `--semantic-only` comparison is available
    for device exploration; it never claims a golden-image comparison passed.

## CI and Pixel Eagle

The `Visual correctness` workflow runs software Vulkan lifecycle/image tests and
WASM WebGPU on PRs. Main, weekly and manual runs also capture Metal and DX12;
maintainers can request them on a PR using the `visual-platforms` label.
Software adapters provide correctness evidence; their timings do not enter GPU
performance history. Separate cache lanes and read-only permissions are preserved.

Pixel Eagle is an optional review interface. Configure:

| Setting | Value |
| --- | --- |
| Secret `PIXEL_EAGLE_TOKEN` | Project-scoped token from project settings |
| Variable `PIXEL_EAGLE_PROJECT` | Project UUID from its URL |
| Variable `PIXEL_EAGLE_ENABLED` | `true` to enable reporting |
| Variable `PIXEL_EAGLE_BASELINES` | JSON mapping `visual-native-linux` and `visual-browser-linux` to reviewed integer service run IDs |

After the workflows are on the default branch, manually dispatch `Visual diff
report` with a completed source run ID and `upload_only=true` to upload candidate
references. Inspect the linked runs, then configure their fixed IDs. Bootstrap
uploads explicitly report `passed:false` because no comparison occurred. Leave
project automatic baseline updates disabled. The reporter never approves images
or selects the latest main run automatically.

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
screen and DPR. It is not hardware attestation. Images use a fixed internal
resolution for correctness; shipping DPR, gestures, thermal behavior and sustained
frame performance remain separate [mobile benchmark work](../docs-dev/splat-performance-todo.md).
