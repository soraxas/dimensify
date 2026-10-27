use bevy::prelude::*;
use dimensify_camera::{TouchGestureFrame, TouchMotionTracker, TouchSample};

fn point(id: u64, x: f32, y: f32, started: bool) -> TouchSample {
    TouchSample {
        id,
        position: Vec2::new(x, y),
        start_position: Vec2::new(x, y),
        just_started: started,
    }
}
fn update(state: &mut TouchMotionTracker, samples: &[TouchSample]) {
    state.update_owned(
        samples,
        true,
        Some(Rect::new(-1000.0, -1000.0, 1000.0, 1000.0)),
    );
}

#[test]
fn one_finger_and_two_finger_motion_preserve_existing_pixel_units() {
    let mut state = TouchMotionTracker::default();
    update(&mut state, &[point(1, 10.0, 20.0, true)]);
    assert_eq!(state.gesture(), TouchGestureFrame::None);
    update(&mut state, &[point(1, 18.0, 26.0, false)]);
    assert_eq!(
        state.gesture(),
        TouchGestureFrame::OneFinger {
            motion: Vec2::new(8.0, 6.0)
        }
    );
    state.clear();
    update(
        &mut state,
        &[point(1, 0.0, 0.0, true), point(2, 0.0, 100.0, true)],
    );
    update(
        &mut state,
        &[point(2, 10.0, 130.0, false), point(1, 10.0, -10.0, false)],
    );
    assert_eq!(
        state.gesture(),
        TouchGestureFrame::TwoFinger {
            motion: Vec2::new(10.0, 10.0),
            pinch: 40.0
        }
    );
    assert_eq!(state.current().0.unwrap().id, 1);
}

#[test]
fn finger_add_remove_replace_and_recycled_start_rebase_motion() {
    let mut state = TouchMotionTracker::default();
    update(&mut state, &[point(1, 0.0, 0.0, true)]);
    for samples in [
        vec![point(1, 40.0, 0.0, false), point(2, 100.0, 0.0, true)],
        vec![point(1, 80.0, 0.0, false)],
        vec![point(1, 200.0, 0.0, true)],
        vec![point(9, -200.0, 0.0, true)],
    ] {
        update(&mut state, &samples);
        assert_eq!(state.gesture(), TouchGestureFrame::None);
    }
}

#[test]
fn suppressed_and_third_finger_contacts_require_fresh_starts() {
    let mut state = TouchMotionTracker::default();
    update(&mut state, &[point(1, 0.0, 0.0, true)]);
    state.update_owned(
        &[point(1, 20.0, 0.0, false)],
        false,
        Some(Rect::new(-1000.0, -1000.0, 1000.0, 1000.0)),
    );
    update(&mut state, &[point(1, 40.0, 0.0, false)]);
    assert!(state.current().0.is_none());
    update(
        &mut state,
        &[
            point(1, 0.0, 0.0, true),
            point(2, 10.0, 0.0, true),
            point(3, 20.0, 0.0, true),
        ],
    );
    update(
        &mut state,
        &[point(1, 40.0, 0.0, false), point(2, 50.0, 0.0, false)],
    );
    assert!(state.current().0.is_none());
}

#[test]
fn mixed_viewport_ownership_never_becomes_a_pinch() {
    let mut state = TouchMotionTracker::default();
    let viewport = Some(Rect::new(0.0, 0.0, 100.0, 100.0));
    state.update_owned(
        &[point(1, 50.0, 50.0, true), point(2, 150.0, 50.0, true)],
        true,
        viewport,
    );
    state.update_owned(
        &[point(1, 60.0, 50.0, false), point(2, 90.0, 50.0, false)],
        true,
        viewport,
    );
    assert!(state.current().0.is_none());
    assert_eq!(state.gesture(), TouchGestureFrame::None);
}
