//! Explicit source-coordinate cameras; asset outliers must not pick the view.
use bevy::prelude::*;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct CameraPath {
    pub target: Vec3,
    pub offset: Vec3,
    pub up: Vec3,
}

impl CameraPath {
    pub fn from_json(value: &Value) -> Result<Self, String> {
        fn vector(value: &Value, key: &str) -> Result<Vec3, String> {
            let array = value[key]
                .as_array()
                .ok_or_else(|| format!("camera {key} needs three numbers"))?;
            if array.len() != 3 {
                return Err(format!("camera {key} needs three numbers"));
            }
            let mut values = [0.0; 3];
            for (index, row) in array.iter().enumerate() {
                values[index] = row.as_f64().ok_or("camera coordinate must be numeric")? as f32;
            }
            let result = Vec3::from_array(values);
            result
                .is_finite()
                .then_some(result)
                .ok_or_else(|| "camera must be finite".into())
        }
        let eye = vector(value, "eye")?;
        let target = vector(value, "target")?;
        let up = vector(value, "up")?;
        let offset = eye - target;
        if !offset.length_squared().is_finite()
            || !up.length_squared().is_finite()
            || offset.length() < 0.01
            || up.length() < 0.01
            || offset.normalize().cross(up.normalize()).length() < 0.01
        {
            return Err("camera eye/target/up must form a nondegenerate view".into());
        }
        Ok(Self {
            target,
            offset,
            up: up.normalize(),
        })
    }

    pub fn transform(&self, side: bool, moving: bool, frame: usize, period: usize) -> Transform {
        let angle = if side { 0.55 } else { 0.0 }
            + if moving {
                std::f32::consts::FRAC_PI_4
                    * (frame as f32 / period as f32 * std::f32::consts::TAU).sin()
            } else {
                0.0
            };
        let offset =
            Quat::from_axis_angle(self.up, angle) * self.offset * if side { 0.65 } else { 1.0 };
        Transform::from_translation(self.target + offset).looking_at(self.target, self.up)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_camera_preserves_eye_and_down_axis_at_first_frame() {
        let path = CameraPath::from_json(
            &json!({"eye":[-3.8,0.1,-7.2],"target":[0.0,0.1,0.0],"up":[0.0,-1.0,0.0]}),
        )
        .unwrap();
        let first = path.transform(false, false, 0, 240);
        assert!(
            first
                .translation
                .abs_diff_eq(Vec3::new(-3.8, 0.1, -7.2), 1e-5)
        );
        assert!(first.up().abs_diff_eq(Vec3::NEG_Y, 1e-5));
        let side = path.transform(true, false, 0, 240);
        assert!(
            ((side.translation - path.target).length() / path.offset.length() - 0.65).abs() < 1e-5
        );
        assert!(
            path.transform(false, true, 60, 240)
                .translation
                .distance(first.translation)
                > 1.0
        );
    }

    #[test]
    fn malformed_and_parallel_cameras_are_rejected() {
        for value in [
            json!({}),
            json!({"eye":[0,0,0],"target":[0,0,0],"up":[0,1,0]}),
            json!({"eye":[0,1,0],"target":[0,0,0],"up":[0,1,0]}),
            json!({"eye":[0,0,"bad"],"target":[0,0,0],"up":[0,1,0]}),
        ] {
            assert!(CameraPath::from_json(&value).is_err());
        }
    }
}
