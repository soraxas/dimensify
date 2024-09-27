use crate::constants::{DEFAULT_COLOR, DEFAULT_OPACITY};
use crate::graphics::InstancedMaterials;
use crate::scene_graphics::graphic_node::{
    NodeDataGraphics, NodeDataGraphicsPhysics, NodeWithGraphics, NodeWithGraphicsAndPhysics,
    NodeWithGraphicsAndPhysicsBuilder, NodeWithGraphicsBuilder,
};
use crate::scene_graphics::helpers::{bevy_mesh, generate_collider_mesh};
use crate::scene_graphics::prefab_mesh::{self, PrefabMesh};
use crate::BevyMaterial;
use bevy::asset::{Assets, Handle};
use bevy::color::{Color, Srgba};
use bevy::math::{Quat, Vec3};
use bevy::prelude::{BuildChildren, Commands, Mesh, SpatialBundle, Transform};
use bevy_ecs::entity;
use bevy_ecs::system::EntityCommands;
use bevy_pbr::wireframe::Wireframe;
use bevy_pbr::{PbrBundle, StandardMaterial};
use derive_builder::Builder;
use na::Point3;
use rapier3d::dynamics::{RigidBodyHandle, RigidBodySet};
use rapier3d::geometry::{Collider, ColliderHandle, ColliderSet, Cone, Cylinder, Shape, ShapeType};
use rapier3d::math::Isometry;
use rapier3d::prelude::{point, ColliderBuilder, Real, RigidBody};
use std::collections::HashMap;

#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub struct ColliderAsPrefabMeshWithPhysicsSpawner<'a> {
    pub handle: Option<ColliderHandle>,
    pub collider: &'a Collider,
    pub prefab_meshes: &'a mut HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: &'a mut InstancedMaterials,
    #[builder(default = "Isometry::identity()")]
    pub delta: Isometry<Real>,

    #[builder(default = "DEFAULT_COLOR")]
    pub color: Point3<f32>,
}

use thiserror::Error;
#[derive(Error, Debug)]
pub enum SpawnError {
    #[error("Collider set is missing, but a raw collider is provided")]
    ColliderSetMissing,
    #[error("Body set is missing, but raw body is provided")]
    BodySetMissing,
    #[error("Material is empty. This visual is either explicitly constructed as empty, or already consumed")]
    EntityMaterialMissing,
    #[error("Collider position is missing. Either provide it, or use a collider with a position")]
    ColliderPosMissing,
}

#[derive(Debug)]
pub enum EntityDataVisual {
    Color(Point3<f32>),
    Material(BevyMaterial),
    MaterialHandle(Handle<BevyMaterial>),
    Empty,
}

impl<T> From<T> for EntityDataVisual
where
    T: Into<Point3<f32>>,
{
    fn from(point3: T) -> Self {
        Self::Color(point3.into())
    }
}

impl EntityDataVisual {
    pub fn build_material(
        &mut self,
        materials: &mut Assets<BevyMaterial>,
    ) -> Result<Handle<BevyMaterial>, SpawnError> {
        match std::mem::replace(self, Self::Empty) {
            Self::Empty => Err(SpawnError::EntityMaterialMissing),
            Self::MaterialHandle(handle) => Ok(handle),
            Self::Color(color) => {
                //default material property
                let bevy_color =
                    Color::from(Srgba::new(color.x, color.y, color.z, DEFAULT_OPACITY));
                Ok(materials.add(StandardMaterial {
                    metallic: 0.5,
                    perceptual_roughness: 0.5,
                    double_sided: true, // TODO: this doesn't do anything?
                    ..StandardMaterial::from(bevy_color)
                }))
            }
            Self::Material(material) => Ok(materials.add(material)),
        }
    }
}

#[derive(Debug)]
pub enum BodyDataType {
    Body(RigidBody),
    BodyHandle(RigidBodyHandle),
}

impl From<RigidBodyHandle> for BodyDataType {
    fn from(handle: RigidBodyHandle) -> Self {
        Self::BodyHandle(handle)
    }
}

#[derive(Debug)]
pub enum ColliderDataType<'a> {
    ColliderHandle(ColliderHandle),
    ColliderHandleWithRef(ColliderHandle, &'a Collider),
    Collider(Collider),
}

impl From<ColliderHandle> for ColliderDataType<'_> {
    fn from(handle: ColliderHandle) -> Self {
        Self::ColliderHandle(handle)
    }
}

impl From<Collider> for ColliderDataType<'_> {
    fn from(collider: Collider) -> Self {
        Self::Collider(collider)
    }
}

impl From<ColliderBuilder> for ColliderDataType<'_> {
    fn from(builder: ColliderBuilder) -> Self {
        Self::Collider(builder.build())
    }
}

pub enum ShapeType_ {
    Body(Collider),
    StandaloneShape(Box<dyn Shape>),
}

impl core::fmt::Debug for ShapeType_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeType_::Body(c) => write!(f, "ShapeType_(Body({:#?}))", c.shared_shape()),
            ShapeType_::StandaloneShape(_) => write!(f, "ShapeType_(StandaloneShape(..))"),
        }
    }
}

#[derive(Builder, Debug)]
#[builder(pattern = "owned")]
pub struct EntityData<'a> {
    #[builder(default)]
    pub collider: Option<ColliderDataType<'a>>,
    #[builder(default)]
    pub body: Option<BodyDataType>,
    // pub body_handle: Option<RigidBodyHandle>,
    #[builder(default)]
    pub shape: Option<ShapeType_>,
    #[builder(default = "DEFAULT_COLOR.into()")]
    pub material: EntityDataVisual,
    #[builder(default)]
    pub collider_pos: Option<Isometry<Real>>,
    #[builder(default)]
    pub delta: Isometry<Real>,
}

impl EntityData<'_> {
    pub fn get_collider_pos(&mut self) -> Result<Isometry<Real>, SpawnError> {
        let collider = &self.collider;
        self.collider_pos
            .or_else(|| match collider {
                Some(ColliderDataType::Collider(collider)) => Some(*collider.position()),
                Some(ColliderDataType::ColliderHandleWithRef(_, collider)) => {
                    Some(*collider.position())
                }
                _ => None,
            })
            .ok_or(SpawnError::ColliderPosMissing)
    }
}

fn spawn_unit_node(
    entity_commands: &mut EntityCommands,
    scale: Vec3,
    collider: Option<ColliderHandle>,
    body: Option<RigidBodyHandle>,
    collider_pos: &Isometry<Real>,
    delta: Isometry<Real>,
    mesh_handle: Option<Handle<Mesh>>,
    material_handle: Handle<StandardMaterial>,
) -> NodeWithGraphicsAndPhysics {
    let shape_pos = collider_pos * delta;
    let transform = transform_from_collider(&shape_pos, &delta, scale);
    let material_weak_handle = material_handle.clone_weak();

    if let Some(mesh) = mesh_handle {
        let bundle = PbrBundle {
            mesh,
            material: material_handle,
            transform,
            ..Default::default()
        };

        entity_commands.insert(bundle);
    }

    NodeWithGraphicsAndPhysicsBuilder::default()
        .collider(collider)
        .delta(delta)
        .data(NodeDataGraphicsPhysics {
            body,
            entity: Some(entity_commands.id()),
            opacity: DEFAULT_OPACITY,
        })
        .value(material_weak_handle.into())
        .build()
        .expect("All fields are set")
}

fn transform_from_collider(
    shape_pos: &Isometry<Real>,
    delta: &Isometry<Real>,
    scale: Vec3,
) -> Transform {
    let shape_pos = shape_pos * delta;
    Transform {
        translation: shape_pos.translation.vector.into(),
        rotation: Quat::from_xyzw(
            shape_pos.rotation.i,
            shape_pos.rotation.j,
            shape_pos.rotation.k,
            shape_pos.rotation.w,
        ),
        scale,
    }
}

fn maybe_collider_insert_with_parent(
    collider: Collider,
    collider_set: &mut ColliderSet,
    body_handle: Option<RigidBodyHandle>,
    body_set: &mut RigidBodySet,
) -> ColliderHandle {
    match body_handle {
        Some(handle) => collider_set.insert_with_parent(collider, handle, body_set),
        None => collider_set.insert(collider),
    }
}

pub fn spawn_datapack(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<BevyMaterial>,
    mut data_pack: EntityData,
    prefab_mesh: Option<&mut PrefabMesh>,
    collider_set: Option<&mut ColliderSet>,
    mut body_set: Option<&mut RigidBodySet>,
) -> Result<NodeWithGraphicsAndPhysics, SpawnError> {
    let shape_pos = data_pack.get_collider_pos()? * data_pack.delta;

    let material_handle = data_pack.material.build_material(materials)?;
    let material_weak_handle = material_handle.clone_weak();

    let body_handle = match data_pack.body {
        Some(BodyDataType::Body(body)) => match &mut body_set {
            Some(body_set) => Some(body_set.insert(body)),
            None => return Err(SpawnError::BodySetMissing),
        },
        Some(BodyDataType::BodyHandle(handle)) => Some(handle),
        None => None,
    };

    let (collider_handle, entity_id) = match data_pack.collider {
        Some(ColliderDataType::ColliderHandle(handle)) => (Some(handle), None),
        Some(ColliderDataType::Collider(..) | ColliderDataType::ColliderHandleWithRef(..)) => {
            let (collider, collider_handle) = match data_pack.collider {
                Some(ColliderDataType::Collider(collider)) => {
                    let body_set = body_set.ok_or(SpawnError::BodySetMissing)?;
                    let collider_set = collider_set.ok_or(SpawnError::ColliderSetMissing)?;

                    let collider_handle = maybe_collider_insert_with_parent(
                        collider,
                        collider_set,
                        body_handle,
                        body_set,
                    );
                    (&collider_set[collider_handle], collider_handle)
                }
                Some(ColliderDataType::ColliderHandleWithRef(handle, collider)) => {
                    (collider, handle)
                }
                _ => unreachable!(), // these are the two only possibilities, after the outer match arms.
            };

            let use_wireframe = collider.is_sensor();

            let shape = collider.shape();
            let entity_id = match shape.as_compound() {
                Some(compound) => {
                    let scale = PrefabMesh::get_mesh_scale(collider.shape()).unwrap_or(Vec3::ONE);
                    let transform =
                        transform_from_collider(collider.position(), &data_pack.delta, scale);
                    // need to nest this entity
                    let mut parent_entity =
                        commands.spawn(SpatialBundle::from_transform(transform));

                    let mut children: Vec<NodeWithGraphicsAndPhysics> = Vec::new();
                    parent_entity.with_children(|child_builder| {
                        for (inner_shape_pos, inner_shape) in compound.shapes() {
                            // recursively add all shapes in the compound

                            let child_entity = &mut child_builder.spawn_empty();
                            if use_wireframe {
                                child_entity.insert(Wireframe);
                            }

                            let inner_shape = &**inner_shape;

                            let mesh_handle = prefab_mesh
                                .as_ref()
                                .and_then(|prefab_mesh| {
                                    prefab_mesh.maybe_get_strong_prefab_mesh_handle(
                                        &inner_shape.shape_type(),
                                    )
                                })
                                .or_else(|| /* generate mesh if not found */
                                generate_collider_mesh(inner_shape).map(|m| meshes.add(m)));

                            // we don't want to use prefab-mesh scale, if we are not using prefab-mesh.
                            let scale = prefab_mesh
                                .as_ref()
                                .and_then(|_| PrefabMesh::get_mesh_scale(inner_shape))
                                .unwrap_or(Vec3::ONE);

                            // we don't need to add children directly to the main handler, as all operation will be transitive (by bevy)
                            children.push(spawn_unit_node(
                                child_entity,
                                scale,
                                Some(collider_handle),
                                body_handle,
                                inner_shape_pos,
                                data_pack.delta,
                                mesh_handle,
                                material_weak_handle.clone_weak(),
                            ));
                        }
                    });
                    // early return
                    return Ok(NodeWithGraphicsAndPhysicsBuilder::default()
                        .delta(data_pack.delta)
                        .collider(Some(collider_handle))
                        .data(NodeDataGraphicsPhysics {
                            body: None,
                            entity: Some(parent_entity.id()),
                            opacity: DEFAULT_OPACITY,
                        })
                        .value(children.into())
                        .build()
                        .expect("All fields are set"));
                }
                None => {
                    let mesh_handle = prefab_mesh
                        .as_ref()
                        .and_then(|prefab_mesh| {
                            prefab_mesh.maybe_get_strong_prefab_mesh_handle(&shape.shape_type())
                        })
                        .or_else(|| /* generate mesh if not found */
                                generate_collider_mesh(shape).map(|m| meshes.add(m)));

                    let mut entity_commands = commands.spawn_empty();
                    if use_wireframe {
                        entity_commands.insert(Wireframe);
                    }

                    if mesh_handle.is_none() {
                        panic!(
                            "Failed to generate mesh for shape {:#?}",
                            shape.shape_type()
                        );
                    }

                    let scale = prefab_mesh
                        .as_ref()
                        .and_then(|_| PrefabMesh::get_mesh_scale(shape))
                        .unwrap_or(Vec3::ONE);

                    spawn_unit_node(
                        &mut entity_commands,
                        scale,
                        Some(collider_handle),
                        body_handle,
                        &shape_pos,
                        data_pack.delta,
                        mesh_handle,
                        material_weak_handle.clone_weak(),
                    );
                    entity_commands.id()
                }
            };
            (Some(collider_handle), Some(entity_id))
        }
        None => todo!(),
    };

    // if let Some(shape) = data_pack.shape {
    //     let shape = shape.as_ref();
    //     let scale = collider_mesh_scale(shape);

    //     let mut transform = Transform::from_scale(scale);
    //     transform.translation.x = shape_pos.translation.vector.x;
    //     transform.translation.y = shape_pos.translation.vector.y;
    //     transform.translation.z = shape_pos.translation.vector.z;
    //     transform.rotation = Quat::from_xyzw(
    //         shape_pos.rotation.i,
    //         shape_pos.rotation.j,
    //         shape_pos.rotation.k,
    //         shape_pos.rotation.w,
    //     );

    //     if let Some(mesh) = generate_collider_mesh(shape) {
    //         let bundle = PbrBundle {
    //             mesh: meshes.add(mesh),
    //             material: material_handle.clone_weak(),
    //             transform,
    //             ..Default::default()
    //         };
    //         entity_commands.insert(bundle);
    //     } else {
    //         warn!("Failed to generate mesh for shape");
    //     }
    // }

    Ok(NodeWithGraphicsAndPhysicsBuilder::default()
        .collider(collider_handle)
        .delta(data_pack.delta)
        .data(NodeDataGraphicsPhysics {
            body: None,
            entity: entity_id,
            opacity: DEFAULT_OPACITY,
        })
        .value(material_handle.into())
        .build()
        .expect("All fields are set"))
}
