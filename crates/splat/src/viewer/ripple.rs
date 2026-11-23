//! Right-click feedback: ripple the splat surface under the pointer.
//!
//! A right press that is released in place (not a pan drag) picks the surface
//! through the shared depth pick, starts a [`crate::effects::SplatRipples`]
//! ripple there and announces the point with [`SurfaceClicked`], the hook for
//! annotation tools. Picking follows the orbit-pivot toggle (`O`); with it off
//! the pick misses and nothing ripples.
use bevy::prelude::*;
use dimensify_camera::{
    InspectEnvironment, InspectPick,
    viewer::{NavigationConfig, NavigationPick},
};

/// A right-click landed on the splat surface at `point` (world space).
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct SurfaceClicked {
    pub point: Vec3,
}

/// Farthest the pointer may move between press and release, in logical pixels.
const CLICK_SLOP_PX: f32 = 4.;
/// Longest press that still counts as a click.
const CLICK_SECS: f32 = 0.35;
/// Give up on a pick that has not resolved after this long.
const PICK_SECS: f32 = 1.;
/// Ripple radius as a fraction of the camera's distance to the hit, so it
/// reads at the same screen size at any zoom.
const RADIUS_PER_DISTANCE: f32 = 0.15;

pub(super) fn install(app: &mut App) {
    app.init_resource::<crate::effects::SplatRipples>()
        .add_message::<SurfaceClicked>()
        .add_systems(
            Update,
            right_click.after(dimensify_camera::viewer::NavigationSystems),
        );
}

/// Whether a press released at `release` after `held` seconds is a click.
fn is_click(press: Vec2, release: Vec2, held: f32) -> bool {
    press.distance(release) <= CLICK_SLOP_PX && held <= CLICK_SECS
}

#[derive(Default)]
struct ClickState {
    /// Pointer and time of an outstanding right press.
    press: Option<(Vec2, f32)>,
    /// Pointer and start time of a pick awaiting its answer.
    picking: Option<(Vec2, f32)>,
}

#[allow(clippy::too_many_arguments)]
fn right_click(
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window>,
    hovered: Query<&bevy::picking::hover::Hovered>,
    config: Res<NavigationConfig>,
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut pick: ResMut<NavigationPick>,
    mut ripples: ResMut<crate::effects::SplatRipples>,
    mut clicked: MessageWriter<SurfaceClicked>,
    mut state: Local<ClickState>,
) {
    let now = time.elapsed_secs();
    let pointer = windows.iter().find_map(Window::cursor_position);
    if let Some(mouse) = mouse {
        let over_ui = hovered.iter().any(|hovered| hovered.0);
        if mouse.just_pressed(MouseButton::Right)
            && !over_ui
            && !config.blocked
            && let Some(pointer) = pointer
        {
            state.press = Some((pointer, now));
            // Start compiling the effect stage while the button is down.
            ripples.arm(true);
        }
        if mouse.just_released(MouseButton::Right)
            && let Some((press, start)) = state.press.take()
        {
            match pointer.filter(|&release| is_click(press, release, now - start)) {
                Some(_) => state.picking = Some((press, now)),
                None => ripples.arm(false),
            }
        }
    }

    let Some((pointer, start)) = state.picking else {
        return;
    };
    let view = pick.camera().and_then(|entity| cameras.get(entity).ok());
    let Some((camera, transform)) = view else {
        state.picking = None;
        ripples.arm(false);
        return;
    };
    match pick.pick_hit(camera, transform, pointer) {
        InspectPick::Hit(hit) => {
            let radius = RADIUS_PER_DISTANCE * transform.translation().distance(hit.point);
            ripples.spawn(hit.point, radius.max(1e-3));
            clicked.write(SurfaceClicked { point: hit.point });
            state.picking = None;
        }
        InspectPick::Pending if now - start < PICK_SECS => {}
        InspectPick::Pending => {
            pick.cancel_pick();
            state.picking = None;
            ripples.arm(false);
        }
        InspectPick::Miss => {
            state.picking = None;
            ripples.arm(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_short_press_released_in_place_is_a_click() {
        assert!(is_click(Vec2::ZERO, Vec2::new(3., 0.), 0.2));
        // A pan drag.
        assert!(!is_click(Vec2::ZERO, Vec2::new(10., 0.), 0.2));
        // A long hold.
        assert!(!is_click(Vec2::ZERO, Vec2::ZERO, 1.));
    }
}
