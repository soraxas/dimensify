use bevy::prelude::*;

use na::{point, Point3};
use rapier3d::data::Index;
use rapier3d::parry::partitioning::IndexedData;
use thiserror::Error;

use crate::constants::DEFAULT_COLOR;
use crate::dimensify::Plugins;
use crate::harness::Harness;
use crate::objects::node::EntityWithGraphics;
use rapier3d::dynamics::{RigidBodyHandle, RigidBodySet};
use rapier3d::geometry::{ColliderHandle, ColliderSet, ShapeType};
use rapier3d::math::{Isometry, Real, Vector};
//use crate::objects::capsule::Capsule;
//use crate::objects::plane::Plane;
// use crate::objects::mesh::Mesh;
use crate::objects::entity_spawner::{ColliderAsMeshSpawner, ColliderAsMeshSpawnerBuilder};
use crate::objects::entity_spawner::{EntitySetSpawner, EntitySpawner, EntitySpawnerArg};
use crate::scene::{
    ArenaExtension, ObjectHandle, ObjectPartHandle, Scene, SceneObject, SceneObjectPart,
    SceneObjectPartHandle,
};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg32;
use std::collections::HashMap;

pub type BevyMaterial = StandardMaterial;

pub type InstancedMaterials = HashMap<Point3<usize>, Handle<BevyMaterial>>;
// pub const SELECTED_OBJECT_MATERIAL_KEY: Point3<usize> = point![42, 42, 42];

#[derive(Event)]
pub(crate) struct ResetWorldGraphicsEvent;

pub(crate) fn plugin(app: &mut App) {
    app.add_event::<ResetWorldGraphicsEvent>().add_systems(
        Update,
        reset_world_graphics_event.run_if(on_event::<ResetWorldGraphicsEvent>()),
    );
}

fn reset_world_graphics_event(
    mut commands: Commands,
    mut plugins: ResMut<Plugins>,
    mut graphics: ResMut<GraphicsManager>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BevyMaterial>>,
    mut harness: ResMut<Harness>,
    mut gfx_components: Query<&mut Transform>,
    // mut event: EventReader<ResetWorldGraphicsEvent>,
) {
    {
        for (handle, _) in harness.physics.bodies.iter() {
            let obj_handle =
                graphics
                    .scene
                    .insert_object_part(SceneObjectPart::CollidableWithPhysics {
                        colliders: Vec::new(),
                        body: handle,
                    });

            graphics.add_body_colliders(
                &mut commands,
                &mut meshes,
                &mut materials,
                obj_handle,
                &harness.physics.bodies,
                &harness.physics.colliders,
            );
        }

        // for (handle, _) in harness.physics.colliders.iter() {
        //     graphics.add_collider(
        //         &mut commands,
        //         &mut meshes,
        //         &mut materials,
        //         handle,
        //         &harness.physics.colliders,
        //     );
        // }

        let graphics = graphics.into_inner();
        let physics = &mut harness.physics;

        let pending_entity_spawners = &mut graphics.pending_entity_spawners;

        for mut spawner in pending_entity_spawners.drain(..) {
            let arg = EntitySpawnerArg {
                commands: &mut commands,
                meshes: &mut meshes,
                materials: &mut materials,
                bodies: &mut physics.bodies,
                colliders: &mut physics.colliders,
                impulse_joints: &mut physics.impulse_joints,
                multibody_joints: &mut physics.multibody_joints,
                prefab_meshes: &mut graphics.prefab_meshes,
                instanced_materials: &mut graphics.instanced_materials,
            };
            for (handle, mut new_nodes) in spawner.spawn_entities_sets(arg) {
                let nodes = if let Some(c) = graphics.scene.get_mut_by_body_handle(handle) {
                    c
                } else {
                    graphics
                        .scene
                        .insert_new_object_part_as_collidable_with_physics(handle)
                }
                .get_entities_mut()
                .expect("Should have colliders as we were just inserting rigid body");

                nodes.append(&mut new_nodes);
            }
        }

        for plugin in &mut plugins.0 {
            plugin.init_graphics(
                graphics,
                &mut commands,
                &mut meshes,
                &mut materials,
                &mut gfx_components,
                &mut harness,
            );
        }
    }
}

// pub trait SceneObject {
//     fn build_harness(
//         self,
//         _: RigidBodySet,
//         _: ColliderSet,
//         _: ImpulseJointSet,
//         _: MultibodyJointSet,
//     );
// }

// pub struct Ball {
//     pub radius: f32,
// }

// impl SceneObject for Ball {
//     fn build_harness(
//         self,
//         mut bodies: RigidBodySet,
//         mut colliders: ColliderSet,
//         impulse_joints: ImpulseJointSet,
//         multibody_joints: MultibodyJointSet,
//     ) {
//         let rigid_body = RigidBodyBuilder::dynamic().translation(vector![0.0, 10.0, 0.0]);
//         let handle = bodies.insert(rigid_body);
//         let collider = ColliderBuilder::ball(self.radius);
//         colliders.insert_with_parent(collider, handle, &mut bodies);
//     }
// }

/// The unique handle of a rigid body added to a `RigidBodySet`.

#[derive(Resource)]
pub struct GraphicsManager {
    rand: Pcg32,
    pub scene: Scene,
    // b2sn: HashMap<RigidBodyHandle, Vec<EntityWithGraphics>>,
    b2color: HashMap<RigidBodyHandle, Point3<f32>>,
    pub prefab_meshes: HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: InstancedMaterials,
    pub gfx_shift: Vector<Real>,
    pub pending_entity_spawners: Vec<Box<dyn EntitySetSpawner + 'static>>,
}

impl GraphicsManager {
    pub fn new() -> GraphicsManager {
        GraphicsManager {
            rand: Pcg32::seed_from_u64(0),
            scene: Scene::default(),
            // b2sn: HashMap::new(),
            b2color: HashMap::new(),
            prefab_meshes: HashMap::new(),
            instanced_materials: HashMap::new(),
            gfx_shift: Vector::zeros(),
            pending_entity_spawners: Vec::new(),
        }
    }

    // pub fn selection_material(&self) -> Handle<BevyMaterial> {
    //     self.instanced_materials[&SELECTED_OBJECT_MATERIAL_KEY].clone_weak()
    // }

    pub fn clear(&mut self, commands: &mut Commands) {
        for sns in self.scene.iter_object_part_mut() {
            for sn in sns.iter_mut() {
                sn.despawn(commands);
            }
        }

        self.instanced_materials.clear();
        self.scene.clear();
        self.b2color.clear();
        self.rand = Pcg32::seed_from_u64(0);
    }

    pub fn remove_collider_nodes(
        &mut self,
        commands: &mut Commands,
        handle: Option<ObjectHandle>,
        collider: ColliderHandle,
    ) {
        let handle = handle.unwrap_or(ObjectHandle::invalid());
        if let Some(sns) = self.scene.get_mut(handle) {
            for sn in sns.iter_all_entities_mut() {
                sn.visit_node_mut(&mut |node| {
                    if node.collider == Some(collider) {
                        node.despawn(commands);
                    }
                });
            }
        }
    }

    pub fn remove_object(&mut self, commands: &mut Commands, handle: ObjectHandle) {
        if let Some(sns) = self.scene.get_mut(handle) {
            for sn in sns.iter_all_entities_mut() {
                sn.visit_node_with_entity(&mut |_, entity| {
                    commands.entity(entity).despawn();
                });
            }
        }

        self.scene.remove(handle);
    }

    pub fn remove_object_part(&mut self, commands: &mut Commands, handle: SceneObjectPartHandle) {
        if let Some(sns) = self.scene.get_mut(handle.object_handle) {
            if let Some(part) = sns.get_mut(handle.part_handle) {
                for sn in part.iter_mut() {
                    sn.visit_node_with_entity(&mut |_, entity| {
                        commands.entity(entity).despawn();
                    });
                }
            }
        }
    }

    pub fn set_body_color(
        &mut self,
        materials: &mut Assets<BevyMaterial>,
        b: RigidBodyHandle,
        color: [f32; 3],
    ) {
        self.b2color.insert(b, color.into());

        if let Some(ns) = self.scene.get_mut_by_body_handle(b) {
            for n in ns.iter_mut() {
                n.set_color(materials, color.into())
            }
        }
    }

    pub fn set_initial_body_color(&mut self, b: RigidBodyHandle, color: [f32; 3]) {
        self.b2color.insert(b, color.into());
    }

    pub fn next_color(&mut self) -> Point3<f32> {
        Self::gen_color(&mut self.rand)
    }

    fn gen_color(rng: &mut Pcg32) -> Point3<f32> {
        let mut color: Point3<f32> = rng.gen();

        // Quantize the colors a bit to get some amount of auto-instancing from bevy.
        color.x = (color.x * 5.0).round() / 5.0;
        color.y = (color.y * 5.0).round() / 5.0;
        color.z = (color.z * 5.0).round() / 5.0;
        color
    }

    fn alloc_color(
        &mut self,
        materials: &mut Assets<BevyMaterial>,
        handle: RigidBodyHandle,
        is_fixed: bool,
    ) -> Point3<f32> {
        let color = if is_fixed {
            DEFAULT_COLOR
        } else {
            match self.b2color.get(&handle).cloned() {
                Some(c) => c,
                None => Self::gen_color(&mut self.rand),
            }
        };

        self.set_body_color(materials, handle, color.into());

        color
    }

    /// assign a body to some colour, with collider as shape
    pub fn add_body_colliders(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        handle: SceneObjectPartHandle,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
    ) {
        let b_handle = self.scene.get_body_handle(handle).unwrap();
        let body = bodies.get(b_handle).unwrap();

        let color = self
            .b2color
            .get(&b_handle)
            .copied()
            .unwrap_or_else(|| self.alloc_color(materials, b_handle, !body.is_dynamic()));

        // let _ = self.add_body_colliders_with_color(
        //     commands, meshes, materials, handle, bodies, colliders, color,
        // );

        ////////////////////////
        // create a new node with color
        let mut new_nodes = Vec::new();

        for collider_handle in bodies[b_handle].colliders() {
            let collider = &colliders[*collider_handle];

            let mut spawner = ColliderAsMeshSpawner {
                handle: Some(*collider_handle),
                collider,
                prefab_meshes: &mut self.prefab_meshes,
                instanced_materials: &mut self.instanced_materials,
                delta: Isometry::identity(),
                color,
            };

            new_nodes.push(spawner.spawn(commands, meshes, materials));
        }

        // new_nodes
        //     .iter_mut()
        //     .for_each(|n| n.update(colliders, components, &self.gfx_shift));

        let nodes = self
            .scene
            .get_part_mut(handle)
            .expect("caller should have ensured part exists")
            .get_entities_mut()
            .expect("Should have colliders as we were just inserting rigid body");

        nodes.append(&mut new_nodes);
    }

    // /// assign a body to some colour, with collider as shape
    // pub fn add_body_colliders_from_spawner(
    //     &mut self,
    //     commands: &mut Commands,
    //     meshes: &mut Assets<Mesh>,
    //     materials: &mut Assets<BevyMaterial>,
    //     handle: RigidBodyHandle,
    //     mut spawner: impl EntitySpawner,
    // ) {
    //     ////////////////////////
    //     // create a new node with color
    //     let mut new_nodes = Vec::new();

    //     new_nodes.push(spawner.spawn(commands, meshes, materials));

    //     let nodes = if let Some(c) = self.scene.get_mut_by_body_handle(b_handle) {
    //         c
    //     } else {
    //         self.scene
    //             .insert_new_object_part_as_collidable_with_physics(handle)
    //     }
    //     .get_entities_mut()
    //     .expect("Should have colliders as we were just inserting rigid body");
    //     nodes.append(&mut new_nodes);
    // }

    /// add a new collider to an existing body
    pub fn add_collider(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        handle: ColliderHandle,
        colliders: &ColliderSet,
    ) {
        // panic!("not supported");
        let collider = &colliders[handle];
        let collider_parent = collider.parent().unwrap_or(RigidBodyHandle::invalid());

        let color = self
            .b2color
            .get(&collider_parent)
            .copied()
            .unwrap_or(DEFAULT_COLOR);

        let mut spawner = ColliderAsMeshSpawnerBuilder::default()
            .handle(Some(handle))
            .collider(collider)
            .prefab_meshes(&mut self.prefab_meshes)
            .instanced_materials(&mut self.instanced_materials)
            .color(color)
            .build()
            .unwrap();

        let nodes = if let Some(c) = self.scene.get_mut_by_body_handle(collider_parent) {
            c
        } else {
            self.scene
                .insert_new_object_part_as_collidable_with_physics(collider_parent)
        }
        .get_entities_mut()
        .expect("Should have colliders as we were just inserting rigid body");

        nodes.push(spawner.spawn(commands, meshes, materials));
    }

    /// add a shape as visual to the scene
    // fn add_shape(
    //     &mut self,
    //     commands: &mut Commands,
    //     meshes: &mut Assets<Mesh>,
    //     materials: &mut Assets<BevyMaterial>,
    //     handle: Option<ColliderHandle>,
    //     shape: &dyn Shape,
    //     sensor: bool,
    //     pos: &Isometry<Real>,
    //     delta: &Isometry<Real>,
    //     color: Point3<f32>,
    // ) -> EntityWithGraphics {

    //     let mut spawner = ColliderAsMeshSpawner {
    //         handle,
    //         shape,
    //         sensor,
    //         prefab_meshes: &mut self.prefab_meshes,
    //         instanced_materials: &mut self.instanced_materials,
    //         delta,
    //         color,
    //         pos,
    //     };

    //     spawner.spawn(commands, meshes, materials)
    // }

    pub fn add_shape_by_spawner() {}

    pub fn draw(
        &mut self,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
        components: &mut Query<&mut Transform>,
        _materials: &mut Assets<BevyMaterial>,
    ) {
        for n in self.scene.iter_all_entities_mut() {
            // if let Some(bo) = n
            //     .collider
            //     .and_then(|h| bodies.get(colliders.get(h)?.parent()?))
            // {
            //     if bo.activation().time_since_can_sleep
            //         >= RigidBodyActivation::default_time_until_sleep()
            //     {
            //         n.set_color(materials, point![1.0, 0.0, 0.0]);
            //     }
            //     /* else if bo.activation().energy < bo.activation().threshold {
            //         n.set_color(materials, point![0.0, 0.0, 1.0]);
            //     } */
            //     else {
            //         n.set_color(materials, point![0.0, 1.0, 0.0]);
            //     }
            // }

            n.update(colliders, components, &self.gfx_shift);
            // n.update(colliders, components, &self.gfx_shift);
        }
    }

    // pub fn draw_positions(&mut self, window: &mut Window, rbs: &RigidBodies<f32>) {
    //     for (_, ns) in self.b2sn.iter_mut() {
    //         for n in ns.iter_mut() {
    //             let object = n.object();
    //             let rb = rbs.get(object).expect("Rigid body not found.");

    //             // if let WorldObjectBorrowed::RigidBody(rb) = object {
    //                 let t      = rb.position();
    //                 let center = rb.center_of_mass();

    //                 let rotmat = t.rotation.to_rotation_matrix().unwrap();
    //                 let x = rotmat.column(0) * 0.25f32;
    //                 let y = rotmat.column(1) * 0.25f32;
    //                 let z = rotmat.column(2) * 0.25f32;

    //                 window.draw_line(center, &(*center + x), &point![1.0, 0.0, 0.0]);
    //                 window.draw_line(center, &(*center + y), &point![0.0, 1.0, 0.0]);
    //                 window.draw_line(center, &(*center + z), &point![0.0, 0.0, 1.0]);
    //             // }
    //         }
    //     }
    // }

    // pub fn body_nodes(&self, handle: RigidBodyHandle) -> Option<&Vec<EntityWithGraphics>> {
    //     self.b2sn.get(&handle)
    // }

    pub fn body_nodes_mut(
        &mut self,
        handle: RigidBodyHandle,
    ) -> Option<&mut Vec<EntityWithGraphics>> {
        self.scene
            .get_mut_by_body_handle(handle)
            .map(|p| p.get_entities_mut().unwrap())
    }

    pub fn nodes(&self) -> impl Iterator<Item = &EntityWithGraphics> {
        self.scene.iter_all_entities()
    }

    pub fn nodes_mut(&mut self) -> impl Iterator<Item = &mut EntityWithGraphics> {
        self.scene.iter_all_entities_mut()
    }

    pub fn prefab_meshes(&self) -> &HashMap<ShapeType, Handle<Mesh>> {
        &self.prefab_meshes
    }

    pub fn prefab_meshes_mut(&mut self) -> &mut HashMap<ShapeType, Handle<Mesh>> {
        &mut self.prefab_meshes
    }
}

impl Default for GraphicsManager {
    fn default() -> Self {
        Self::new()
    }
}
