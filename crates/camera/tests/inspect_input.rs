#![cfg(feature = "inspect")]
use bevy::prelude::*;
use dimensify_camera::{
    InspectDrag, InspectGesture, InspectMotion, inspect_wheel_input, pointer_drag_owned,
};

#[test]
fn held_ui_press_cannot_acquire_a_drag_after_leaving_ui() {
    let blocked = pointer_drag_owned(false, true, true, false, false);
    assert!(!blocked);
    assert!(!pointer_drag_owned(blocked, true, false, false, true));
    let owned = pointer_drag_owned(false, true, true, false, true);
    assert!(owned);
    assert!(pointer_drag_owned(owned, false, false, true, false));
    assert!(!pointer_drag_owned(owned, false, false, false, true));
}

#[test]
fn drag_latches_pan_modifier_and_rebases_when_a_button_joins() {
    let mut drag = InspectDrag::default();
    let first = drag.update_owned(true, false, true).unwrap();
    assert!(matches!(
        first,
        InspectGesture::Mouse(_, InspectMotion::Pan)
    ));
    assert_eq!(drag.update_owned(true, false, false), Some(first));
    let second = drag.update_owned(true, true, false).unwrap();
    assert_ne!(first, second);
    assert!(matches!(
        second,
        InspectGesture::Mouse(_, InspectMotion::Pan)
    ));
    assert_eq!(drag.update_owned(true, false, false), Some(second));
    assert_eq!(drag.update_owned(false, false, false), None);
    let third = drag.update_owned(true, false, false).unwrap();
    assert!(matches!(
        third,
        InspectGesture::Mouse(_, InspectMotion::Orbit)
    ));
    assert_ne!(Some(third), drag.update_owned(false, false, false));
}

#[test]
fn wheel_priorities_preserve_combined_pan_zoom_and_logical_units() {
    let input = inspect_wheel_input(
        Some(Vec2::splat(200.0)),
        Vec2::new(3.0, -2.0),
        Vec2::ONE,
        2.0,
        false,
    );
    assert_eq!(
        input.gesture,
        Some(InspectGesture::Wheel(InspectMotion::Pan))
    );
    assert_eq!(input.delta, Vec2::new(3.0, -2.0));
    assert_eq!(input.zoom, 100.0);
    assert_eq!(input.pointer, Some(Vec2::splat(200.0)));
    assert_eq!(
        inspect_wheel_input(None, Vec2::ZERO, Vec2::ONE, 0.0, false).gesture,
        Some(InspectGesture::Wheel(InspectMotion::Orbit))
    );
}

#[test]
fn locked_orbit_still_allows_zoom_and_an_empty_frame_has_no_gesture() {
    let input = inspect_wheel_input(None, Vec2::ZERO, Vec2::ONE, -1.0, true);
    assert_eq!(
        input.gesture,
        Some(InspectGesture::Wheel(InspectMotion::Zoom))
    );
    assert_eq!(input.delta, Vec2::ZERO);
    assert_eq!(input.zoom, -50.0);
    assert_eq!(
        inspect_wheel_input(None, Vec2::ZERO, Vec2::ONE, 0.0, true).gesture,
        None
    );
}
