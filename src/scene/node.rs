#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use derive_builder::Builder;
use rapier3d::prelude::{RigidBodyForces, RigidBodyHandle};

use super::NodeData;
use rapier3d::geometry::ColliderHandle;
use rapier3d::math::{Isometry, Real};

#[derive(Clone, Debug)]
pub enum NodeInner<T, InnerData> {
    Nested { children: Vec<Node<T, InnerData>> },
    Standalone { leaf_data: Option<InnerData> },
}

impl<T, U> Default for NodeInner<T, U> {
    fn default() -> Self {
        NodeInner::Standalone {
            leaf_data: Default::default(),
        }
    }
}

/// Default into implementation for turning inner data into a standalone node
impl<T, InnerData> From<InnerData> for NodeInner<T, InnerData> {
    fn from(value: InnerData) -> Self {
        NodeInner::Standalone {
            leaf_data: Some(value),
        }
    }
}

/// Default into implementation for turning vector of nodes into a nested node
impl<T, InnerData> From<Vec<Node<T, InnerData>>> for NodeInner<T, InnerData> {
    fn from(children: Vec<Node<T, InnerData>>) -> Self {
        NodeInner::Nested { children }
    }
}

/// A component inside an object part
/// This contains the spatial coordinate
#[derive(Builder, Clone, Debug, Default)]
pub struct Node<IntermediateData, LeafData> {
    #[builder(default)]
    pub collider: Option<ColliderHandle>,
    #[builder(default)]
    pub delta: Isometry<Real>,
    #[builder(default)]
    pub value: NodeInner<IntermediateData, LeafData>,
    pub data: IntermediateData,
}

impl<T, U> Node<T, U> {
    pub fn new(
        // entity: Entity,
        collider: Option<ColliderHandle>,
        delta: Isometry<Real>,
        // opacity: f32,
        inner: NodeInner<T, U>,
        data: T,
    ) -> Self {
        Self {
            // entity,
            collider,
            delta,
            // opacity,
            value: inner,
            data,
        }
    }

    /// panics if this, potentially leaf node, has inner data
    pub fn children_mut(&mut self) -> &mut Vec<Node<T, U>> {
        if let NodeInner::Standalone { leaf_data } = &self.value {
            if leaf_data.is_some() {
                panic!("This is a leaf node, with existing data. Cannot have children.");
            } else {
                self.value = NodeInner::Nested { children: vec![] };
            }
        }

        match self.value {
            NodeInner::Nested { ref mut children } => children,
            NodeInner::Standalone { .. } => panic!("Should never reach. This is a leaf node"),
        }
    }

    /// a visitor pattern for the entity and its children
    pub fn visit_all_node_mut(&mut self, visitor: &mut impl FnMut(&mut Node<T, U>)) {
        visitor(self);
        match &mut self.value {
            NodeInner::Standalone { .. } => (),
            NodeInner::Nested {
                children: nested_children,
                ..
            } => nested_children.iter_mut().for_each(visitor),
        };
    }

    /// a visitor pattern for the entity and its children
    pub fn visit_leaf_node(&self, visitor: &mut impl FnMut(&Node<T, U>)) {
        match &self.value {
            NodeInner::Standalone { .. } => visitor(self),
            NodeInner::Nested {
                children: nested_children,
                ..
            } => nested_children.iter().for_each(visitor),
        };
    }

    /// a visitor pattern for the entity and its children
    pub fn visit_leaf_node_mut(&mut self, visitor: &mut impl FnMut(&mut Node<T, U>)) {
        match &mut self.value {
            NodeInner::Standalone { .. } => visitor(self),
            NodeInner::Nested {
                children: nested_children,
                ..
            } => nested_children.iter_mut().for_each(visitor),
        };
    }
}

impl<T, U> NodeData for Node<T, U> {
    fn get_collider_handle(&self) -> Option<ColliderHandle> {
        self.collider
    }
}

#[allow(dead_code)]
type BasicNode = Node<(), ()>;
