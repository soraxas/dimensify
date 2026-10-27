//! Headless camera locomotion over an analytic floor; no physics engine or window.
use bevy::prelude::*;
use dimensify_camera::{WalkBody, WalkEnvironment, WalkInput, WalkMotion, WalkMove};
use std::time::Duration;

struct Floor;

impl WalkEnvironment for Floor {
    fn is_clear(&mut self, eye: Vec3, eye_height: f32) -> bool {
        eye.y >= eye_height
    }

    fn terrain_height(&mut self, _: Vec3, _: f32) -> Option<f32> {
        Some(0.0)
    }

    fn move_eye(
        &mut self,
        eye: Vec3,
        velocity: Vec3,
        eye_height: f32,
        grounded: bool,
        dt: Duration,
    ) -> WalkMove {
        let mut position = eye + velocity * dt.as_secs_f32();
        let contact = position.y < eye_height;
        position.y = position.y.max(eye_height);
        WalkMove {
            position,
            vertical_velocity: if contact { 0.0 } else { velocity.y },
            grounded: grounded || contact,
        }
    }
}

fn main() {
    let mut body = WalkBody {
        position: Vec3::new(0.0, 1.5, 0.0),
        vertical_velocity: 0.0,
        grounded: true,
        eye_height: 1.3,
    };
    let mut motion = WalkMotion::new(&body);
    let mut support = motion.hold_level(body.eye_height);
    let mut pending = false;
    let mut peak = body.position.y;
    // Display at 120 Hz while the shared integrator runs at 60 Hz. A jump or
    // pointer impulse on a frame without a fixed step remains queued.
    for frame in 0..720 {
        let input = WalkInput {
            move_local: if frame < 120 { Vec3::Z } else { Vec3::ZERO },
            jump: frame == 60,
            ..default()
        };
        pending = motion.advance(&mut body, &mut support, &input, 1.0 / 120.0, &mut Floor);
        peak = peak.max(body.position.y);
    }
    assert!(peak > 2.0, "the jump must leave the floor");
    assert!(body.grounded && (body.position.y - 1.5).abs() < 0.01);
    assert!(!pending && motion.velocity == Vec2::ZERO);
    println!(
        "Final eye: {:?}; jump peak: {peak:.3}; motion settled",
        body.position
    );
}
