//! Headless playback of two ordered cursor positions on a receiving clock.
use bevy_math::Vec3;
use dimensify_interpolation::{Playback, SampleStamp};
use std::time::Duration;

fn main() {
    let mut cursor = Playback::<Vec3>::default();
    for (sequence, milliseconds, value) in [(0, 0, Vec3::ZERO), (1, 80, Vec3::X * 10.0)] {
        let now = Duration::from_millis(milliseconds);
        cursor.push(
            SampleStamp {
                stream: 1,
                sequence,
                received_at: now,
            },
            value,
            now,
        );
    }
    for milliseconds in [80, 120, 160] {
        let (position, moving) = cursor.sample(Duration::from_millis(milliseconds));
        println!("{milliseconds} ms: cursor={position:?}, needs_frame={moving}");
        if milliseconds == 120 {
            assert!((position.x - 5.0).abs() < 1e-5);
        }
        if milliseconds == 160 {
            assert_eq!(position, Vec3::X * 10.0);
            assert!(!moving);
        }
    }
}
