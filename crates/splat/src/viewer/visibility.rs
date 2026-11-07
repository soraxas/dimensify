//! Visibility field controls: hide, fade or display Gaussians that were never observed from about
//! the current viewing direction.
//!
//! A field is loaded with `--visibility <file.safetensors>` or found next to the opened PLY at
//! `<dir>/visibility/visibility.safetensors`. It must have been built for that PLY (same Gaussian
//! order); whole-file PLY loading keeps source order.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use bevy_gaussian_splatting::visibility::{
    GaussianVisibility, VisibilityCurve, VisibilityDisplay, VisibilityField, VisibilitySettings,
};

use crate::SplatScene;

/// Viewer-wide field state; settings are copied onto the loaded cloud every frame.
#[derive(Resource, Default)]
pub(super) struct ViewerVisibility {
    /// Explicit `--visibility` path.
    pub explicit: Option<PathBuf>,
    /// Field for the current cloud and the path it came from.
    pub field: Option<(PathBuf, Arc<VisibilityField>)>,
    pub settings: VisibilitySettings,
    /// Status line for the panel.
    pub status: String,
    /// Cloud entity the field was attached to.
    pub(super) attached: Option<Entity>,
    /// PLY path last checked for a sidecar field.
    pub(super) checked: Option<PathBuf>,
}

impl ViewerVisibility {
    pub fn active(&self) -> bool {
        self.attached.is_some()
    }

    /// Effective relative-evidence threshold (setting or the field's own).
    pub fn rho(&self) -> f32 {
        self.settings.rho.or(self.field.as_ref().map(|f| f.1.rho)).unwrap_or(0.3)
    }
}

/// Candidate field locations for a splat file.
pub(super) fn sidecar_paths(ply: &Path) -> Vec<PathBuf> {
    let dir = ply.parent().unwrap_or(Path::new("."));
    let stem = ply.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    vec![
        dir.join("visibility").join("visibility.safetensors"),
        dir.join(format!("{stem}.visibility.safetensors")),
    ]
}

fn load(path: &Path) -> Result<VisibilityField, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    VisibilityField::from_safetensors(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Load the field for the opened file (once per path) and attach it to the loaded cloud.
pub(super) fn attach(
    mut commands: Commands,
    mut state: ResMut<ViewerVisibility>,
    scene: Res<SplatScene>,
    files: Res<super::files::LocalFiles>,
    clouds: Query<(&bevy_gaussian_splatting::PlanarGaussian3dHandle, Option<&GaussianVisibility>)>,
    assets: Res<Assets<bevy_gaussian_splatting::PlanarGaussian3d>>,
) {
    let current = files.current_path();
    if current != state.checked {
        state.checked = current.clone();
        state.attached = None;
        let candidates = match (&state.explicit, &current) {
            (Some(p), _) => vec![p.clone()],
            (None, Some(ply)) => sidecar_paths(ply).into_iter().filter(|p| p.exists()).collect(),
            _ => vec![],
        };
        state.field = None;
        state.status = "no visibility field (pass --visibility <file>)".into();
        if let Some(path) = candidates.into_iter().next() {
            match load(&path) {
                Ok(field) => {
                    state.status = format!(
                        "{}: {} Gaussians, degree {}, {} views",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        field.count,
                        field.degree,
                        field.n_views.map_or("?".into(), |n| n.to_string())
                    );
                    info!("visibility field {}", state.status);
                    state.field = Some((path, Arc::new(field)));
                }
                Err(e) => {
                    warn!("visibility field: {e}");
                    state.status = format!("could not load field: {e}");
                }
            }
        }
    }
    let Some(loaded) = scene.loaded.as_ref() else {
        return;
    };
    let Ok((handle, existing)) = clouds.get(loaded.entity) else {
        return;
    };
    let Some((_, field)) = state.field.clone() else {
        return;
    };
    if state.attached != Some(loaded.entity) {
        let Some(cloud) = assets.get(&handle.0) else {
            return;
        };
        let len = cloud.spherical_harmonic.len();
        // Loaders pad clouds to a multiple of 32 Gaussians.
        if field.count > len || len - field.count >= 32 {
            state.status = format!("field has {} Gaussians, cloud has {len}: not attached", field.count);
            warn!("visibility {}", state.status);
            state.field = None;
            return;
        }
        commands.entity(loaded.entity).insert(GaussianVisibility { field, settings: state.settings });
        state.attached = Some(loaded.entity);
        return;
    }
    if existing.is_some_and(|v| v.settings != state.settings) {
        commands.entity(loaded.entity).insert(GaussianVisibility { field, settings: state.settings });
    }
}

/// Keyboard: V display mode, C curve, [ ] threshold (lo and hi together), - = ramp width,
/// , . rho, G gamma x2 / Shift+G gamma /2.
pub(super) fn keys(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<ViewerVisibility>) {
    if !state.active() {
        return;
    }
    let s = &mut state.settings;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if keys.just_pressed(KeyCode::KeyV) {
        s.display = s.display.next();
    }
    if keys.just_pressed(KeyCode::KeyC) {
        let i = VisibilityCurve::ALL.iter().position(|c| *c == s.curve).unwrap_or(0);
        s.curve = VisibilityCurve::ALL[(i + 1) % VisibilityCurve::ALL.len()];
    }
    let mid = 0.5 * (s.lo + s.hi);
    let half = 0.5 * (s.hi - s.lo);
    let (mut mid2, mut half2) = (mid, half);
    if keys.just_pressed(KeyCode::BracketLeft) {
        mid2 -= 0.05;
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        mid2 += 0.05;
    }
    if keys.just_pressed(KeyCode::Minus) {
        half2 -= 0.05;
    }
    if keys.just_pressed(KeyCode::Equal) {
        half2 += 0.05;
    }
    if (mid2, half2) != (mid, half) {
        let half2 = half2.clamp(0.0, 0.5);
        let mid2 = mid2.clamp(0.0, 1.0);
        s.lo = (mid2 - half2).clamp(0.0, 1.0);
        s.hi = (mid2 + half2).clamp(0.0, 1.0);
    }
    if keys.just_pressed(KeyCode::KeyG) {
        s.gamma = if shift { (s.gamma * 0.5).max(0.125) } else { (s.gamma * 2.0).min(8.0) };
    }
    let rho = state.rho();
    let s = &mut state.settings;
    if keys.just_pressed(KeyCode::Comma) {
        s.rho = Some((rho - 0.05).max(0.05));
    }
    if keys.just_pressed(KeyCode::Period) {
        s.rho = Some((rho + 0.05).min(1.0));
    }
}

pub(super) fn summary(state: &ViewerVisibility) -> String {
    let s = &state.settings;
    format!(
        "display {}  |  curve {}  |  fade {:.2}..{:.2}  gamma {:.2}  rho {:.2}  gain {:.2}",
        s.display.label(),
        s.curve.label(),
        s.lo,
        s.hi,
        s.gamma,
        state.rho(),
        s.gain
    )
}

// ------------------------------------------------------------------------------------- panel

#[derive(Component)]
pub(super) struct Panel;
#[derive(Component)]
pub(super) struct StatusText;
#[derive(Component, Clone, Copy, PartialEq)]
pub(super) enum Choice {
    Display(VisibilityDisplay),
    Curve(VisibilityCurve),
}
#[derive(Component, Clone, Copy, PartialEq, Debug)]
pub(super) enum Knob {
    Lo,
    Hi,
    Gamma,
    Rho,
    Gain,
    Overlay,
}

impl Knob {
    const ALL: [Self; 6] = [Self::Lo, Self::Hi, Self::Gamma, Self::Rho, Self::Gain, Self::Overlay];

    fn label(self) -> &'static str {
        match self {
            Self::Lo => "fade start (vis below: removed)",
            Self::Hi => "fade end (vis above: kept)",
            Self::Gamma => "curve exponent",
            Self::Rho => "rho (share of best direction for full vis)",
            Self::Gain => "evidence gain",
            Self::Overlay => "overlay strength",
        }
    }

    fn range(self) -> (f32, f32) {
        match self {
            Self::Lo | Self::Hi | Self::Overlay => (0.0, 1.0),
            Self::Gamma => (0.125, 4.0),
            Self::Rho => (0.05, 1.0),
            Self::Gain => (0.05, 5.0),
        }
    }

    fn get(self, state: &ViewerVisibility) -> f32 {
        let s = &state.settings;
        match self {
            Self::Lo => s.lo,
            Self::Hi => s.hi,
            Self::Gamma => s.gamma,
            Self::Rho => state.rho(),
            Self::Gain => s.gain,
            Self::Overlay => s.overlay,
        }
    }

    fn set(self, s: &mut VisibilitySettings, v: f32) {
        match self {
            Self::Lo => {
                s.lo = v;
                s.hi = s.hi.max(v);
            }
            Self::Hi => {
                s.hi = v;
                s.lo = s.lo.min(v);
            }
            Self::Gamma => s.gamma = v,
            Self::Rho => s.rho = Some(v),
            Self::Gain => s.gain = v,
            Self::Overlay => s.overlay = v,
        }
    }
}

fn choice_button(parent: &mut ChildSpawnerCommands, choice: Choice, label: &str) {
    use bevy::feathers::controls::FeathersButton;
    use bevy::scene::prelude::*;
    let entity = parent.target_entity();
    parent
        .commands_mut()
        .spawn_scene(bsn! {
            @FeathersButton
            Node { min_height: px(32), padding: UiRect::horizontal(px(8)) }
        })
        .insert((choice, ChildOf(entity)))
        .observe(on_choice)
        .with_children(|p| {
            p.spawn((Text::new(label), bevy::feathers::theme::ThemedText));
        });
}

fn on_choice(
    event: On<bevy::ui_widgets::Activate>,
    choices: Query<&Choice>,
    mut state: ResMut<ViewerVisibility>,
) {
    match choices.get(event.entity) {
        Ok(Choice::Display(d)) => state.settings.display = *d,
        Ok(Choice::Curve(c)) => state.settings.curve = *c,
        Err(_) => {}
    }
}

fn on_slider(
    event: On<bevy::ui_widgets::ValueChange<f32>>,
    knobs: Query<&Knob>,
    mut commands: Commands,
    mut state: ResMut<ViewerVisibility>,
) {
    commands.entity(event.source).insert(bevy::ui_widgets::SliderValue(event.value));
    if let Ok(knob) = knobs.get(event.source) {
        knob.set(&mut state.settings, event.value);
    }
}

#[allow(deprecated)]
pub(super) fn setup(mut commands: Commands, assets: Res<AssetServer>, state: Res<ViewerVisibility>) {
    use bevy::feathers::controls::{FeathersSliderProps, slider_bundle};
    use bevy::ui_widgets::{SliderPrecision, SliderStep};
    commands
        .spawn((
            Panel,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(76),
                width: px(340),
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(4)),
                row_gap: px(6),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            GlobalZIndex(19),
            super::style::panel(),
        ))
        .with_children(|panel| {
            panel.spawn(super::style::text(&assets, "Visibility field", 18.));
            panel.spawn((StatusText, super::style::text(&assets, "", 12.)));
            panel.spawn(super::style::text(&assets, "display  [V]", 13.));
            panel.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), ..default() }).with_children(|row| {
                for d in VisibilityDisplay::ALL {
                    choice_button(row, Choice::Display(d), d.label());
                }
            });
            panel.spawn(super::style::text(&assets, "curve  [C]", 13.));
            panel.spawn(Node { column_gap: px(4), ..default() }).with_children(|row| {
                for c in VisibilityCurve::ALL {
                    choice_button(row, Choice::Curve(c), c.label());
                }
            });
            for knob in Knob::ALL {
                panel.spawn(super::style::text(&assets, knob.label(), 12.));
                let (min, max) = knob.range();
                panel
                    .spawn(slider_bundle(
                        FeathersSliderProps { value: knob.get(&state), min, max },
                        (knob, SliderStep(0.01), SliderPrecision(2)),
                    ))
                    .observe(on_slider);
            }
        });
}

/// Show the panel while a field is attached; mirror keyboard edits into the sliders.
pub(super) fn panel(
    mut commands: Commands,
    state: Res<ViewerVisibility>,
    mut panels: Query<&mut Node, With<Panel>>,
    mut status: Query<&mut Text, With<StatusText>>,
    sliders: Query<(Entity, &Knob, &bevy::ui_widgets::SliderValue)>,
    mut buttons: Query<(&Choice, &mut BackgroundColor)>,
) {
    for mut node in &mut panels {
        let display = if state.field.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    if !state.is_changed() {
        return;
    }
    for mut text in &mut status {
        text.0 = format!("{}\n{}", state.status, summary(&state));
    }
    for (entity, knob, value) in &sliders {
        let v = knob.get(&state);
        if (value.0 - v).abs() > 1e-4 {
            commands.entity(entity).insert(bevy::ui_widgets::SliderValue(v));
        }
    }
    for (choice, mut bg) in &mut buttons {
        let selected = match choice {
            Choice::Display(d) => *d == state.settings.display,
            Choice::Curve(c) => *c == state.settings.curve,
        };
        let colour = if selected { Color::srgba(0.25, 0.45, 0.9, 0.9) } else { Color::NONE };
        if bg.0 != colour {
            bg.0 = colour;
        }
    }
}
