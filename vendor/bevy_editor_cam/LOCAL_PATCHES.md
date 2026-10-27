# Maintained camera motion dependency

This source retains the upstream MIT and Apache-2.0 licenses and `.gitrepo`
provenance for the Bevy 0.19 fork. Preserve the existing motion and extension
behavior; record focused generic patches here.

- Bevy 0.19 camera/projection and buffered-message APIs are supported by the
  maintained source. The existing dolly-zoom anchor regression remains included.
- `bevy_render` and `bevy_image` are enabled by the independent-skybox extension;
  `bevy_color` is enabled by the anchor-indicator extension. Disabling default
  extensions permits camera motion without a renderer. Full default extensions
  still select those dependencies.

`dimensify_camera/inspect` consumes the motion API. It does not install the stock
input or camera-writing plugins; consumers retain their single pose writer.
