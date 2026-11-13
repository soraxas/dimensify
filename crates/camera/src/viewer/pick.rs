//! Renderer-agnostic pivot pick requests for orbit navigation.
//!
//! [`NavigationPick`] turns a gesture's pointer into a physical-pixel request and
//! waits for a responder (for example the depth-buffer readback) to answer it.
//! Without a responder, keep it disabled: picks then miss immediately and the
//! controller uses its focus plane.
use crate::{InspectEnvironment, InspectHit, InspectPick};
use bevy::prelude::*;
use std::sync::{Arc, Mutex};

/// One outstanding pick in render-target physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickRequest {
    pub id: u64,
    /// Main-world camera whose rendered depth is sampled.
    pub camera: Entity,
    /// Pointer in render-target physical pixels.
    pub pixel: UVec2,
    /// Search radius in physical pixels; the nearest surface within it wins.
    pub radius: u32,
}

/// Latest answer: request id and the world-space point, if any.
type PickAnswer = Option<(u64, Option<Vec3>)>;

/// Answers requests from any thread (for example a buffer-map callback).
#[derive(Clone, Default)]
pub struct PickResponder(Arc<Mutex<PickAnswer>>);

impl PickResponder {
    /// Publish the world-space point for `id`, or `None` for empty background.
    ///
    /// A late answer to an older (cancelled) request never replaces a newer one.
    pub fn respond(&self, id: u64, point: Option<Vec3>) {
        if let Ok(mut slot) = self.0.lock()
            && slot.is_none_or(|(answered, _)| is_newer(id, answered))
        {
            *slot = Some((id, point));
        }
    }

    fn take(&self, id: u64) -> Option<Option<Vec3>> {
        let mut slot = self.0.lock().ok()?;
        match *slot {
            Some((answered, point)) if answered == id => {
                *slot = None;
                Some(point)
            }
            _ => None,
        }
    }
}

/// Whether request `id` was issued after `other`, tolerating id wrap-around.
fn is_newer(id: u64, other: u64) -> bool {
    id.wrapping_sub(other) as i64 >= 0
}

/// Largest search radius in physical pixels, bounding the readback window.
const MAX_RADIUS: f32 = 128.0;

/// Toggleable pivot picking under the pointer, resolved asynchronously.
#[derive(Resource, Clone)]
pub struct NavigationPick {
    /// Pivot on the surface under the pointer instead of the focus plane.
    pub enabled: bool,
    /// Pointer tolerance in logical pixels, so sparse points remain pickable.
    pub radius_px: f32,
    camera: Option<Entity>,
    request: Option<PickRequest>,
    next_id: u64,
    responder: PickResponder,
}

impl Default for NavigationPick {
    fn default() -> Self {
        Self {
            enabled: false,
            radius_px: 12.0,
            camera: None,
            request: None,
            next_id: 0,
            responder: default(),
        }
    }
}

impl NavigationPick {
    /// The outstanding request, if any, for a responder to service.
    /// Hidden while picking is disabled, so a disabled request is never serviced.
    pub fn request(&self) -> Option<PickRequest> {
        self.request.filter(|_| self.enabled)
    }

    /// Handle used to answer [`Self::request`].
    pub fn responder(&self) -> PickResponder {
        self.responder.clone()
    }

    /// The camera whose depth is picked.
    pub fn camera(&self) -> Option<Entity> {
        self.camera
    }

    /// Select the camera whose depth is picked (navigation does this each frame).
    pub fn set_camera(&mut self, camera: Entity) {
        if self.camera != Some(camera) {
            self.camera = Some(camera);
            self.request = None;
        }
    }
}

impl InspectEnvironment for NavigationPick {
    fn pick_hit(
        &mut self,
        camera: &Camera,
        transform: &GlobalTransform,
        pointer: Vec2,
    ) -> InspectPick {
        if !self.enabled {
            // Drop any outstanding request so re-enabling never consumes its answer.
            self.request = None;
            return InspectPick::Miss;
        }
        if let Some(request) = self.request {
            let Some(point) = self.responder.take(request.id) else {
                return InspectPick::Pending;
            };
            self.request = None;
            return point.map_or(InspectPick::Miss, |point| {
                InspectPick::Hit(InspectHit {
                    point,
                    normal: (transform.translation() - point).normalize_or(Vec3::Y),
                })
            });
        }
        let (Some(entity), Some(rect), Some(scale)) = (
            self.camera,
            camera.physical_viewport_rect(),
            camera.target_scaling_factor(),
        ) else {
            return InspectPick::Miss;
        };
        // Pointers are target-relative logical pixels (as `viewport_to_world`
        // expects); the depth texture is addressed in target physical pixels.
        let pixel = pointer * scale;
        if !pointer.is_finite() || !rect.as_rect().contains(pixel) {
            return InspectPick::Miss;
        }
        self.next_id = self.next_id.wrapping_add(1);
        self.request = Some(PickRequest {
            id: self.next_id,
            camera: entity,
            pixel: pixel.as_uvec2(),
            // Bounded so the readback window cannot overflow; a NaN radius casts to 0.
            radius: (self.radius_px * scale).ceil().clamp(0.0, MAX_RADIUS) as u32,
        });
        InspectPick::Pending
    }

    fn cancel_pick(&mut self) {
        self.request = None;
    }

    fn constrain(&self, _: Vec3, proposed: Vec3) -> Vec3 {
        proposed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_late_answer_to_a_cancelled_request_keeps_the_newer_answer() {
        let responder = PickResponder::default();
        responder.respond(2, Some(Vec3::X));
        responder.respond(1, Some(Vec3::Y));
        assert_eq!(responder.take(2), Some(Some(Vec3::X)));
        // Ids compare across wrap-around.
        responder.respond(0, None);
        responder.respond(u64::MAX, Some(Vec3::Z));
        assert_eq!(responder.take(0), Some(None));
    }

    #[test]
    fn disabling_drops_the_outstanding_request() {
        let mut pick = NavigationPick {
            enabled: true,
            ..default()
        };
        pick.request = Some(PickRequest {
            id: 1,
            camera: Entity::PLACEHOLDER,
            pixel: UVec2::ZERO,
            radius: 1,
        });
        pick.responder().respond(1, Some(Vec3::X));
        pick.enabled = false;
        let (camera, transform) = (Camera::default(), GlobalTransform::default());
        assert!(matches!(
            pick.pick_hit(&camera, &transform, Vec2::ZERO),
            InspectPick::Miss
        ));
        assert_eq!(pick.request(), None);
    }
}
