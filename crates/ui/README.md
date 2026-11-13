# Dimensify UI

The default package provides camera ownership markers without rendering or UI
dependencies. Enable `viewer` for shared Feathers styling and motion:

```toml
dimensify_ui = { path = "../ui", features = ["viewer"] }
```

Add `dimensify_ui::viewer::ViewerUiPlugin` after Bevy's default plugins. It installs
missing Feathers plugins, keeps an existing `UiTheme` or supplies the dark theme,
and initializes the shared `Motion` preference.

```rust,ignore
use bevy::prelude::*;
use dimensify_ui::viewer::{self, PanelMotion};

commands.spawn((
    viewer::panel_node(420.),
    viewer::panel(),
    ScrollPosition::default(),
    PanelMotion::new(false),
));
// In an application Update system:
panel_motion.set_open(true);
```

`PanelMotion::new(open)` starts settled; `set_open(open)` retargets from the
current position, and `is_open()` reports the target. The plugin owns panel
`Node.display` (Flex/None) and `UiTransform.translation`, with a 24 px slide over
200 ms. It synchronizes these in PostUpdate, in `ViewerUiSystems`, before layout.
Mark toolbar and status roots with `Toolbar` and `StatusCard` respectively, using
`toolbar()` and `status_node()` for their layouts. `PanelMotion` automatically
adds `ResponsivePanel`: its top/bottom insets grow to clear the measured wrapping
toolbar and status card, with DPI conversion and an 8 px gap. The 104/120 px defaults
remain minimum insets. Resize changes settle on the next layout update.

Closing and hidden panel descendants lose pointer interaction and tab stops;
focus is cleared. Reopening restores their previous picking/tab configuration
and preserves controls that were already disabled. Applications retain ownership
of domain-specific control enablement while panels are open.

The public `viewer` helpers are:

- `text(&AssetServer, impl Into<String>, f32) -> impl Bundle`: regular Feathers font
  and themed text color.
- `panel() -> impl Bundle`, `header() -> impl Bundle`: themed background/border.
- `toolbar() -> Node`: wrapping top toolbar with 16 px outer insets.
- `button() -> Node`: centered Feathers toolbar button layout, 108 px minimum width
  and 44 px height. Insert alongside a `FeathersButton` scene.
- `status_node() -> Node`: bottom status layout, inset 16 px.
- `panel_node(width: f32) -> Node`: scrollable right-side panel, top 104 px,
  bottom 120 px and right 16 px. Add `ScrollPosition` separately.

Disclosures share the same reversible easing through `Reveal::new(open)`,
`advance(open, delta_seconds, immediate) -> f32`, and `fraction() -> f32`.
`update_node(&mut Node, open, delta_seconds, immediate) -> bool` applies a single
fractional grid row and returns whether layout changed. Its child must have zero
minimum height and clipped overflow. To preserve ECS change detection, pass
`node.bypass_change_detection()` and call `node.set_changed()` only when it returns
true. Settled reveals do not rebuild grid tracks.

`Motion { reduced: bool }` disables transitions immediately, including mid-flight
and with zero elapsed time. Native applications set this preference directly;
browsers refresh it from the live `prefers-reduced-motion` media query.

The viewer plugin also provides a shared infinite XZ grid at Y=0. It starts
hidden. Toggle `ViewerGrid.enabled` and mark the toolbar text with `GridLabel`;
the plugin synchronizes visibility, text and the button accessibility label.
The grid does not alter camera poses or scene bounds. A camera exactly on Y=0
cannot see the plane; move vertically to reveal it.
