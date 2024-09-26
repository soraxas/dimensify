use rapier3d::data::arena::{Iter, IterMut};
use rapier3d::data::Arena;
use rapier3d::parry::partitioning::IndexedData;
use rapier3d::prelude::ColliderHandle;
use thiserror::Error;

use rapier3d::dynamics::RigidBodyHandle;
use rapier3d::math::Real;

use super::NodeData;

macro_rules! define_handle {
    ($handle_name:ident) => {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
        #[cfg_attr(feature = "serde-serialize", derive(Serialize, Deserialize))]
        #[repr(transparent)]
        pub struct $handle_name(rapier3d::data::arena::Index);

        impl $handle_name {
            /// Converts this handle into its (index, generation) components.
            pub fn into_raw_parts(self) -> (u32, u32) {
                self.0.into_raw_parts()
            }

            /// Reconstructs an handle from its (index, generation) components.
            pub fn from_raw_parts(id: u32, generation: u32) -> Self {
                Self(rapier3d::data::arena::Index::from_raw_parts(id, generation))
            }

            /// An always-invalid rigid-body handle.
            pub fn invalid() -> Self {
                Self(rapier3d::data::arena::Index::from_raw_parts(
                    u32::MAX,
                    u32::MAX,
                ))
            }
        }

        impl IndexedData for $handle_name {
            fn default() -> Self {
                Self(IndexedData::default())
            }

            fn index(&self) -> usize {
                self.0.index()
            }
        }
    };
}

define_handle!(ObjectHandle);
define_handle!(ObjectPartHandle);

#[derive(Error, Debug)]
pub enum SceneObjectPartInvalidError {
    #[error("This scene object part is not a variant with colliders")]
    NoColliders,
    #[error("unknown data store error")]
    Unknown,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct SceneObjectPartHandle {
    pub object_handle: ObjectHandle,
    pub part_handle: ObjectPartHandle,
}

#[derive(Debug, Default)]
pub enum SceneObjectPart<NodeType> {
    #[default]
    Empty,
    Collidable {
        nodes: Vec<NodeType>,
    },
    CollidableWithPhysics {
        nodes: Vec<NodeType>,
        body: RigidBodyHandle,
    },
}

impl<NodeType> SceneObjectPart<NodeType> {
    pub fn get_entities(&self) -> Option<&Vec<NodeType>> {
        match self {
            SceneObjectPart::Collidable { nodes: colliders } => Some(colliders),
            SceneObjectPart::CollidableWithPhysics {
                nodes: colliders, ..
            } => Some(colliders),
            _ => None,
        }
    }
    pub fn get_entities_mut(&mut self) -> Option<&mut Vec<NodeType>> {
        match self {
            SceneObjectPart::Collidable { nodes: colliders } => Some(colliders),
            SceneObjectPart::CollidableWithPhysics {
                nodes: colliders, ..
            } => Some(colliders),
            _ => None,
        }
    }

    pub fn insert_collider(
        &mut self,
        collider: NodeType,
    ) -> Result<(), SceneObjectPartInvalidError> {
        match self.get_entities_mut() {
            Some(colliders) => {
                colliders.push(collider);
                Ok(())
            }
            None => Err(SceneObjectPartInvalidError::Unknown),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &NodeType> {
        match self {
            SceneObjectPart::Collidable { nodes: colliders } => colliders.iter(),
            SceneObjectPart::CollidableWithPhysics {
                nodes: colliders, ..
            } => colliders.iter(),
            SceneObjectPart::Empty => [].iter(),
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut NodeType> {
        match self {
            SceneObjectPart::Collidable { nodes: colliders } => colliders.iter_mut(),
            SceneObjectPart::CollidableWithPhysics {
                nodes: colliders, ..
            } => colliders.iter_mut(),
            SceneObjectPart::Empty => [].iter_mut(),
        }
    }

    pub fn get_body_handle(&self) -> Option<RigidBodyHandle> {
        match self {
            SceneObjectPart::CollidableWithPhysics { body, .. } => Some(*body),
            _ => None,
        }
    }
}

/// Implement an extension trait for the `Arena` type to allow iterating over the values of the arena.
pub trait ArenaExtension {
    type Item;
    fn iter_value(&self) -> impl Iterator<Item = &Self::Item>;
    fn iter_value_mut(&mut self) -> impl Iterator<Item = &mut Self::Item>;
    fn insert_and_get_mut(&mut self, item: Self::Item) -> &mut Self::Item;
}

impl<T> ArenaExtension for Arena<T> {
    type Item = T;

    fn iter_value(&self) -> impl Iterator<Item = &Self::Item> {
        self.iter().map(|(_, v)| v)
    }
    fn iter_value_mut(&mut self) -> impl Iterator<Item = &mut Self::Item> {
        self.iter_mut().map(|(_, v)| v)
    }

    fn insert_and_get_mut(&mut self, item: Self::Item) -> &mut Self::Item {
        let handle = self.insert(item);
        self.get_mut(handle)
            .expect("Should always succeed as we had just inserted it")
    }
}

#[derive(Debug, Default)]
pub struct SceneObject<NodeType> {
    parts: Arena<SceneObjectPart<NodeType>>,
    pub state: Vec<Real>,
}

/// implements a macro for the `Inner Arena` type to allow iterating over the values of the arena.
/// This trait is common for both Scene and SceneObject
///
/// NOTE: this hard-coded XXXX<T,U> as  $gen_arg1:ident, $gen_arg2:ident,
/// because I'm not smart enough to figure out how to make it generic.
macro_rules! impl_arena_iter_extension {
    ($arena_field:ident,$Item:ident,$Handle:ident, $gen_arg1:ident) => {
        pub fn insert(&mut self, part: $Item<$gen_arg1>) -> $Handle {
            $Handle(self.$arena_field.insert(part))
        }
        pub fn insert_and_get_mut(&mut self, part: $Item<$gen_arg1>) -> &mut $Item<$gen_arg1> {
            self.$arena_field.insert_and_get_mut(part)
        }
        pub fn get(&self, handle: $Handle) -> Option<&$Item<$gen_arg1>> {
            self.$arena_field.get(handle.0)
        }
        pub fn get_mut(&mut self, handle: $Handle) -> Option<&mut $Item<$gen_arg1>> {
            self.$arena_field.get_mut(handle.0)
        }

        pub fn iter(&self) -> Iter<$Item<$gen_arg1>> {
            self.$arena_field.iter()
        }

        pub fn iter_mut(&mut self) -> IterMut<$Item<$gen_arg1>> {
            self.$arena_field.iter_mut()
        }

        pub fn iter_value(&self) -> impl Iterator<Item = &$Item<$gen_arg1>> {
            self.$arena_field.iter_value()
        }

        pub fn iter_value_mut(&mut self) -> impl Iterator<Item = &mut $Item<$gen_arg1>> {
            self.$arena_field.iter_value_mut()
        }

        pub fn clear(&mut self) {
            self.$arena_field.clear()
        }

        pub fn remove(&mut self, handle: $Handle) -> Option<$Item<$gen_arg1>> {
            self.$arena_field.remove(handle.0)
        }
    };
}

impl<NodeType> SceneObject<NodeType> {
    impl_arena_iter_extension!(parts, SceneObjectPart, ObjectPartHandle, NodeType);

    pub fn iter_all_entities(&self) -> impl Iterator<Item = &NodeType> {
        self.iter_value().filter_map(|p| p.get_entities()).flatten()
    }

    pub fn iter_all_entities_mut(&mut self) -> impl Iterator<Item = &mut NodeType> {
        self.iter_value_mut()
            .filter_map(|p| p.get_entities_mut())
            .flatten()
    }
}

#[derive(Default)]
pub struct Scene<NodeType> {
    objects: Arena<SceneObject<NodeType>>,
}

impl<NodeType> Scene<NodeType> {
    impl_arena_iter_extension!(objects, SceneObject, ObjectHandle, NodeType);

    pub fn insert_object_part_empty(&mut self) -> SceneObjectPartHandle
    where
        NodeType: Default,
    {
        let object_handle = self.insert(SceneObject::<NodeType>::default());
        let part_handle = self
            .get_mut(object_handle)
            .expect("we had just inserted")
            .insert(SceneObjectPart::Empty);
        SceneObjectPartHandle {
            object_handle,
            part_handle,
        }
    }
    pub fn insert_object_part(
        &mut self,
        object_part: SceneObjectPart<NodeType>,
    ) -> SceneObjectPartHandle
    where
        NodeType: Default,
    {
        let object_handle = self.insert(SceneObject::<NodeType>::default());
        let part_handle = self
            .get_mut(object_handle)
            .expect("we had just inserted")
            .insert(object_part);
        SceneObjectPartHandle {
            object_handle,
            part_handle,
        }
    }

    /// expensive operation (loop through all objects and parts)
    pub fn get_handle_by_body_handle(
        &mut self,
        handle: RigidBodyHandle,
    ) -> Option<SceneObjectPartHandle> {
        for (obj_handle, obj) in self.iter() {
            if let Some((part_handle, _)) = obj
                .iter()
                .find(|(_, op)| op.get_body_handle() == Some(handle))
            {
                return Some(SceneObjectPartHandle {
                    object_handle: ObjectHandle(obj_handle),
                    part_handle: ObjectPartHandle(part_handle),
                });
            }
        }
        None
    }

    /// expensive operation (loop through all objects and parts)
    pub fn get_handle_by_collider_handle(
        &mut self,
        handle: ColliderHandle,
    ) -> Option<SceneObjectPartHandle>
    where
        NodeType: NodeData,
    {
        for (obj_handle, obj) in self.iter() {
            if let Some((part_handle, _)) = obj
                .iter()
                .find(|(_, op)| op.iter().any(|e| e.get_collider_handle() == Some(handle)))
            {
                return Some(SceneObjectPartHandle {
                    object_handle: ObjectHandle(obj_handle),
                    part_handle: ObjectPartHandle(part_handle),
                });
            }
        }
        None
    }

    pub fn get_mut_by_body_handle(
        &mut self,
        handle: RigidBodyHandle,
    ) -> Option<&mut SceneObjectPart<NodeType>> {
        // the borrow checker doesn't currently work with early returns with mut references
        // (https://stackoverflow.com/questions/68262927/why-does-rust-consider-borrows-active-in-other-branches)
        self.objects
            .iter_mut()
            .flat_map(|(_, o)| o.iter_value_mut())
            .find(|o| Some(handle) == o.get_body_handle())
    }

    pub fn insert_new_object_part_as_collidable_with_physics(
        &mut self,
        handle: RigidBodyHandle,
    ) -> &mut SceneObjectPart<NodeType>
    where
        NodeType: Default,
    {
        self.insert_and_get_mut(SceneObject::default())
            .insert_and_get_mut(SceneObjectPart::CollidableWithPhysics {
                body: handle,
                nodes: Vec::new(),
            })
    }

    // pub fn insert_new_object_part(
    //     &mut self,
    //     handle: RigidBodyHandle,
    // ) -> &mut SceneObjectPart {
    //     let object = self.objects.insert_and_get_mut(SceneObject::default());
    //     object.insert_and_get_mut(SceneObjectPart::WithPhysics { body: handle })
    // }

    pub fn get_part(&self, handle: SceneObjectPartHandle) -> Option<&SceneObjectPart<NodeType>> {
        self.objects
            .get(handle.object_handle.0)
            .and_then(|o| o.get(handle.part_handle))
    }

    pub fn remove_part(
        &mut self,
        handle: SceneObjectPartHandle,
    ) -> Option<SceneObjectPart<NodeType>> {
        self.objects
            .get_mut(handle.object_handle.0)
            .and_then(|o| o.remove(handle.part_handle))
    }

    pub fn get_part_mut(
        &mut self,
        handle: SceneObjectPartHandle,
    ) -> Option<&mut SceneObjectPart<NodeType>> {
        self.objects
            .get_mut(handle.object_handle.0)
            .and_then(|o| o.get_mut(handle.part_handle))
    }

    pub fn iter_object_part(&self) -> impl Iterator<Item = &SceneObjectPart<NodeType>> {
        self.objects.iter().flat_map(|(_, o)| o.iter_value())
    }

    pub fn iter_object_part_mut(&mut self) -> impl Iterator<Item = &mut SceneObjectPart<NodeType>> {
        self.objects
            .iter_mut()
            .flat_map(|(_, o)| o.iter_value_mut())
    }

    pub fn iter_all_entities(&self) -> impl Iterator<Item = &NodeType> {
        self.iter_value().flat_map(|op| op.iter_all_entities())
    }

    pub fn iter_all_entities_mut(&mut self) -> impl Iterator<Item = &mut NodeType> {
        self.iter_value_mut()
            .flat_map(|op| op.iter_all_entities_mut())
    }

    pub fn get_body_handle(&self, handle: SceneObjectPartHandle) -> Option<RigidBodyHandle> {
        self.get(handle.object_handle)
            .and_then(|o| o.get(handle.part_handle).and_then(|o| o.get_body_handle()))
    }
}
