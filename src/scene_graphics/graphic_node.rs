#![allow(clippy::unnecessary_cast)] // Casts are needed for switching between f32/f64.

use bevy::asset::{Assets, Handle};
use bevy::color::{Color, Srgba};
use bevy::math::Quat;
use bevy::prelude::{warn, Transform};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::{Commands, Query};
use derive_builder::Builder;
use na::Point3;
use rapier3d::geometry::ColliderSet;

use rapier3d::math::{Isometry, Real, Vector};
use rapier3d::prelude::{RigidBody, RigidBodyHandle};

use crate::constants::DEFAULT_OPACITY;
use crate::scene::node::{Node, NodeBuilder, NodeInner};
use crate::scene::NodeDataWithPhysics;
use crate::BevyMaterial;

pub trait WithGraphicsExt {
    fn despawn(&mut self, commands: &mut Commands);

    fn set_color(&mut self, materials: &mut Assets<BevyMaterial>, color: Point3<f32>);

    /// based on the colliders' position, update the graphic's transform
    fn sync_graphics(
        &mut self,
        colliders: &ColliderSet,
        components: &mut Query<&mut Transform>,
        gfx_shift: &Vector<Real>,
    );

    fn get_material(&self) -> Option<&Handle<BevyMaterial>>;
}

/// A component inside an object part
/// This contains the spatial coordinate
#[derive(Builder, Clone, Debug, Default)]
pub struct NodeDataGraphics {
    #[builder(default)]
    pub entity: Option<Entity>,
    // pub collider: Option<ColliderHandle>,
    // #[builder(default)]
    // delta: Isometry<Real>,
    #[builder(default = "DEFAULT_OPACITY")]
    pub opacity: f32,
}

/// A component inside an object part
/// This contains the spatial coordinate
#[derive(Builder, Clone, Debug, Default)]
pub struct NodeDataGraphicsPhysics {
    #[builder(default)]
    pub entity: Option<Entity>,
    #[builder(default)]
    pub body: Option<RigidBodyHandle>,
    // pub collider: Option<ColliderHandle>,
    // #[builder(default)]
    // delta: Isometry<Real>,
    #[builder(default = "DEFAULT_OPACITY")]
    pub opacity: f32,
}

type GraphicNodeInnerData = Handle<BevyMaterial>;
/// external can use this type to interact with the node with graphics
pub type NodeWithGraphics = Node<NodeDataGraphics, GraphicNodeInnerData>;
pub type NodeWithGraphicsAndPhysics = Node<NodeDataGraphicsPhysics, GraphicNodeInnerData>;
/// builder for the node with graphics
pub type NodeWithGraphicsBuilder = NodeBuilder<NodeDataGraphics, GraphicNodeInnerData>;
pub type NodeWithGraphicsAndPhysicsBuilder =
    NodeBuilder<NodeDataGraphicsPhysics, GraphicNodeInnerData>;

macro_rules! impl_with_graphics {
    () => {
        fn despawn(&mut self, commands: &mut Commands) {
            //FIXME: Should this be despawn_recursive?

            if let Some(mut cmd) = self
                .data
                .entity
                .and_then(|entity| commands.get_entity(entity))
            {
                cmd.despawn();
            }
            // commands.entity(self.entity).despawn();
            // self.visit_node_with_entity(&mut |_, entity| {
            //     commands.entity(entity).despawn();
            // });
        }

        fn set_color(&mut self, materials: &mut Assets<BevyMaterial>, color: Point3<f32>) {
            match &mut self.value {
                NodeInner::Standalone {
                    leaf_data: material,
                } => match material.as_ref() {
                    Some(m_handle) => {
                        if let Some(material) = materials.get_mut(m_handle) {
                            material.base_color = Color::from(Srgba::new(
                                color.x,
                                color.y,
                                color.z,
                                self.data.opacity,
                            ));
                        }
                    }
                    None => warn!("Leaf node at {:#?} contains no material", self),
                },
                &mut NodeInner::Nested { .. } => self.visit_leaf_node_mut(&mut |node| {
                    node.set_color(materials, color);
                }),
            };
        }

        /// based on the colliders' position, update the graphic's transform
        /// If the node has no collider, it will loop into its children to try to sync the transform
        /// This function is recursive down to its node children, until the first
        /// node with collider is found.
        /// e.g.
        ///        x1
        ///       / \
        ///      x2  o1
        ///     / \   \
        ///    o2  x3  o3
        ///   / \    \  \
        /// o4  o5   x4  o7
        ///
        /// In this example, x are nodes with no collider, o are nodes with collider
        /// This function will sync the transform of o1, o2, o6
        /// Because o1 has a collider, o3 also has collider, but it is not the first parent node with collider
        /// non-parent nodes with collider, like o3, o4, o5, o7, should be setup in a way where the bevy system
        /// will use relative coordinates to the parent node with collider to render/position them.
        fn sync_graphics(
            &mut self,
            colliders: &ColliderSet,
            components: &mut Query<&mut Transform>,
            gfx_shift: &Vector<Real>,
        ) {
            if let Some(Some(co)) = self.collider.map(|c| colliders.get(c)) {
                if let Some(mut pos) = self
                    .data
                    .entity
                    .and_then(|entity| components.get_mut(entity).ok())
                {
                    let co_pos = co.position() * self.delta;
                    pos.translation.x = (co_pos.translation.vector.x + gfx_shift.x) as f32;
                    pos.translation.y = (co_pos.translation.vector.y + gfx_shift.y) as f32;
                    {
                        pos.translation.z = (co_pos.translation.vector.z + gfx_shift.z) as f32;
                        pos.rotation = Quat::from_xyzw(
                            co_pos.rotation.i as f32,
                            co_pos.rotation.j as f32,
                            co_pos.rotation.k as f32,
                            co_pos.rotation.w as f32,
                        );
                    }
                }
            } else {
                // loop into children to try to sync
                if let NodeInner::Nested { children } = &mut self.value {
                    children.iter_mut().for_each(|child| {
                        child.sync_graphics(colliders, components, gfx_shift);
                    });
                }
            }
        }

        fn get_material(&self) -> Option<&Handle<BevyMaterial>> {
            match &self.value {
                NodeInner::Standalone {
                    leaf_data: material,
                } => material.as_ref(),
                NodeInner::Nested { .. } => None,
            }
        }
    };
}

impl WithGraphicsExt for NodeWithGraphics {
    impl_with_graphics!();
}

impl WithGraphicsExt for NodeWithGraphicsAndPhysics {
    impl_with_graphics!();
}

impl NodeDataWithPhysics for NodeWithGraphicsAndPhysics {
    fn get_body_handle(&self) -> Option<RigidBodyHandle> {
        self.data.body
    }
    fn new_from_body_handle(body_handle: RigidBodyHandle) -> Self {
        NodeWithGraphicsAndPhysicsBuilder::default()
            .data(
                NodeDataGraphicsPhysicsBuilder::default()
                    .body(Some(body_handle))
                    .build()
                    .expect("All fields are set"),
            )
            .build()
            .expect("All fields are set")
    }
}
