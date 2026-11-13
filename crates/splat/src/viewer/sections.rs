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

pub(super) use dimensify_ui::viewer::{Motion, Reveal};

pub(super) fn animate(
    time: Res<Time<Real>>,
    motion: Res<Motion>,
    settings: Res<super::settings::ViewerSettings>,
    mut bodies: Query<(&Body, &mut Reveal, &mut Node)>,
) {
    for (body, mut reveal, mut node) in &mut bodies {
        if reveal.update_node(
            node.bypass_change_detection(),
            settings.sections.is_open(body.0),
            time.delta_secs(),
            motion.reduced || !settings.open,
        ) {
            node.set_changed();
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
