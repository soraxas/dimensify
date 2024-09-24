use bevy::prelude::*;

use na::{point, Point3};

use crate::dimensify::Plugins;
use crate::harness::Harness;
use crate::objects::node::{EntitySpawnerArg, EntitySpawnerBlahBlah, EntityWithGraphics};
use rapier3d::dynamics::{RigidBodyHandle, RigidBodySet};
use rapier3d::geometry::{ColliderHandle, ColliderSet, ShapeType};
use rapier3d::math::{Isometry, Real, Vector};
//use crate::objects::capsule::Capsule;
//use crate::objects::plane::Plane;
// use crate::objects::mesh::Mesh;
use crate::objects::entity_spawner::ColliderAsMeshSpawner;
use crate::objects::entity_spawner::EntitySpawner;
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
            graphics.add_body_colliders(
                &mut commands,
                &mut meshes,
                &mut materials,
                handle,
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
            for (handle, mut new_nodes) in spawner.spawn_with_sets(arg) {
                let nodes = graphics.b2sn.entry(handle).or_default();
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

#[derive(Resource)]
pub struct GraphicsManager {
    rand: Pcg32,
    b2sn: HashMap<RigidBodyHandle, Vec<EntityWithGraphics>>,
    b2color: HashMap<RigidBodyHandle, Point3<f32>>,
    b2wireframe: HashMap<RigidBodyHandle, bool>,
    ground_color: Point3<f32>,
    pub prefab_meshes: HashMap<ShapeType, Handle<Mesh>>,
    pub instanced_materials: InstancedMaterials,
    pub gfx_shift: Vector<Real>,
    pub pending_entity_spawners: Vec<Box<dyn EntitySpawnerBlahBlah + 'static>>,
}

impl GraphicsManager {
    pub fn new() -> GraphicsManager {
        GraphicsManager {
            rand: Pcg32::seed_from_u64(0),
            b2sn: HashMap::new(),
            b2color: HashMap::new(),
            ground_color: point![0.5, 0.5, 0.5],
            b2wireframe: HashMap::new(),
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
        for sns in self.b2sn.values_mut() {
            for sn in sns.iter_mut() {
                sn.despawn(commands);
            }
        }

        self.instanced_materials.clear();
        self.b2sn.clear();
        self.b2color.clear();
        self.b2wireframe.clear();
        self.rand = Pcg32::seed_from_u64(0);
    }

    pub fn remove_collider_nodes(
        &mut self,
        commands: &mut Commands,
        body: Option<RigidBodyHandle>,
        collider: ColliderHandle,
    ) {
        let body = body.unwrap_or(RigidBodyHandle::invalid());
        if let Some(sns) = self.b2sn.get_mut(&body) {
            for sn in sns.iter_mut() {
                sn.visit_node_mut(&mut |node| {
                    if node.collider == Some(collider) {
                        node.despawn(commands);
                    }
                });
            }
        }
    }

    pub fn remove_body_nodes(&mut self, commands: &mut Commands, body: RigidBodyHandle) {
        if let Some(sns) = self.b2sn.get_mut(&body) {
            for sn in sns.iter_mut() {
                sn.visit_node_with_entity(&mut |_, entity| {
                    commands.entity(entity).despawn();
                });
            }
        }

        self.b2sn.remove(&body);
    }

    pub fn set_body_color(
        &mut self,
        materials: &mut Assets<BevyMaterial>,
        b: RigidBodyHandle,
        color: [f32; 3],
    ) {
        self.b2color.insert(b, color.into());

        if let Some(ns) = self.b2sn.get_mut(&b) {
            for n in ns.iter_mut() {
                n.set_color(materials, color.into())
            }
        }
    }

    pub fn set_initial_body_color(&mut self, b: RigidBodyHandle, color: [f32; 3]) {
        self.b2color.insert(b, color.into());
    }

    pub fn set_body_wireframe(&mut self, b: RigidBodyHandle, enabled: bool) {
        self.b2wireframe.insert(b, enabled);

        if let Some(_ns) = self.b2sn.get_mut(&b) {
            // for n in ns.iter_mut().filter_map(|n| n.scene_node_mut()) {
            //     if enabled {
            //         n.set_surface_rendering_activation(true);
            //         n.set_lines_width(1.0);
            //     } else {
            //         n.set_surface_rendering_activation(false);
            //         n.set_lines_width(1.0);
            //     }
            // }
        }
    }

    pub fn toggle_wireframe_mode(&mut self, colliders: &ColliderSet, enabled: bool) {

        // for n in self.b2sn.values_mut().flat_map(|val| val.iter_mut()) {

        //     if let Some(entity_collider) = n.collider {
        //         let force_wireframe = if let Some(collider) = colliders.get(entity_collider) {
        //             collider.is_sensor()
        //                 || collider.parent().and_then(|parent| {
        //                     self
        //                         .b2wireframe
        //                         .get(&parent)
        //                         .cloned()
        //                 }).unwrap_or(false)
        //         } else {
        //             false
        //         };

        //         if let Some(node) = n.scene_node_mut() {
        //             if force_wireframe || enabled {
        //                 node.set_lines_width(1.0);
        //                 node.set_surface_rendering_activation(false);
        //             } else {
        //                 node.set_lines_width(0.0);
        //                 node.set_surface_rendering_activation(true);
        //             }
        //         }
        //     }

        // }
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
        let mut color = self.ground_color;

        if !is_fixed {
            match self.b2color.get(&handle).cloned() {
                Some(c) => color = c,
                None => color = Self::gen_color(&mut self.rand),
            }
        }

        self.set_body_color(materials, handle, color.into());

        color
    }

    /// assign a body to some colour, with collider as shape
    pub fn add_body_colliders(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        handle: RigidBodyHandle,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
    ) {
        let body = bodies.get(handle).unwrap();

        let color = self
            .b2color
            .get(&handle)
            .copied()
            .unwrap_or_else(|| self.alloc_color(materials, handle, !body.is_dynamic()));

        // let _ = self.add_body_colliders_with_color(
        //     commands, meshes, materials, handle, bodies, colliders, color,
        // );

        ////////////////////////
        // create a new node with color
        let mut new_nodes = Vec::new();

        for collider_handle in bodies[handle].colliders() {
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

        // for node in new_nodes.iter_mut().filter_map(|n| n.scene_node_mut()) {
        //     if self.b2wireframe.get(&handle).cloned() == Some(true) {
        //         node.set_lines_width(1.0);
        //         node.set_surface_rendering_activation(false);
        //     } else {
        //         node.set_lines_width(0.0);
        //         node.set_surface_rendering_activation(true);
        //     }
        // }

        let nodes = self.b2sn.entry(handle).or_default();
        nodes.append(&mut new_nodes);
    }

    /// assign a body to some colour, with collider as shape
    pub fn add_body_colliders_from_spawner(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        handle: RigidBodyHandle,
        mut spawner: impl EntitySpawner,
    ) {
        ////////////////////////
        // create a new node with color
        let mut new_nodes = Vec::new();

        new_nodes.push(spawner.spawn(commands, meshes, materials));

        let nodes = self.b2sn.entry(handle).or_default();
        nodes.append(&mut new_nodes);
    }

    pub fn replace_body_collider(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        handle: ColliderHandle,
        bodies: &RigidBodySet,
        colliders: &ColliderSet,
    ) {
        todo!("replace_body_collider");
        let collider = &colliders[handle];
        let collider_parent = collider
            .parent()
            .expect("should we always have rigid body parent?");

        if let Some(entities) = self.b2sn.get_mut(&collider_parent) {
            // entities.drain(..).for_each(
            //     |f|
            //     f.collider
            // );
        } else {
            warn!(
                "No graphics for rigid body parent of collider {:?}. No replacing happened.",
                handle
            );
        }

        self.add_body_colliders(
            commands,
            meshes,
            materials,
            collider_parent,
            bodies,
            colliders,
        );
    }

    #[deprecated]
    pub fn add_collider(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<BevyMaterial>,
        handle: ColliderHandle,
        colliders: &ColliderSet,
    ) {
        panic!("not supported");
        // let collider = &colliders[handle];
        // let collider_parent = collider.parent().unwrap_or(RigidBodyHandle::invalid());

        // let color = self.c2color.get(&handle).copied().unwrap_or_else(|| {
        //     let color = self
        //         .b2color
        //         .get(&collider_parent)
        //         .copied()
        //         .unwrap_or(self.ground_color);
        //     color
        // });
        // let mut nodes = std::mem::take(self.b2sn.entry(collider_parent).or_default());
        // nodes.push(self.add_shape(
        //     commands,
        //     meshes,
        //     materials,
        //     Some(handle),
        //     collider.shape(),
        //     collider.is_sensor(),
        //     collider.position(),
        //     &Isometry::identity(),
        //     color,
        // ));
        // self.b2sn.insert(collider_parent, nodes);
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
        for (_, ns) in self.b2sn.iter_mut() {
            for n in ns.iter_mut() {
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

    pub fn body_nodes(&self, handle: RigidBodyHandle) -> Option<&Vec<EntityWithGraphics>> {
        self.b2sn.get(&handle)
    }

    pub fn body_nodes_mut(
        &mut self,
        handle: RigidBodyHandle,
    ) -> Option<&mut Vec<EntityWithGraphics>> {
        self.b2sn.get_mut(&handle)
    }

    pub fn nodes(&self) -> impl Iterator<Item = &EntityWithGraphics> {
        self.b2sn.values().flat_map(|val| val.iter())
    }

    pub fn nodes_mut(&mut self) -> impl Iterator<Item = &mut EntityWithGraphics> {
        self.b2sn.values_mut().flat_map(|val| val.iter_mut())
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
