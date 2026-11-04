//! Independent disclosure state for the viewer settings sections.
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
    pub fn show(&mut self, section: Section) {
        self.0 |= 1 << section as u8;
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
