//! Inspect motion with optional application-owned picking and collision.
use crate::{CameraPose, OrbitState, PoseTransition, TargetPose, clamp_vertical_fov};
use bevy::{platform::time::Instant, prelude::*, window::RequestRedraw};
use bevy_editor_cam::{controller::motion::CurrentMotion, prelude::EditorCam};
use std::time::Duration;
mod anchor;
mod input;
pub use anchor::{clip_ray_interval, inspect_view_anchor};
pub use input::{
    InspectDrag, InspectGesture, InspectInput, InspectMotion, inspect_wheel_input,
    pointer_drag_owned,
};

/// Scene surface returned by a consumer's picking implementation.
#[derive(Clone, Copy, Debug)]
pub struct InspectHit {
    /// World-space point in the camera's coordinate frame.
    pub point: Vec3,
    /// World-space surface normal.
    pub normal: Vec3,
}

/// Result of a pivot pick, which may resolve over several frames (GPU readback).
#[derive(Clone, Copy, Debug)]
pub enum InspectPick {
    Hit(InspectHit),
    /// Nothing under the pointer; the focus plane is used.
    Miss,
    /// Ask again next frame with the same pointer.
    Pending,
}

/// Optional scene queries. Physics, clipping and asset policy stay in consumers.
pub trait InspectEnvironment {
    /// Pick an approximate visible surface under a logical viewport position.
    /// Called once when a gesture starts, then each frame while it returns
    /// [`InspectPick::Pending`] (with the original pointer).
    fn pick_hit(
        &mut self,
        camera: &Camera,
        transform: &GlobalTransform,
        pointer: Vec2,
    ) -> InspectPick;
    /// Discard any in-flight pick before a new gesture asks again.
    fn cancel_pick(&mut self) {}
    /// Resolve an eye displacement against application-owned obstacles.
    fn constrain(&self, origin: Vec3, proposed: Vec3) -> Vec3;
}
impl InspectEnvironment for () {
    fn pick_hit(&mut self, _: &Camera, _: &GlobalTransform, _: Vec2) -> InspectPick {
        InspectPick::Miss
    }
    fn constrain(&self, _: Vec3, proposed: Vec3) -> Vec3 {
        proposed
    }
}

/// An unresolved pivot pick: the gesture waits, buffering its input.
struct PendingPivot {
    gesture: InspectGesture,
    pointer: Option<Vec2>,
    started: Instant,
    frames: u32,
    delta: Vec2,
    zoom: f32,
}

impl PendingPivot {
    /// Give up after a few rendered frames, but never sooner than a short floor,
    /// so pipelined rendering and slow first frames still resolve.
    fn expired(&self, now: Instant) -> bool {
        self.frames > 8 && now.saturating_duration_since(self.started) > Duration::from_millis(150)
    }
}

/// Semantic inputs and policy for a single controller update.
#[derive(Clone, Copy, Debug)]
pub struct InspectFrame {
    /// Device-independent gesture and displacement.
    pub input: InspectInput,
    /// Modal or lifecycle suppression cancels pending motion.
    pub motion_suppressed: bool,
    /// Selected-object pivot overrides pointer picking.
    pub selected_pivot: Option<Vec3>,
    /// Whether to ask the environment to constrain eye displacement.
    pub collision_enabled: bool,
    /// Normalized lens zoom used when OrbitState selects FOV zoom.
    pub lens_zoom: f32,
    /// Elapsed application frame time.
    pub delta_time: Duration,
}
impl Default for InspectFrame {
    fn default() -> Self {
        Self {
            input: default(),
            motion_suppressed: false,
            selected_pivot: None,
            collision_enabled: true,
            lens_zoom: 0.0,
            delta_time: Duration::ZERO,
        }
    }
}

/// Borrowed camera pipeline state; the consumer retains its single pose writer.
pub struct InspectContext<'a, 'b> {
    /// Camera viewport and projection metadata.
    pub camera: &'a Camera,
    /// Live projection to copy into scratch before updating it.
    pub projection: &'a Projection,
    /// Scratch projection, including orthographic scale output.
    pub scratch: Mut<'b, Projection>,
    /// Currently visible camera pose.
    pub live: &'a CameraPose,
    /// Semantic orbit state and view requests.
    pub orbit: &'a mut OrbitState,
    /// Requested pose updated by manipulation.
    pub target: &'a mut TargetPose,
    /// Shared transition interrupted by manipulation or restarted by view requests.
    pub transition: &'a mut PoseTransition,
}

/// Inspect motion state that publishes targets without writing render transforms.
#[derive(Resource, Default)]
pub struct InspectController {
    /// Motion settings and diagnostics; do not install the stock camera plugin.
    pub editor: EditorCam,
    gesture: Option<InspectGesture>,
    last_input: Option<Instant>,
    revision: u64,
    pivot: Option<(Vec3, Vec3)>,
    pending: Option<PendingPivot>,
}

impl InspectController {
    /// Return the frozen world-space pivot and surface normal.
    pub fn pivot(&self) -> Option<(Vec3, Vec3)> {
        self.pivot
    }
    /// Discard held input, smoothing and the frozen pivot.
    pub fn cancel(&mut self) {
        self.editor.current_motion = CurrentMotion::Stationary;
        self.gesture = None;
        self.last_input = None;
        self.pivot = None;
        self.pending = None;
    }
    /// Whether a gesture is waiting for its pivot pick.
    pub fn is_picking(&self) -> bool {
        self.pending.is_some()
    }
    fn begin(&mut self, gesture: InspectGesture, anchor: bevy::math::DVec3) {
        self.cancel();
        match gesture.motion() {
            InspectMotion::Orbit => self.editor.start_orbit(Some(anchor)),
            InspectMotion::Pan => self.editor.start_pan(Some(anchor)),
            InspectMotion::Zoom => self.editor.start_zoom(Some(anchor)),
        }
        self.gesture = Some(gesture);
        self.last_input = Some(Instant::now());
    }
}

impl InspectController {
    /// Advance manipulation and return whether hidden smoothing needs another frame.
    pub fn advance(
        &mut self,
        frame: &InspectFrame,
        context: InspectContext<'_, '_>,
        redraw: &mut MessageWriter<RequestRedraw>,
        environment: &mut impl InspectEnvironment,
    ) -> bool {
        let InspectContext {
            camera,
            projection,
            mut scratch,
            live,
            orbit,
            target,
            transition,
        } = context;
        let input = frame.input;
        // Semantic view requests are consumed once. Published fields remain useful
        // to UI/fit/playback, but never run a second damping loop during manipulation.
        if self.revision != orbit.revision() {
            self.cancel();
            self.revision = orbit.revision();
            target.0 = orbit.requested_pose();
            orbit.accept_requested_view();
            transition.start(live);
        }
        if frame.motion_suppressed || input.cancel {
            self.cancel();
            return false;
        }
        let now = Instant::now();
        let held = input.gesture.is_some();
        // A new gesture identity starts a pivot pick; input is buffered until the
        // pick resolves so an asynchronous (GPU) pick loses no motion.
        if let Some(gesture) = input.gesture
            && self.gesture != Some(gesture)
            && self.pending.as_ref().is_none_or(|p| p.gesture != gesture)
        {
            environment.cancel_pick();
            self.pending = Some(PendingPivot {
                gesture,
                pointer: input.pointer,
                started: now,
                frames: 0,
                delta: Vec2::ZERO,
                zoom: 0.0,
            });
        }
        let mut delta = input.delta;
        let mut zoom = input.zoom;
        if let Some(pending) = &mut self.pending {
            pending.delta += input.delta;
            pending.zoom += input.zoom;
            pending.frames += 1;
            let skip = frame.selected_pivot.is_some()
                || matches!(pending.gesture, InspectGesture::ViewCube(_));
            let pick = match pending.pointer {
                Some(pointer) if !skip => environment.pick_hit(
                    camera,
                    &GlobalTransform::from(live.to_transform()),
                    pointer,
                ),
                _ => InspectPick::Miss,
            };
            let hit = match pick {
                InspectPick::Hit(hit) => Some(hit),
                InspectPick::Pending if !pending.expired(now) => {
                    redraw.write(RequestRedraw);
                    return true;
                }
                InspectPick::Pending => {
                    environment.cancel_pick();
                    None
                }
                InspectPick::Miss => None,
            };
            let PendingPivot {
                gesture,
                pointer,
                delta: buffered,
                zoom: buffered_zoom,
                ..
            } = self.pending.take().expect("pending pivot");
            (delta, zoom) = (buffered, buffered_zoom);
            let surface = frame.selected_pivot.or(hit.map(|hit| hit.point));
            let anchor = if matches!(gesture, InspectGesture::ViewCube(_)) {
                bevy::math::DVec3::NEG_Z * f64::from(live.distance.max(0.01))
            } else {
                inspect_view_anchor(
                    camera,
                    live,
                    pointer,
                    surface,
                    self.editor.last_anchor_depth,
                )
            };
            let point = live.position + live.rotation() * anchor.as_vec3();
            let normal = hit
                .map_or(-live.forward(), |hit| hit.normal)
                .normalize_or(Vec3::Y);
            self.begin(gesture, anchor);
            self.pivot = Some((point, normal));
            target.0 = *live;
            transition.timer = 1.0;
        }
        if held {
            self.last_input = Some(now);
        }
        if !self.editor.current_motion.is_moving() {
            return false;
        }
        // Feed zeros while draining: reading the queue without feeding it would
        // repeat its cached sample. Never call end_move(), which enables a fling.
        self.editor.send_screenspace_input(delta);
        self.editor
            .send_zoom_input(if orbit.zoom_by_fov && !orbit.use_ortho {
                0.0
            } else {
                zoom.clamp(-500.0, 500.0)
            });
        *scratch = projection.clone();
        let original = target.0;
        if let Some((translation, rotation)) = self.editor.update_transform_and_projection(
            camera,
            &original.position.as_dvec3(),
            &original.rotation().as_dquat(),
            scratch.reborrow(),
            redraw,
            frame.delta_time,
        ) {
            let q = original.rotation().as_dquat();
            let proposed = (original.position.as_dvec3() + q * translation).as_vec3();
            let position = if frame.collision_enabled {
                environment.constrain(original.position, proposed)
            } else {
                proposed
            };
            let rotation = (q * rotation).normalize().as_quat();
            if position != proposed
                && let Some((point, _)) = self.pivot
            {
                let corrected = rotation.inverse() * (point - position);
                if let CurrentMotion::UserControlled { anchor, .. } =
                    &mut self.editor.current_motion
                {
                    *anchor = corrected.as_dvec3();
                }
                self.editor.last_anchor_depth = corrected.z as f64;
            }
            let distance = (-self.editor.last_anchor_depth).max(0.01) as f32;
            let fov = if orbit.zoom_by_fov && !orbit.use_ortho {
                clamp_vertical_fov(original.fov * (1.0 - frame.lens_zoom * 0.15))
            } else {
                original.fov
            };
            if position.is_finite() && rotation.is_finite() {
                target.0 = CameraPose::from_rotation(position, rotation, distance, fov);
                if let Projection::Orthographic(p) = &*scratch {
                    orbit.ortho_scale = p.scale.max(1e-5);
                    orbit.target_ortho_scale = orbit.ortho_scale;
                }
                orbit.publish_pose(&target.0);
            }
        }
        let drain = self
            .editor
            .smoothing
            .pan
            .max(self.editor.smoothing.orbit)
            .max(self.editor.smoothing.zoom);
        // A scroll burst retains its anchor across short pauses. Drag release only
        // waits for the finite smoothing window; it has no velocity-based momentum.
        let timeout = if matches!(self.gesture, Some(InspectGesture::Wheel(_))) {
            drain.max(Duration::from_millis(160))
        } else {
            drain
        };
        if !held
            && self
                .last_input
                .is_some_and(|last| now.saturating_duration_since(last) >= timeout)
        {
            self.cancel();
            false
        } else {
            true
        }
    }
}
