# Gallery and viewer UI

- New Rust viewer examples must reuse the shared UI in
  `dimensify_ui::viewer` rather than introduce separate viewer designs.
- Web gallery pages and link builders must reuse `gallery.css` and the URL
  helpers in `launch.mjs`. Keep the existing splat UI as the shared design.
- Show only startup options supported by the selected viewer. Point cloud
  startup currently accepts only `source` and `up=y|z`; it does not support
  camera parameters or toolbar hiding for embeds.
