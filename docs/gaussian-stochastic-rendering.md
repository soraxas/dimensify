# Experimental stochastic transparency

Dimensify renders with sorted alpha blending, and by default the viewer skips
sorting during camera motion when sorted frames are slow (see
[adaptive modes](#adaptive-modes)). The experimental stochastic mode samples each Gaussian fragment's alpha as coverage, writes surviving color
opaquely and uses reverse-Z depth to resolve overlapping splats. It skips GPU
radix dispatches. This is a rendering mode, not MCMC model training.

!!! warning "Quality trade-off"
    One fixed sample produces visible spatial grain. This version has no spatial
    denoiser, temporal accumulation or convergence guarantee. Camera motion and LOD remapping can change coverage. It is not a
    quality-equivalent replacement for sorted rendering.

Use the viewer's **Rendering** setting, native
`--transparency auto|movement|sorted|stochastic` argument, or browser
`?transparency=` query. The selection also applies to new files and pending
streamed generations. Switching between `sorted` and `stochastic` by hand can
briefly hide clouds while the new GPU pipeline compiles; checks compare settled
output after preparation.

## Adaptive modes

The viewer policy follows SuperSplat's `view.stochastic` setting:

| Policy | While a Gaussian camera moves | Once it has settled |
| --- | --- | --- |
| `sorted` | sorted | sorted |
| `stochastic` | stochastic | stochastic |
| `movement` | stochastic | sorted |
| `auto` (default) | `movement` when sorted frames are slow, otherwise sorted | sorted |

Motion is any change to a `GaussianCamera` global transform. The camera must stay
unchanged for `SETTLE_SECONDS` (150 ms) before sorting resumes, so frames
without input in the middle of a touch drag do not alternate modes. `auto` takes
the median of the last 30 sorted frame intervals, ignoring stochastic frames.
It engages above `AUTO_ENGAGE_MS` (25 ms, below 40 fps) and disengages when
that median falls back under the threshold. SuperSplat instead uses a 60 ms GPU
timestamp span, because it renders only on change. Dimensify uses whole-frame
intervals because timestamp queries exist only in instrumented builds and many
mobile browsers do not expose them.

Adaptive policies set `CloudSettings::transparency_warmup`, so the other
transparency pipeline compiles in advance and the first moving frame does not
wait for it. Paths without stochastic support, such as WebGL and CPU sorting,
stay sorted under adaptive policies. Applications install `apply_transparency`
after transform propagation. The optional `SplatTransparencyState` resource
reports the effective mode, motion and `auto` measurements.

Applications can set `CloudSettings::transparency` to
`GaussianTransparency::Stochastic`. Use a consistent mode for every cloud in a
view. The first implementation supports 3D color rendering on the buffer-storage
GPU radix path. Unsupported Gaussian/inspection/additive modes retain sorted
behavior. Ordinary MSAA does not provide independent stochastic samples.

`stochastic_seed` selects a reproducible noise pattern; `stochastic_sample`
changes it explicitly. The renderer combines the seed with cloud identity and
source-record index to decorrelate overlapping splats. Record-index changes
through LOD selection can therefore change noise, even when source data overlap.
Keep seeds fixed for comparisons. A fixed-sample image does not demonstrate
multi-sample convergence or stability during motion.

Opaque mesh depth is respected. General interleaving with translucent meshes or
mixing sorted and stochastic clouds in one view is not supported by this
experimental mode. Stochastic splats write to the view depth buffer and can
therefore affect depth-dependent postprocessing. Start with an isolated Gaussian
view, MSAA off and no transparency postprocessing.

## Verification and measurement

The GPU correctness inventory includes binary coverage, repeated static samples,
overlap depth and source-order distribution, restoration of sorted output, and
packed LOD handoff checks. These are semantic assertions; generated captures
belong in artifacts and reviewed references belong in Pixel Eagle.

The native packed LOD runner accepts `--transparency sorted|stochastic`.
Stochastic reports use `dimensify-native-lod-stochastic-offscreen-v1`, separate
from sorted history. They require acknowledged direct projected draws and zero
encoded sorts. Raw JSON validation enforces the protocol's transparency and
acknowledgement fields before aggregation. Earlier sorted-only reports retain
their implicit sorted mode during saved-report validation. Retained sort buffers remain allocated for mode switching, so
skipping sort does not imply lower allocated memory.

Compare the same fixture, camera, resolution, SH, LOD budgets and hardware.
Measure completed-frame and individual GPU pass timings separately; completion
waits are not display FPS. Stochastic radix timing is unavailable because the
radix pass does not run. Report noise and image-quality differences alongside
any timing comparison. No physical-mobile performance claim follows from these
headless checks.
