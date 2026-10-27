//! Build one globally sorted cloud from non-overlapping selected leaf ranges.

use super::{DecodedChunk, SplatRange};
use bevy_gaussian_splatting::{Gaussian3d, Planar, PlanarGaussian3d};

/// Compose selected ranges into one expanded render source.
pub fn compose<'a>(
    ranges: &[SplatRange],
    source: impl Fn(usize) -> Option<&'a DecodedChunk>,
) -> Result<PlanarGaussian3d, String> {
    let mut output = PlanarGaussian3d::default();
    for range in ranges {
        if range.count == 0 {
            continue;
        }
        let chunk = source(range.file).ok_or("chunk not ready")?;
        let cloud = chunk.cloud.expanded()?;
        let end = range
            .offset
            .checked_add(range.count)
            .ok_or("range overflow")?;
        if end > chunk.count {
            return Err("range extends beyond decoded chunk".into());
        }
        let r = range.offset..end;
        output
            .position_visibility
            .extend_from_slice(&cloud.position_visibility[r.clone()]);
        output
            .spherical_harmonic
            .extend_from_slice(&cloud.spherical_harmonic[r.clone()]);
        output
            .rotation
            .extend_from_slice(&cloud.rotation[r.clone()]);
        output
            .scale_opacity
            .extend_from_slice(&cloud.scale_opacity[r]);
    }
    // Match the renderer's 32-entry alignment without copying source padding.
    let pad = (32 - output.len() % 32) % 32;
    let zero = Gaussian3d::default();
    output
        .position_visibility
        .extend(std::iter::repeat_n(zero.position_visibility, pad));
    output
        .spherical_harmonic
        .extend(std::iter::repeat_n(zero.spherical_harmonic, pad));
    output
        .rotation
        .extend(std::iter::repeat_n(zero.rotation, pad));
    output
        .scale_opacity
        .extend(std::iter::repeat_n(zero.scale_opacity, pad));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_selected_ranges_and_excludes_decoder_padding() {
        let splats: Vec<_> = (0..5)
            .map(|x| Gaussian3d {
                position_visibility: [x as f32, 0., 0., 1.].into(),
                ..Default::default()
            })
            .collect();
        let chunk = DecodedChunk {
            cloud: splats.into(),
            count: 4,
        };
        let ranges = [SplatRange {
            file: 0,
            offset: 1,
            count: 2,
        }];
        let out = compose(&ranges, |_| Some(&chunk)).unwrap();
        assert_eq!(out.position_visibility[0].position[0], 1.);
        assert_eq!(out.position_visibility[1].position[0], 2.);
        assert_eq!(out.len(), 32);
        assert!(out.scale_opacity[2..].iter().all(|g| g.opacity == 0.));
        assert!(
            compose(
                &[SplatRange {
                    file: 0,
                    offset: 3,
                    count: 2
                }],
                |_| Some(&chunk)
            )
            .is_err()
        );
    }
}
