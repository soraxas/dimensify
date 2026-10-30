# Experimental stochastic transparency

Dimensify supports sorted alpha blending by default. The experimental stochastic
mode samples each Gaussian fragment's alpha as coverage, writes surviving color
opaquely and uses reverse-Z depth to resolve overlapping splats. It skips GPU
radix dispatches. This is a rendering mode, not MCMC model training.

!!! warning "Quality trade-off"
    One fixed sample produces visible spatial grain. This version has no spatial
    denoiser, temporal accumulation, motion-adaptive switching or convergence
    guarantee. Camera motion and LOD remapping can change coverage. It is not a
    quality-equivalent replacement for sorted rendering.

Use the viewer's **Render: Sorted / Render: Stochastic** button, native
`--transparency stochastic` argument, or browser `?transparency=stochastic` query.
The selection also applies to new files and pending streamed generations.
The first switch can briefly hide clouds while the new GPU pipeline compiles;
checks compare settled output after preparation.

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
