//! Independent settings disclosures with reversible, clipped height transitions.
use bevy::prelude::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Section {
    Coordinates,
    Rendering,
    Lod,
    Memory,
    Advanced,
    Diagnostics,
}
impl Section {
    pub const ALL: [Self; 6] = [
        Self::Coordinates,
        Self::Rendering,
        Self::Lod,
        Self::Memory,
        Self::Advanced,
        Self::Diagnostics,
    ];
    pub fn title(self) -> &'static str {
        match self {
            Self::Coordinates => "Coordinates",
            Self::Rendering => "Rendering",
            Self::Lod => "Level of detail (LOD)",
            Self::Memory => "Memory and loading",
            Self::Advanced => "Advanced rendering",
            Self::Diagnostics => "Diagnostics",
        }
    }
}
#[derive(Clone)]
pub(super) struct Sections(u8);
impl Default for Sections {
    fn default() -> Self {
        Self((1 << Section::Rendering as u8) | (1 << Section::Lod as u8))
    }
}
impl Sections {
    pub fn is_open(&self, section: Section) -> bool {
        self.0 & (1 << section as u8) != 0
    }
    pub fn toggle(&mut self, section: Section) {
        self.0 ^= 1 << section as u8;
    }
    pub fn label(&self, section: Section) -> String {
        format!(
            "{} {}",
            if self.is_open(section) { "[-]" } else { "[+]" },
            section.title()
        )
    }
}

#[derive(Component)]
pub(super) struct Body(pub Section);

#[derive(Component)]
pub(super) struct Reveal(f32);
impl Reveal {
    pub fn new(open: bool) -> Self {
        Self(if open { 1. } else { 0. })
    }
    pub fn advance(&mut self, open: bool, delta: f32, immediate: bool) -> f32 {
        let target = if open { 1. } else { 0. };
        if immediate {
            self.0 = target;
        } else {
            let distance = delta.max(0.) / 0.2;
            self.0 += (target - self.0).clamp(-distance, distance);
        }
        self.0 * self.0 * (3. - 2. * self.0)
    }
}

#[derive(Resource)]
pub(super) struct Motion {
    pub reduced: bool,
}
impl Default for Motion {
    fn default() -> Self {
        #[cfg(target_arch = "wasm32")]
        let reduced = {
            use wasm_bindgen::{JsCast, JsValue};
            web_sys::window()
                .and_then(|window| {
                    let function = js_sys::Reflect::get(&window, &"matchMedia".into())
                        .ok()?
                        .dyn_into::<js_sys::Function>()
                        .ok()?;
                    let query = function
                        .call1(
                            &window,
                            &JsValue::from_str("(prefers-reduced-motion: reduce)"),
                        )
                        .ok()?;
                    js_sys::Reflect::get(&query, &"matches".into())
                        .ok()?
                        .as_bool()
                })
                .unwrap_or(false)
        };
        #[cfg(not(target_arch = "wasm32"))]
        let reduced = false;
        Self { reduced }
    }
}

pub(super) fn animate(
    time: Res<Time<Real>>,
    motion: Res<Motion>,
    settings: Res<super::settings::ViewerSettings>,
    mut bodies: Query<(&Body, &mut Reveal, &mut Node)>,
) {
    for (body, mut reveal, mut node) in &mut bodies {
        let previous = reveal.0;
        let fraction = reveal.advance(
            settings.sections.is_open(body.0),
            time.delta_secs(),
            motion.reduced || !settings.open,
        );
        let display = if fraction == 0. {
            Display::None
        } else {
            Display::Grid
        };
        // Settled sections do not allocate tracks or invalidate layout each frame.
        if previous == reveal.0 && node.display == display {
            continue;
        }
        let rows = vec![GridTrack::flex(fraction)];
        if node.display != display {
            node.display = display;
        }
        if node.grid_template_rows != rows {
            node.grid_template_rows = rows;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reveal_moves_in_both_directions_and_respects_reduced_motion() {
        let mut reveal = Reveal::new(false);
        let halfway = reveal.advance(true, 0.1, false);
        assert!(halfway > 0. && halfway < 1.);
        let reversing = reveal.advance(false, 0.02, false);
        assert!(reversing > 0. && reversing < halfway);
        assert_eq!(reveal.advance(false, 1., false), 0.);
        assert_eq!(reveal.advance(true, 0., true), 1.);
        assert_eq!(reveal.advance(false, 0., true), 0.);
    }
}
