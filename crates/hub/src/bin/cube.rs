//! The example's entire application schema: no site URLs, loaders or product types.
use dimensify_session::{DocumentProjection, RegistrationSet, SharedDocument, WritePolicy};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cube {
    pub center: [f32; 3],
    pub color: [f32; 3],
}
impl Default for Cube {
    fn default() -> Self {
        Self {
            center: [0.; 3],
            color: [0.2, 0.7, 0.9],
        }
    }
}
impl SharedDocument for Cube {
    const SCHEMA: &'static str = "example.shared-cube.v1";
    fn registrations() -> RegistrationSet<Self> {
        RegistrationSet::default()
            .document::<CubeState>(WritePolicy::Editors)
            .presentation_camera()
    }
    fn validate(&self) -> bool {
        self.center.iter().all(|v| v.is_finite() && v.abs() <= 3.)
            && self
                .color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    }
}

pub struct CubeState;
impl DocumentProjection<Cube> for CubeState {
    type Payload = Cube;
    const ID: &'static str = "example.cube";
    const VERSION: u32 = 1;
    fn read(d: &Cube) -> Cube {
        d.clone()
    }
    fn replace(d: &mut Cube, payload: Cube) {
        *d = payload;
    }
    fn validate(payload: &Cube) -> bool {
        payload.validate()
    }
}
