use rapier3d::data::arena::{Iter, IterMut};
use rapier3d::data::Arena;
use rapier3d::parry::partitioning::IndexedData;
use rapier3d::prelude::ColliderHandle;

use rapier3d::dynamics::RigidBodyHandle;
use rapier3d::math::Real;

use paste::paste;

use super::{NodeData, NodeDataWithPhysics};

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

define_handle!(SceneObjectHandle);
define_handle!(ObjectNodeHandle);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct SceneObjectPartHandle {
    pub object_handle: SceneObjectHandle,
    pub node_handle: ObjectNodeHandle,
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
    parts: Arena<NodeType>,
    pub state: Vec<Real>,
}

/// implements a macro for the `Inner Arena` type to allow iterating over the values of the arena.
/// This trait is common for both Scene and SceneObject
macro_rules! impl_arena_iter_extension {
    ($arena_field:ident,
        // name of the struct/enum
        $Item:ident
        // only one or none `<>`
        $(<
            // match one or more ident separated by a comma
            $( $generic:ident ),+
        >)?
        , $Handle:ident
        // represent the stored value inside arena
        , $inner_value_name:ident
    ) => {
        pub fn insert(&mut self, part: $Item$(< $( $generic ),+ >)?) -> $Handle {
            $Handle(self.$arena_field.insert(part))
        }
        pub fn insert_and_get_mut(&mut self, part: $Item$(< $( $generic ),+ >)?) -> &mut $Item$(< $( $generic ),+ >)? {
            self.$arena_field.insert_and_get_mut(part)
        }
        pub fn get(&self, handle: $Handle) -> Option<&$Item$(< $( $generic ),+ >)?> {
            self.$arena_field.get(handle.0)
        }
        pub fn get_mut(&mut self, handle: $Handle) -> Option<&mut $Item$(< $( $generic ),+ >)?> {
            self.$arena_field.get_mut(handle.0)
        }

        // implements the iterator (with custom name) for the inner value
paste! {
        pub fn [< iter_ $inner_value_name _arena >] (&self) -> Iter<$Item$(< $( $generic ),+ >)?> {
            self.$arena_field.iter()
        }

        pub fn  [< iter_ $inner_value_name _arena_mut >] (&mut self) -> IterMut<$Item$(< $( $generic ),+ >)?> {
            self.$arena_field.iter_mut()
        }

        pub fn [< iter_ $inner_value_name >] (&self) -> impl Iterator<Item = &$Item$(< $( $generic ),+ >)?> {
            self.$arena_field.iter_value()
        }

        pub fn [< iter_ $inner_value_name _mut >] (&mut self) -> impl Iterator<Item = &mut $Item$(< $( $generic ),+ >)?> {
            self.$arena_field.iter_value_mut()
        }
}
        pub fn clear(&mut self) {
            self.$arena_field.clear()
        }

        pub fn remove(&mut self, handle: $Handle) -> Option<$Item$(< $( $generic ),+ >)?> {
            self.$arena_field.remove(handle.0)
        }
    };
}

impl<NodeType> SceneObject<NodeType> {
    impl_arena_iter_extension!(parts, NodeType, ObjectNodeHandle, node);
}

#[derive(Default, Debug)]
pub struct Scene<NodeType> {
    objects: Arena<SceneObject<NodeType>>,
}

impl<NodeType> Scene<NodeType> {
    impl_arena_iter_extension!(objects, SceneObject<NodeType>, SceneObjectHandle, object);

    pub fn insert_object_part_empty(&mut self) -> SceneObjectPartHandle
    where
        NodeType: Default,
    {
        let object_handle = self.insert(SceneObject::<NodeType>::default());
        let part_handle = self
            .get_mut(object_handle)
            .expect("we had just inserted")
            .insert(NodeType::default());
        SceneObjectPartHandle {
            object_handle,
            node_handle: part_handle,
        }
    }
    pub fn insert_object_part(&mut self, object_part: NodeType) -> SceneObjectPartHandle
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
            node_handle: part_handle,
        }
    }

    // /// expensive operation (loop through all objects and parts)
    // pub fn get_handle_by_body_handle(
    //     &mut self,
    //     handle: RigidBodyHandle,
    // ) -> Option<SceneObjectPartHandle> {
    //     for (obj_handle, obj) in self.iter() {
    //         if let Some((part_handle, _)) = obj
    //             .iter()
    //             .find(|(_, op)| op.get_body_handle() == Some(handle))
    //         {
    //             return Some(SceneObjectPartHandle {
    //                 object_handle: SceneObjectHandle(obj_handle),
    //                 part_handle: InnerObjectPartHandle(part_handle),
    //             });
    //         }
    //     }
    //     None
    // }

    /// expensive operation (loop through all objects and parts)
    pub fn find_node(&self, criteria: impl Fn(&NodeType) -> bool) -> Option<SceneObjectPartHandle> {
        for (obj_handle, obj) in self.iter_object_arena() {
            if let Some((part_handle, _)) = obj.iter_node_arena().find(|(_, node)| criteria(node)) {
                return Some(SceneObjectPartHandle {
                    object_handle: SceneObjectHandle(obj_handle),
                    node_handle: ObjectNodeHandle(part_handle),
                });
            }
        }
        None
    }

    #[deprecated(note = "**Using this function is a mistake.**
    This function is slow, since it involves walking through all
    objects and node (see [find_node]).
    You should be keeping track of what's what, and ideally will
    never need to use this function.

    If you _do_ need to use this function, please consider a refactor.")]
    pub fn get_handle_by_collider_handle(
        &mut self,
        handle: ColliderHandle,
    ) -> Option<SceneObjectPartHandle>
    where
        NodeType: NodeData,
    {
        self.find_node(|op| op.get_collider_handle() == Some(handle))
    }

    #[deprecated(note = "**Using this function is a mistake.**
    See [get_handle_by_collider_handle].")]
    pub fn get_handle_by_body_handle(
        &mut self,
        handle: RigidBodyHandle,
    ) -> Option<SceneObjectPartHandle>
    where
        NodeType: NodeDataWithPhysics,
    {
        self.find_node(|op| op.get_body_handle() == Some(handle))
    }

    pub fn insert_new_object_part_as_collidable_with_physics(
        &mut self,
        handle: RigidBodyHandle,
    ) -> &mut NodeType
    where
        NodeType: Default + NodeDataWithPhysics,
    {
        self.insert_and_get_mut(SceneObject::default())
            .insert_and_get_mut(NodeType::new_from_body_handle(handle))
    }

    // pub fn insert_new_object_part(
    //     &mut self,
    //     handle: RigidBodyHandle,
    // ) -> &mut SceneObjectPart {
    //     let object = self.objects.insert_and_get_mut(SceneObject::default());
    //     object.insert_and_get_mut(SceneObjectPart::WithPhysics { body: handle })
    // }

    pub fn get_node(&self, handle: SceneObjectPartHandle) -> Option<&NodeType> {
        self.objects
            .get(handle.object_handle.0)
            .and_then(|o| o.get(handle.node_handle))
    }
    pub fn get_node_mut(&mut self, handle: SceneObjectPartHandle) -> Option<&mut NodeType> {
        self.objects
            .get_mut(handle.object_handle.0)
            .and_then(|o| o.get_mut(handle.node_handle))
    }

    pub fn remove_node(&mut self, handle: SceneObjectPartHandle) -> Option<NodeType> {
        self.objects
            .get_mut(handle.object_handle.0)
            .and_then(|o| o.remove(handle.node_handle))
    }

    pub fn iter_all_nodes(&self) -> impl Iterator<Item = &NodeType> {
        self.iter_object().flat_map(|op| op.iter_node())
    }

    pub fn iter_all_nodes_mut(&mut self) -> impl Iterator<Item = &mut NodeType> {
        self.iter_object_mut().flat_map(|op| op.iter_node_mut())
    }

    pub fn get_body_handle(&self, handle: SceneObjectPartHandle) -> Option<RigidBodyHandle>
    where
        NodeType: NodeDataWithPhysics,
    {
        self.get(handle.object_handle)
            .and_then(|o| o.get(handle.node_handle).and_then(|o| o.get_body_handle()))
    }
}
