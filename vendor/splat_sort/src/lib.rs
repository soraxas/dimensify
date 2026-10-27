//! Engine-independent view-depth ordering for static Gaussian captures.

#[cfg(feature = "worker")]
use wasm_bindgen::prelude::*;

/// Retained world positions and reusable sort storage for one source generation.
#[cfg_attr(feature = "worker", wasm_bindgen)]
pub struct DepthSorter {
    positions: Vec<f32>,
    keys: Vec<u32>,
    order: Vec<u32>,
    scratch: Vec<u32>,
    histogram: Vec<u32>,
}

#[cfg_attr(feature = "worker", wasm_bindgen)]
impl DepthSorter {
    /// Retain XYZ positions once, rather than transferring them on camera motion.
    #[cfg_attr(feature = "worker", wasm_bindgen(constructor))]
    pub fn new(positions: Vec<f32>) -> Result<Self, String> {
        if positions.len() % 3 != 0 || !positions.iter().all(|v| v.is_finite()) {
            return Err("Expected finite XYZ positions".into());
        }
        let count = positions.len() / 3;
        if count > u32::MAX as usize {
            return Err("Too many positions for u32 indices".into());
        }
        Ok(Self {
            positions,
            keys: vec![0; count],
            order: vec![0; count],
            scratch: vec![0; count],
            histogram: vec![0; 65536],
        })
    }

    /// Return back-to-front indices using the world-to-view Z row (including W).
    pub fn sort(&mut self, axis: &[f32]) -> Result<Vec<u32>, String> {
        if axis.len() != 4 || !axis.iter().all(|v| v.is_finite()) {
            return Err("Expected a finite world-to-view Z row".into());
        }
        for (i, p) in self.positions.chunks_exact(3).enumerate() {
            let depth = p[0] * axis[0] + p[1] * axis[1] + p[2] * axis[2] + axis[3];
            let bits = depth.to_bits();
            // Flip IEEE754 sign/magnitude into an unsigned ascending total order.
            // View Z is negative in front of Bevy's camera: ascending is far-first.
            self.keys[i] = if bits & 0x8000_0000 != 0 {
                !bits
            } else {
                bits ^ 0x8000_0000
            };
            self.order[i] = i as u32;
        }
        for shift in [0, 16] {
            self.histogram.fill(0);
            for &index in &self.order {
                self.histogram[((self.keys[index as usize] >> shift) & 65535) as usize] += 1;
            }
            let mut offset = 0;
            for count in &mut self.histogram {
                let n = *count;
                *count = offset;
                offset += n;
            }
            for &index in &self.order {
                let bucket = ((self.keys[index as usize] >> shift) & 65535) as usize;
                self.scratch[self.histogram[bucket] as usize] = index;
                self.histogram[bucket] += 1;
            }
            std::mem::swap(&mut self.order, &mut self.scratch);
        }
        Ok(self.order.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_depth_and_rotation_match_reference_without_reordering_positions() {
        let positions = vec![100.0, 0.0, -1.0, 0.0, 0.0, -10.0, 0.0, 0.0, 2.0];
        let mut sorter = DepthSorter::new(positions.clone()).unwrap();
        assert_eq!(sorter.sort(&[0.0, 0.0, 1.0, 0.0]).unwrap(), [1, 0, 2]);
        assert_eq!(sorter.sort(&[0.0, 0.0, -1.0, 0.0]).unwrap(), [2, 0, 1]);
        assert_eq!(sorter.positions, positions);
    }

    #[test]
    fn duplicates_are_stable_and_scratch_does_not_leak_previous_order() {
        let mut sorter =
            DepthSorter::new(vec![0.0, 0.0, -3.0, 0.0, 0.0, -3.0, 0.0, 0.0, -8.0]).unwrap();
        assert_eq!(sorter.sort(&[0.0, 0.0, 1.0, 0.0]).unwrap(), [2, 0, 1]);
        assert_eq!(sorter.sort(&[0.0; 4]).unwrap(), [0, 1, 2]);
    }

    #[test]
    fn radix_matches_stable_float_reference_at_capture_scale() {
        let positions: Vec<f32> = (0..100_003u32)
            .flat_map(|i| {
                let x = i.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                [
                    ((x & 65535) as f32 - 32768.0) / 32.0,
                    i as f32 % 127.0,
                    (x >> 16) as f32 - 32768.0,
                ]
            })
            .collect();
        let mut sorter = DepthSorter::new(positions.clone()).unwrap();
        for axis in [[0.0, 0.0, 1.0, 0.0], [0.3, -0.5, -0.8, 6.0]] {
            let mut expected: Vec<u32> = (0..positions.len() as u32 / 3).collect();
            let depth = |i: u32| {
                let p = &positions[i as usize * 3..];
                p[0] * axis[0] + p[1] * axis[1] + p[2] * axis[2] + axis[3]
            };
            expected.sort_by(|a, b| depth(*a).total_cmp(&depth(*b)));
            let actual = sorter.sort(&axis).unwrap();
            assert_eq!(actual.len(), expected.len());
            for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
                assert_eq!(*a, b, "order mismatch at {i}");
            }
        }
    }

    #[test]
    fn empty_and_invalid_inputs_are_bounded() {
        assert!(
            DepthSorter::new(vec![])
                .unwrap()
                .sort(&[0.0, 0.0, 1.0, 0.0])
                .unwrap()
                .is_empty()
        );
        assert!(DepthSorter::new(vec![0.0]).is_err());
        assert!(DepthSorter::new(vec![f32::NAN, 0.0, 0.0]).is_err());
        assert!(
            DepthSorter::new(vec![0.0; 3])
                .unwrap()
                .sort(&[0.0; 3])
                .is_err()
        );
    }
}
