use bevy_math::Vec2;
type CursorBuffer = Playback<Vec2>;
type CursorSampleStamp = SampleStamp;
use super::*;
fn at(ms: u64) -> Duration {
    Duration::from_millis(ms)
}
fn push(b: &mut CursorBuffer, sequence: u64, ms: u64, p: Vec2) {
    b.push(
        CursorSampleStamp {
            stream: 1,
            sequence,
            received_at: at(ms),
        },
        p,
        at(ms),
    );
}
fn near(actual: Vec2, expected: Vec2) {
    assert!(
        actual.abs_diff_eq(expected, 0.001),
        "{actual:?} != {expected:?}"
    );
}
#[test]
fn reference_queue_uses_quadratic_cubic_then_quadratic_and_hits_each_knot() {
    let mut b = CursorBuffer::default();
    push(&mut b, 1, 0, Vec2::ZERO);
    assert_eq!(b.sample(at(0)), (Vec2::ZERO, false));
    push(&mut b, 2, 10, Vec2::new(80., 0.));
    push(&mut b, 3, 20, Vec2::new(80., 80.));
    push(&mut b, 4, 30, Vec2::new(160., 80.));
    near(b.sample(at(30)).0, Vec2::ZERO);
    near(b.sample(at(70)).0, Vec2::new(50., -10.));
    near(b.sample(at(110)).0, Vec2::new(80., 0.));
    near(b.sample(at(130)).0, Vec2::new(85.625, 18.125));
    near(b.sample(at(190)).0, Vec2::new(80., 80.));
    near(b.sample(at(230)).0, Vec2::new(110., 90.));
    assert_eq!(b.sample(at(270)), (Vec2::new(160., 80.), false));
}
#[test]
fn two_points_use_linear_and_arrivals_do_not_change_an_active_curve() {
    let mut b = CursorBuffer::default();
    push(&mut b, 1, 0, Vec2::ZERO);
    b.sample(at(0));
    push(&mut b, 2, 10, Vec2::new(80., 0.));
    b.sample(at(10));
    near(b.sample(at(50)).0, Vec2::new(40., 0.));
    push(&mut b, 3, 50, Vec2::new(80., 80.));
    near(b.sample(at(70)).0, Vec2::new(60., 0.));
    near(b.sample(at(90)).0, Vec2::new(80., 0.));
    assert_eq!(b.sample(at(170)), (Vec2::new(80., 80.), false));
}
#[test]
fn backlog_keeps_current_segment_then_skips_to_latest_with_linear_controls() {
    let mut b = CursorBuffer::default();
    push(&mut b, 1, 0, Vec2::ZERO);
    b.sample(at(0));
    push(&mut b, 2, 10, Vec2::new(80., 0.));
    b.sample(at(10));
    push(&mut b, 3, 20, Vec2::new(80., 80.));
    push(&mut b, 4, 30, Vec2::new(160., 80.));
    push(&mut b, 5, 40, Vec2::new(800., 400.));
    near(b.sample(at(50)).0, Vec2::new(40., 0.));
    near(b.sample(at(130)).0, Vec2::new(440., 200.));
    assert_eq!(b.sample(at(170)), (Vec2::new(800., 400.), false));
}
#[test]
fn idle_final_and_stream_resets_do_not_replay_old_segments() {
    let mut b = CursorBuffer::default();
    push(&mut b, 1, 0, Vec2::ZERO);
    push(&mut b, 2, 10, Vec2::splat(80.));
    b.sample(at(10));
    assert_eq!(b.sample(at(90)), (Vec2::splat(80.), false));
    push(&mut b, 3, 160, Vec2::splat(160.));
    assert_eq!(b.sample(at(160)).0, Vec2::splat(80.));
    assert!(b.sample(at(200)).0.x > 80.);
    assert_eq!(b.sample(at(240)), (Vec2::splat(160.), false));
    push(&mut b, 4, 1000, Vec2::splat(400.));
    assert_eq!(b.sample(at(1000)), (Vec2::splat(400.), false));
    b.push(
        CursorSampleStamp {
            stream: 2,
            sequence: 1,
            received_at: at(1001),
        },
        Vec2::splat(7.),
        at(1001),
    );
    assert_eq!(b.sample(at(1001)), (Vec2::splat(7.), false));
    b.push(
        CursorSampleStamp {
            stream: 2,
            sequence: 2,
            received_at: at(1010),
        },
        Vec2::splat(8.),
        at(1010),
    );
    assert_eq!(b.sample(at(3000)), (Vec2::splat(8.), false));
}
#[test]
fn continuous_jitter_missing_and_invalid_samples_finish_without_starvation() {
    let mut b = CursorBuffer::default();
    push(&mut b, 1, 0, Vec2::ZERO);
    let mut previous = 0.;
    for seq in 2..50 {
        let ms = seq * 30;
        push(&mut b, seq * 2, ms, Vec2::splat(seq as f32));
        let (p, _) = b.sample(at(ms));
        assert!(p.is_finite());
        if seq > 5 {
            assert!(p.x > previous, "starved at {seq}");
        }
        previous = p.x;
    }
    push(&mut b, 90, 1480, Vec2::splat(999.)); // obsolete
    push(&mut b, 98, 1480, Vec2::splat(999.)); // duplicate
    push(&mut b, 100, 1480, Vec2::NAN);
    b.push(
        CursorSampleStamp {
            stream: 1,
            sequence: 101,
            received_at: at(2000),
        },
        Vec2::splat(999.),
        at(1480),
    );
    for ms in (1500..1900).step_by(20) {
        b.sample(at(ms));
    }
    assert_eq!(b.sample(at(1900)), (Vec2::splat(49.), false));
}
#[test]
fn repeated_knots_and_bursts_remain_finite_and_bounded() {
    let mut b = CursorBuffer::default();
    push(&mut b, 1, 0, Vec2::ZERO);
    for seq in 2..1000 {
        push(&mut b, seq, seq, Vec2::splat((seq / 4) as f32));
        assert!(b.knots.len() <= 5);
    }
    for ms in (1000..1320).step_by(10) {
        assert!(b.sample(at(ms)).0.is_finite());
    }
    assert_eq!(b.sample(at(1320)), (Vec2::splat(249.), false));
    let mut still = CursorBuffer::default();
    for seq in 1..5 {
        push(&mut still, seq, seq, Vec2::splat(8.));
    }
    for ms in (4..260).step_by(8) {
        near(still.sample(at(ms)).0, Vec2::splat(8.));
    }
    assert_eq!(still.sample(at(300)), (Vec2::splat(8.), false));
}

#[test]
fn three_dimensional_position_uses_all_axes_with_the_same_queue_policy() {
    use bevy_math::Vec3;
    let mut b = Playback::<Vec3>::default();
    for (i, point) in [
        Vec3::ZERO,
        Vec3::new(80., 0., 0.),
        Vec3::new(80., 0., 80.),
        Vec3::new(160., 0., 80.),
    ]
    .into_iter()
    .enumerate()
    {
        let now = at(i as u64 * 10);
        b.push(
            SampleStamp {
                stream: 3,
                sequence: i as u64 + 1,
                received_at: now,
            },
            point,
            now,
        );
    }
    b.sample(at(30));
    let (p, active) = b.sample(at(70));
    assert!(active && p.abs_diff_eq(Vec3::new(50., 0., -10.), 0.001));
    let (p, _) = b.sample(at(130));
    assert!(p.abs_diff_eq(Vec3::new(85.625, 0., 18.125), 0.001));
    assert_eq!(b.sample(at(270)), (Vec3::new(160., 0., 80.), false));
}

#[test]
fn orientation_follows_short_arc_and_accepts_equivalent_quaternion_signs() {
    use bevy_math::{Quat, Vec3};
    let mut b = Playback::<Quat>::default();
    let start = Quat::from_rotation_z(170_f32.to_radians());
    let end = Quat::from_rotation_z(-170_f32.to_radians());
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 1,
            received_at: at(0),
        },
        start,
        at(0),
    );
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 2,
            received_at: at(10),
        },
        end,
        at(10),
    );
    b.sample(at(10));
    let (q, active) = b.sample(at(50));
    assert!(active && q.is_normalized());
    assert!(
        (q * Vec3::X).abs_diff_eq(-Vec3::X, 0.001),
        "took long arc: {q:?}"
    );
    assert!(!b.sample(at(90)).1);
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 3,
            received_at: at(100),
        },
        -end,
        at(100),
    );
    b.sample(at(100));
    let (q, _) = b.sample(at(140));
    assert!((q * Vec3::X).abs_diff_eq(end * Vec3::X, 0.001));
    assert!(q.is_normalized());
    assert!(!b.sample(at(180)).1);
}

#[test]
fn invalid_or_non_unit_orientations_cannot_corrupt_playback() {
    use bevy_math::Quat;
    let mut b = Playback::<Quat>::default();
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 1,
            received_at: at(0),
        },
        Quat::from_xyzw(0., 0., 0., 2.),
        at(0),
    );
    assert_eq!(b.sample(at(0)), (Quat::IDENTITY, false));
    for (i, q) in [Quat::from_xyzw(0., 0., 0., 0.), Quat::NAN]
        .into_iter()
        .enumerate()
    {
        b.push(
            SampleStamp {
                stream: 1,
                sequence: i as u64 + 2,
                received_at: at(10),
            },
            q,
            at(10),
        );
    }
    assert_eq!(b.sample(at(20)), (Quat::IDENTITY, false));
    assert_eq!(b.buffered_samples(), 1);
}

#[test]
fn clearing_a_pose_buffer_retains_consumer_timing_and_drops_old_order() {
    use bevy_math::Vec3;
    let mut b = Playback::<Vec3>::new(PlaybackSettings {
        segment_duration: at(40),
        stall_reset: at(500),
    });
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 40,
            received_at: at(0),
        },
        Vec3::ONE,
        at(0),
    );
    b.clear();
    assert_eq!(b.buffered_samples(), 0);
    assert_eq!(b.sample(at(10)), (Vec3::ZERO, false));
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 1,
            received_at: at(10),
        },
        Vec3::ZERO,
        at(10),
    );
    b.push(
        SampleStamp {
            stream: 1,
            sequence: 2,
            received_at: at(20),
        },
        Vec3::splat(4.),
        at(20),
    );
    b.sample(at(20));
    assert!(b.sample(at(40)).0.abs_diff_eq(Vec3::splat(2.), 0.001));
    assert_eq!(b.sample(at(60)), (Vec3::splat(4.), false));
}
