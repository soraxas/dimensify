//! Authored lens conventions at the camera ingestion boundary.

/// The axis to which an authored perspective angle refers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FovConvention {
    /// Canonical scene data and application lenses use vertical FOV.
    #[default]
    Vertical,
    /// SuperSplat uses horizontal FOV in landscape and vertical in portrait.
    LongerAxis,
}

impl FovConvention {
    /// Convert an authored angle into the vertical radians used by Bevy.
    pub fn vertical_radians(self, radians: f32, aspect: f32) -> f32 {
        let radians = super::clamp_vertical_fov(radians);
        let vertical = if self == Self::LongerAxis && aspect.is_finite() && aspect > 1.0 {
            2.0 * ((radians * 0.5).tan() / aspect).atan()
        } else {
            radians
        };
        super::clamp_vertical_fov(vertical)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conventions_convert_only_the_longer_landscape_axis() {
        for (convention, aspect, expected) in [
            (FovConvention::LongerAxis, 2.0, 53.130_1),
            (FovConvention::LongerAxis, 1.0, 90.0),
            (FovConvention::LongerAxis, 0.5, 90.0),
            (FovConvention::Vertical, 2.0, 90.0),
        ] {
            let actual = convention
                .vertical_radians(90.0_f32.to_radians(), aspect)
                .to_degrees();
            assert!(
                (actual - expected).abs() < 0.001,
                "{convention:?} aspect={aspect}: {actual}"
            );
        }
    }

    #[test]
    fn invalid_angles_and_aspects_follow_the_projection_safe_policy() {
        for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                FovConvention::LongerAxis
                    .vertical_radians(90.0_f32.to_radians(), aspect)
                    .to_degrees(),
                90.0
            );
        }
        for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                FovConvention::LongerAxis
                    .vertical_radians(angle, 1.0)
                    .to_degrees(),
                45.0
            );
        }
        for angle in [-1.0_f32, 0.0, 180.0, 360.0] {
            let actual = FovConvention::LongerAxis
                .vertical_radians(angle.to_radians(), 1.0)
                .to_degrees();
            assert!(actual > 0.0 && actual < 180.0);
        }
    }
}
