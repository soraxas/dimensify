//! Baseline-independent assertions on the deterministic visual cases.
pub const SIZE: u32 = 256;
pub const CASES: &[&str] = &[
    "anisotropic",
    "overlap-front",
    "overlap-reverse",
    "sh-front",
    "sh-reverse",
];

pub fn check(name: &str, rgba: &[u8]) -> Result<(), String> {
    if !CASES.contains(&name) {
        return Err(format!("unknown visual case: {name}"));
    }
    if rgba.len() != (SIZE * SIZE * 4) as usize {
        return Err("capture dimensions mismatch".into());
    }
    let mut count = 0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (SIZE, SIZE, 0, 0);
    for (i, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
        if pixel[..3].iter().copied().max().unwrap() > 24 {
            count += 1;
            let (x, y) = (i as u32 % SIZE, i as u32 / SIZE);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if !(80..=(SIZE * SIZE / 2)).contains(&count) {
        return Err(format!("invalid Gaussian coverage: {count}"));
    }
    if min_x < 8 || min_y < 8 || max_x >= SIZE - 8 || max_y >= SIZE - 8 {
        return Err("Gaussian footprint clipped".into());
    }
    if name == "anisotropic" {
        if max_x - min_x < 2 * (max_y - min_y) {
            return Err("anisotropic footprint lost horizontal elongation".into());
        }
    } else {
        let center = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
        let (r, b) = (rgba[center] as i16, rgba[center + 2] as i16);
        let red = name == "overlap-front" || name == "sh-front";
        if (red && r - b < 25) || (!red && b - r < 25) {
            return Err(format!(
                "wrong view-dependent center color: red={r}, blue={b}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blank_and_bad_dimensions_are_rejected() {
        assert!(check("anisotropic", &vec![0; (SIZE * SIZE * 4) as usize]).is_err());
        assert!(check("overlap-front", &[255; 4]).is_err());
    }
    #[test]
    fn wrong_front_color_is_rejected() {
        let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
        for y in 100..156 {
            for x in 100..156 {
                let i = (y * SIZE as usize + x) * 4;
                pixels[i + 2] = 180;
                pixels[i + 3] = 255;
            }
        }
        assert!(check("overlap-front", &pixels).is_err());
        assert!(check("overlap-reverse", &pixels).is_ok());
    }
    #[test]
    fn round_is_not_anisotropic() {
        let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
        for y in 100..156 {
            for x in 100..156 {
                let i = (y * SIZE as usize + x) * 4;
                pixels[i] = 180;
            }
        }
        assert!(check("anisotropic", &pixels).is_err());
    }
    #[test]
    fn elongated_shape_is_accepted_and_gray_sh_is_rejected() {
        let mut pixels = vec![0; (SIZE * SIZE * 4) as usize];
        for y in 116..140 {
            for x in 70..186 {
                let i = (y * SIZE as usize + x) * 4;
                pixels[i] = 180;
                pixels[i + 1] = 80;
            }
        }
        assert!(check("anisotropic", &pixels).is_ok());
        assert!(check("sh-front", &pixels).is_ok());
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[2] = pixel[0];
        }
        assert!(check("sh-front", &pixels).is_err());
        assert!(check("sh-reverse", &pixels).is_err());
    }
}
