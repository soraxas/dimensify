pub mod node;
pub mod scene_objects;

pub mod prelude {

    pub use super::scene_objects::*;
}

use rapier3d::prelude::{ColliderHandle, RigidBodyHandle};

/// Node data that are needed for using in scene
pub trait NodeData {
    fn get_collider_handle(&self) -> Option<ColliderHandle>;
}
pub trait NodeDataWithPhysics: NodeData {
    fn get_body_handle(&self) -> Option<RigidBodyHandle>;
    fn new_from_body_handle(body_handle: RigidBodyHandle) -> Self;
}
