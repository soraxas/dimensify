# Maintained camera motion dependency

This source retains the upstream MIT and Apache-2.0 licenses and `.gitrepo`
provenance for the Bevy 0.19 fork. Preserve the existing motion and extension
behavior; record focused generic patches here.

- Active dependencies target Bevy 0.20.1. Cubic spline segments now come from
  `bevy_curve`; the renderer-free shared Inspect consumer, default renderer
  extensions and all upstream examples compile. Example dependencies use
  `bevy_framepace` 0.23 and the maintained `big_space` 0.20 port pinned in the
  manifest until its release.
  Projection construction uses the new right-handed, zero-to-one-depth math
  helpers. The existing dolly-zoom anchor regression remains included.
- `bevy_render` and `bevy_image` are enabled by the independent-skybox extension;
  `bevy_color` is enabled by the anchor-indicator extension. Disabling default
  extensions permits camera motion without a renderer. Full default extensions
  still select those dependencies.

`dimensify_camera/inspect` consumes the motion API. It does not install the stock
input or camera-writing plugins; consumers retain their single pose writer.
