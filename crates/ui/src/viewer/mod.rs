//! Optional viewer styling and motion. Enable the `viewer` feature to use it.
//!
//! Add [`ViewerUiPlugin`] after Bevy's default plugins. Spawn panels with a
//! [`PanelMotion`] and a [`Node`], then call [`PanelMotion::set_open`] to retarget
//! them. Motion owns panel display and transform translation. Disclosures can
//! use [`Reveal::update_node`] with their own domain-specific open state.
mod grid;
mod motion;
pub use grid::{GridLabel, ViewerGrid};
mod style;

pub use motion::{PanelMotion, Reveal};
pub use style::{button, header, panel, panel_node, status_node, text, toolbar};

use bevy::{
    feathers::{FeathersCorePlugin, dark_theme::create_dark_theme, theme::UiTheme},
    input_focus::{
        InputFocus,
        tab_navigation::{TabIndex, TabNavigationPlugin},
    },
    picking::Pickable,
    prelude::*,
    ui::{InteractionDisabled, UiSystems},
};

/// Installs Feathers, a dark theme when none exists, and retained panel motion.
/// Existing Feathers plugins and application themes are preserved.
pub struct ViewerUiPlugin;

impl Plugin for ViewerUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<UiTheme>() {
            app.insert_resource(UiTheme(create_dark_theme()));
        }
        if !app.is_plugin_added::<TabNavigationPlugin>() {
            app.add_plugins(TabNavigationPlugin);
        }
        if !app.is_plugin_added::<FeathersCorePlugin>() {
            app.add_plugins(FeathersCorePlugin);
        }
        app.add_plugins(grid::ViewerGridPlugin);
        app.init_resource::<Motion>()
            .add_systems(First, refresh_motion)
            .add_systems(
                PostUpdate,
                (place_panels, animate_panels, block_panel_input)
                    .chain()
                    .in_set(ViewerUiSystems)
                    .before(UiSystems::Layout),
            );
    }
}

/// Mark the wrapping toolbar root so panels clear its measured bounds.
#[derive(Component)]
pub struct Toolbar;

/// Mark the bottom status root, positioned by [`status_node`].
#[derive(Component)]
pub struct StatusCard;

/// Opt into toolbar/status-aware placement; required automatically by PanelMotion.
#[derive(Component, Default)]
pub struct ResponsivePanel;

fn place_panels(
    bars: Query<(&ComputedNode, &UiGlobalTransform), With<Toolbar>>,
    cards: Query<&ComputedNode, With<StatusCard>>,
    mut panels: Query<&mut Node, With<ResponsivePanel>>,
) {
    let top = bars
        .iter()
        .filter(|(node, _)| !node.is_empty())
        .map(|(node, transform)| {
            (transform.translation.y + node.size().y * 0.5) * node.inverse_scale_factor() + 8.
        })
        .fold(104_f32, f32::max);
    let bottom = cards
        .iter()
        .filter(|node| !node.is_empty())
        .map(|node| node.size().y * node.inverse_scale_factor() + 24.)
        .fold(120_f32, f32::max);
    for mut panel in &mut panels {
        if panel.top != px(top) {
            panel.top = px(top);
        }
        if panel.bottom != px(bottom) {
            panel.bottom = px(bottom);
        }
    }
}

/// Panel synchronization runs after application Update systems, before layout.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewerUiSystems;

/// Shared motion preference; native applications may set `reduced` directly.
/// Browsers refresh this value from the live OS media query each frame.
#[derive(Resource)]
pub struct Motion {
    pub reduced: bool,
}
impl Default for Motion {
    fn default() -> Self {
        #[cfg(target_arch = "wasm32")]
        let reduced = browser_reduced_motion();
        #[cfg(not(target_arch = "wasm32"))]
        let reduced = false;
        Self { reduced }
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_reduced_motion() -> bool {
    // Keep the live MediaQueryList; its matches property follows OS changes.
    // Browser values stay on the browser thread rather than in a Send resource.
    thread_local! {
        static QUERY: Option<wasm_bindgen::JsValue> = {
            use wasm_bindgen::{JsCast, JsValue};
            web_sys::window()
                .and_then(|window| {
                    let function = js_sys::Reflect::get(&window, &"matchMedia".into())
                        .ok()?
                        .dyn_into::<js_sys::Function>()
                        .ok()?;
                    function
                        .call1(
                            &window,
                            &JsValue::from_str("(prefers-reduced-motion: reduce)"),
                        )
                        .ok()
                })
        };
    }
    QUERY.with(|query| {
        query
            .as_ref()
            .and_then(|query| {
                js_sys::Reflect::get(query, &"matches".into())
                    .ok()?
                    .as_bool()
            })
            .unwrap_or(false)
    })
}

fn refresh_motion(mut motion: ResMut<Motion>) {
    #[cfg(target_arch = "wasm32")]
    {
        let reduced = browser_reduced_motion();
        if motion.reduced != reduced {
            motion.reduced = reduced;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &mut motion;
}

impl Reveal {
    /// Apply a clipped grid-row reveal. Settled nodes are left unchanged.
    /// Returns whether layout changed, so ECS callers can preserve change detection.
    /// The single child should have zero min-height and clipped overflow.
    pub fn update_node(
        &mut self,
        node: &mut Node,
        open: bool,
        delta: f32,
        immediate: bool,
    ) -> bool {
        let previous = self.fraction();
        let fraction = self.advance(open, delta, immediate);
        let display = if fraction == 0. {
            Display::None
        } else {
            Display::Grid
        };
        if previous == fraction && node.display == display {
            return false;
        }
        if node.display != display {
            node.display = display;
        }
        let rows = vec![GridTrack::flex(fraction)];
        if node.grid_template_rows != rows {
            node.grid_template_rows = rows;
        }
        true
    }
}

fn animate_panels(
    time: Res<Time<Real>>,
    motion: Res<Motion>,
    mut panels: Query<(&mut PanelMotion, &mut Node, &mut UiTransform)>,
) {
    for (mut panel, mut node, mut transform) in &mut panels {
        let fraction = panel.advance(time.delta_secs(), motion.reduced);
        let display = if fraction == 0. && !panel.is_open() {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
        let translation = Val2::px(24. * (1. - fraction), 0.);
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
}

// Preserve application-owned disabled state and focus/picking configuration.
#[derive(Component)]
struct BlockedInput {
    disabled: bool,
    tab: Option<TabIndex>,
    pickable: Option<Pickable>,
}

fn block_panel_input(
    mut commands: Commands,
    panels: Query<(Entity, &PanelMotion)>,
    children: Query<&Children>,
    states: Query<(
        Has<InteractionDisabled>,
        Option<&TabIndex>,
        Option<&Pickable>,
    )>,
    blocked: Query<(Entity, &BlockedInput)>,
    mut focus: ResMut<InputFocus>,
) {
    let mut inactive = std::collections::HashSet::new();
    for (entity, panel) in &panels {
        if !panel.is_open() {
            inactive.insert(entity);
            inactive.extend(children.iter_descendants(entity));
        }
    }
    if focus.get().is_some_and(|entity| inactive.contains(&entity)) {
        focus.clear();
    }
    for &entity in &inactive {
        if blocked.get(entity).is_ok() {
            continue;
        }
        let Ok((disabled, tab, pickable)) = states.get(entity) else {
            continue;
        };
        commands.entity(entity).insert((
            BlockedInput {
                disabled,
                tab: tab.copied(),
                pickable: pickable.copied(),
            },
            InteractionDisabled,
            Pickable::IGNORE,
        ));
        if tab.is_some() {
            commands.entity(entity).insert(TabIndex(-1));
        }
    }
    for (entity, saved) in &blocked {
        if inactive.contains(&entity) {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.remove::<BlockedInput>();
        if !saved.disabled {
            entity.remove::<InteractionDisabled>();
        }
        if let Some(tab) = saved.tab {
            entity.insert(tab);
        }
        if let Some(pickable) = saved.pickable {
            entity.insert(pickable);
        } else {
            entity.remove::<Pickable>();
        }
    }
}

#[cfg(test)]
mod tests;
