use bevy::prelude::*;
use dimensify_camera::{WalkBody, WalkEnvironment, WalkInput, WalkMotion, WalkMove, WalkSupport};
use std::time::Duration;

struct Floor {
    height: Option<f32>,
    wall: Option<f32>,
}

impl WalkEnvironment for Floor {
    fn is_clear(&mut self, _: Vec3, _: f32) -> bool {
        true
    }
    fn terrain_height(&mut self, _: Vec3, _: f32) -> Option<f32> {
        self.height
    }
    fn move_eye(
        &mut self,
        eye: Vec3,
        velocity: Vec3,
        _: f32,
        grounded: bool,
        dt: Duration,
    ) -> WalkMove {
        let mut position = eye + velocity * dt.as_secs_f32();
        if let Some(z) = self.wall {
            position.z = position.z.max(z);
        }
        WalkMove {
            position,
            vertical_velocity: velocity.y,
            grounded,
        }
    }
}

fn body() -> WalkBody {
    WalkBody {
        position: Vec3::new(0.0, 1.5, 0.0),
        vertical_velocity: 0.0,
        grounded: true,
        eye_height: 1.3,
    }
}

#[test]
fn pointer_impulse_survives_a_frame_without_a_step_and_never_jumps() {
    for hz in [60, 120] {
        let mut pointer = body();
        let mut world = body();
        let mut a = WalkMotion::new(&pointer);
        let mut b = WalkMotion::new(&world);
        let mut sa = WalkSupport::Collision;
        let mut sb = WalkSupport::Collision;
        let mut floor = Floor {
            height: Some(0.0),
            wall: None,
        };
        for frame in 0..hz {
            a.advance(
                &mut pointer,
                &mut sa,
                &WalkInput {
                    pointer_move: if frame == 0 {
                        Vec3::new(1.0, 10.0, 1.0)
                    } else {
                        Vec3::ZERO
                    },
                    speed: 16.0,
                    ..default()
                },
                1.0 / hz as f64,
                &mut floor,
            );
            b.advance(
                &mut world,
                &mut sb,
                &WalkInput {
                    world_move: if frame == 0 {
                        Vec3::new(1.0, 0.0, -1.0)
                    } else {
                        Vec3::ZERO
                    },
                    ..default()
                },
                1.0 / hz as f64,
                &mut floor,
            );
        }
        assert!(pointer.position.abs_diff_eq(world.position, 1e-5));
        assert!(pointer.position.x > 0.3);
        assert!((pointer.position.y - 1.5).abs() < 0.025);
    }
}

#[test]
fn jump_edge_is_queued_until_a_fixed_step() {
    let mut body = body();
    let mut motion = WalkMotion::new(&body);
    let mut support = WalkSupport::Collision;
    let mut floor = Floor {
        height: Some(0.0),
        wall: None,
    };
    assert!(motion.advance(
        &mut body,
        &mut support,
        &WalkInput {
            jump: true,
            ..default()
        },
        1.0 / 120.0,
        &mut floor
    ));
    assert_eq!(body.vertical_velocity, 0.0);
    motion.advance(
        &mut body,
        &mut support,
        &WalkInput::default(),
        1.0 / 120.0,
        &mut floor,
    );
    assert!(body.vertical_velocity > 3.0);
    assert!(!body.grounded);
    motion.advance(
        &mut body,
        &mut support,
        &WalkInput::default(),
        1.0 / 60.0,
        &mut floor,
    );
    assert!(body.position.y > 1.5);
}

#[test]
fn a_long_display_frame_advances_at_most_ten_fixed_steps() {
    let mut a = body();
    let mut b = body();
    let mut ma = WalkMotion::new(&a);
    let mut mb = WalkMotion::new(&b);
    let mut sa = WalkSupport::Collision;
    let mut sb = WalkSupport::Collision;
    let mut floor = Floor {
        height: Some(0.0),
        wall: None,
    };
    let input = WalkInput {
        move_local: Vec3::Z,
        ..default()
    };
    ma.advance(&mut a, &mut sa, &input, 1.0, &mut floor);
    for _ in 0..10 {
        mb.advance(&mut b, &mut sb, &input, 1.0 / 60.0, &mut floor);
    }
    assert!(a.position.abs_diff_eq(b.position, 1e-5));
    assert!(ma.velocity.abs_diff_eq(mb.velocity, 1e-5));
}

#[test]
fn blocked_walk_retains_momentum_then_stops_requesting_frames_on_release() {
    let mut body = body();
    let mut motion = WalkMotion::new(&body);
    let mut support = WalkSupport::Collision;
    let mut floor = Floor {
        height: Some(0.0),
        wall: Some(-0.7),
    };
    for _ in 0..120 {
        assert!(motion.advance(
            &mut body,
            &mut support,
            &WalkInput {
                move_local: Vec3::Z,
                ..default()
            },
            1.0 / 60.0,
            &mut floor
        ));
    }
    assert!(body.position.z >= -0.7);
    assert!(motion.velocity.y < -2.0);
    let mut pending = true;
    for _ in 0..240 {
        pending = motion.advance(
            &mut body,
            &mut support,
            &WalkInput::default(),
            1.0 / 60.0,
            &mut floor,
        );
    }
    assert!(!pending);
    assert_eq!(motion.velocity, Vec2::ZERO);
}

#[test]
fn held_entry_level_is_retained_until_the_environment_can_attach() {
    struct NotReady;
    impl WalkEnvironment for NotReady {
        fn is_clear(&mut self, _: Vec3, _: f32) -> bool {
            false
        }
        fn terrain_height(&mut self, _: Vec3, _: f32) -> Option<f32> {
            panic!("unattached terrain must not be queried")
        }
        fn move_eye(&mut self, _: Vec3, _: Vec3, _: f32, _: bool, _: Duration) -> WalkMove {
            panic!("unattached collision must not run")
        }
    }
    let mut body = body();
    body.position.y = 10.0;
    let mut motion = WalkMotion::new(&body);
    let mut support = motion.hold_level(body.eye_height);
    for _ in 0..120 {
        motion.advance(
            &mut body,
            &mut support,
            &WalkInput::default(),
            1.0 / 60.0,
            &mut NotReady,
        );
    }
    assert!((body.position.y - 10.0).abs() < 1e-5);
    assert!(matches!(support, WalkSupport::Held(_)));
}
