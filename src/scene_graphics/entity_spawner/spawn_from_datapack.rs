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
use bevy::prelude::{default, BuildChildren, Commands, Mesh, SpatialBundle, Transform};
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
    #[error("Node position is missing. Either provide it, or provide a collider (which contains a position)")]
    NodePosMissing,
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

impl From<RigidBody> for BodyDataType {
    fn from(body: RigidBody) -> Self {
        Self::Body(body)
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

#[derive(Default)]
pub enum ShapeSource {
    #[default]
    FromCollider,
    StandaloneShape(Box<dyn Shape>),
    None,
}

impl core::fmt::Debug for ShapeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeSource::None => write!(f, "ShapeSource(None)"),
            ShapeSource::FromCollider => write!(f, "ShapeSource(FromCollider)"),
            ShapeSource::StandaloneShape(_) => write!(f, "ShapeSource(StandaloneShape(..))"),
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
    #[builder(default)]
    pub shape_source: ShapeSource,
    #[builder(default = "DEFAULT_COLOR.into()")]
    pub material: EntityDataVisual,
    #[builder(default)]
    pub node_pos: Option<Isometry<Real>>,
    #[builder(default)]
    pub delta: Isometry<Real>,
}

impl<'a> EntityDataBuilder<'a> {
    pub fn done(self) -> EntityData<'a> {
        self.build()
            .expect("All required fields set at initialisation")
    }
}

impl EntityData<'_> {
    pub fn get_collider_pos(&mut self) -> Result<Isometry<Real>, SpawnError> {
        let collider = &self.collider;
        self.node_pos
            .or_else(|| match collider {
                Some(ColliderDataType::Collider(collider)) => Some(*collider.position()),
                Some(ColliderDataType::ColliderHandleWithRef(_, collider)) => {
                    Some(*collider.position())
                }
                _ => None,
            })
            .ok_or(SpawnError::NodePosMissing)
    }
}

fn spawn_unit_node(
    entity_commands: &mut EntityCommands,
    scale: Vec3,
    collider: Option<ColliderHandle>,
    body: Option<RigidBodyHandle>,
    shape_pos: &Isometry<Real>,
    delta: Isometry<Real>,
    mesh_handle: Option<Handle<Mesh>>,
    material_handle: Handle<StandardMaterial>,
) -> NodeWithGraphicsAndPhysics {
    let shape_pos = shape_pos * delta;
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

fn build_mesh_handle_and_scale(
    collider_shape: &dyn Shape,
    meshes: &mut Assets<Mesh>,
    data_pack: &mut EntityData,
    prefab_mesh: Option<&mut PrefabMesh>,
) -> (Option<Handle<Mesh>>, Vec3) {
    /// this is used inside the match branch, to avoid lifetime issue
    fn inner_helper(
        shape: &dyn Shape,
        meshes: &mut Assets<Mesh>,
        prefab_mesh: Option<&mut PrefabMesh>,
    ) -> (Option<Handle<Mesh>>, Option<Vec3>) {
        let mut scale = None;
        let mesh_handle = prefab_mesh
            .as_ref()
            .and_then(|prefab_mesh| {
                // get scale from prefab-mesh
                scale = PrefabMesh::get_mesh_scale(shape);
                prefab_mesh.maybe_get_strong_prefab_mesh_handle(&shape.shape_type())
            })
            .or_else(|| /* generate mesh if not found */
                    generate_collider_mesh(shape).map(|m| meshes.add(m)));
        (mesh_handle, scale)
    }

    let (mesh_handle, scale) = match &data_pack.shape_source {
        ShapeSource::None => (None, None),
        ShapeSource::FromCollider => inner_helper(collider_shape, meshes, prefab_mesh),
        ShapeSource::StandaloneShape(shape) => inner_helper(shape.as_ref(), meshes, prefab_mesh),
    };

    (mesh_handle, scale.unwrap_or(Vec3::ONE))
}

pub fn spawn_datapack(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<BevyMaterial>,
    mut data_pack: EntityData,
    mut prefab_mesh: Option<&mut PrefabMesh>,
    collider_set: Option<&mut ColliderSet>,
    mut body_set: Option<&mut RigidBodySet>,
) -> Result<NodeWithGraphicsAndPhysics, SpawnError> {
    let node_pos = data_pack.get_collider_pos()? * data_pack.delta;

    let material_handle = data_pack.material.build_material(materials)?;
    let material_weak_handle = material_handle.clone_weak();

    // handle cases for various body

    let body_handle = match std::mem::take(&mut data_pack.body) {
        Some(BodyDataType::Body(body)) => match &mut body_set {
            Some(body_set) => Some(body_set.insert(body)),
            None => return Err(SpawnError::BodySetMissing),
        },
        Some(BodyDataType::BodyHandle(handle)) => Some(handle),
        None => None,
    };

    // handle cases for collider and its nested structure
    let (collider_handle, entity_id) = match data_pack.collider {
        Some(ColliderDataType::ColliderHandle(handle)) => (Some(handle), None),
        Some(ColliderDataType::Collider(..) | ColliderDataType::ColliderHandleWithRef(..)) => {
            let (collider, collider_handle) = match std::mem::take(&mut data_pack.collider) {
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

                            let (mesh_handle, scale) = build_mesh_handle_and_scale(
                                inner_shape,
                                meshes,
                                &mut data_pack,
                                prefab_mesh.as_deref_mut(),
                            );

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
                    let (mesh_handle, scale) =
                        build_mesh_handle_and_scale(shape, meshes, &mut data_pack, prefab_mesh);

                    let mut entity_commands = commands.spawn_empty();
                    if use_wireframe {
                        entity_commands.insert(Wireframe);
                    }

                    spawn_unit_node(
                        &mut entity_commands,
                        scale,
                        Some(collider_handle),
                        body_handle,
                        &node_pos,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use rapier3d::prelude::RigidBodyBuilder;

    macro_rules! assert_match {
    ($expression:expr, $($pattern:tt)+) => {
        match $expression {
            $($pattern)+ => (),
            ref e => panic!("expected `{}` but got `{:?}`", stringify!($($pattern)+), e),
        }
    }
}

    fn run_functor_in_bevy_system(
        functor: impl Fn(&mut Commands, ResMut<Assets<Mesh>>, ResMut<Assets<BevyMaterial>>)
            + Sync
            + Send
            + 'static,
    ) {
        let mut app = App::new();

        app.add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<BevyMaterial>()
            .add_systems(
                Update,
                move |mut commands: Commands,
                      meshes: ResMut<Assets<Mesh>>,
                      materials: ResMut<Assets<BevyMaterial>>| {
                    functor(&mut commands, meshes, materials);
                },
            );

        // Run systems
        app.update();
    }

    #[test]
    fn no_required_fields_added_to_entity_data() {
        // This will panic if any fields have been added to Foo (and therefore FooBuilder)
        // that lack defaults.
        EntityDataBuilder::default().done();
    }

    #[test]
    fn each_node_needs_position() {
        run_functor_in_bevy_system(|commands, meshes, materials| {
            let data_pack = EntityDataBuilder::default()
                .material([0.0, 0.0, 1.0].into())
                .done();

            let res = spawn_datapack(
                commands,
                meshes.into_inner(),
                materials.into_inner(),
                data_pack,
                None,
                None,
                None,
            );

            assert_match!(res, Err(SpawnError::NodePosMissing));
        });
    }

    #[test]
    fn needs_body_set_if_raw_body_is_given() {
        run_functor_in_bevy_system(|commands, meshes, materials| {
            let data_pack = EntityDataBuilder::default()
                .material([0.0, 0.0, 1.0].into())
                // .body(RigidBodyBuilder::fixed().build().into())
                .done();

            let res = spawn_datapack(
                commands,
                meshes.into_inner(),
                materials.into_inner(),
                data_pack,
                None,
                None,
                None,
            );

            assert_match!(res, Err(SpawnError::NodePosMissing));
        });
    }

    #[test]
    fn test_into_trait() {
        let collider = ColliderBuilder::cuboid(1.0, 1.0, 1.0).build();
        assert_match!(collider.clone().into(), ColliderDataType::Collider(_));

        let mut collider_set = ColliderSet::new();
        let collider_handle = collider_set.insert(collider.clone());
        assert_match!(collider_handle.into(), ColliderDataType::ColliderHandle(_));

        let body = RigidBodyBuilder::fixed().build();
        assert_match!(body.clone().into(), BodyDataType::Body(_));

        let mut body_set = RigidBodySet::new();
        let body_handle = body_set.insert(body);
        assert_match!(body_handle.into(), BodyDataType::BodyHandle(_));

        assert_match!([0., 0.5, 1.0].into(), EntityDataVisual::Color(_));
    }
}
