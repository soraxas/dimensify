use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_rapier3d::plugin::RapierContext;
use rapier3d::prelude::*;

use crate::scene::collidable::IgnoredCollidersFilter;

// use bevy_rapier3d::prelude::{ActiveCollisionTypes, ActiveEvents, ColliderBuilder, ColliderSet, ColliderHandle, RigidBodySet, IslandManager, BroadPhaseMultiSap, NarrowPhase, QueryPipeline, IntegrationParameters, CollisionPipeline};

#[derive(Default)]
pub struct SimpleCollisionPipeline {
    pub collider_set: ColliderSet,

    pub query_pipeline: QueryPipeline,

    /// we don't use these (but we need them for various calls). Propose simplifying to not need them.
    pub rigid_body_set: RigidBodySet,
    pub island_manager: IslandManager, // awkwardly required for ColliderSet::remove

    pub integration_parameters: IntegrationParameters,

    collision_pipeline: CollisionPipeline,

    broad_phase: BroadPhaseMultiSap,
    pub narrow_phase: NarrowPhase,
}

/// System responsible for advancing the physics simulation, and updating the internal state
/// for scene queries.
// pub fn step_simulation<Hooks>(
//     mut context: ResMut<RapierContext>,
//     config: Res<RapierConfiguration>,
//     hooks: StaticSystemParam<Hooks>,
//     time: Res<Time>,
//     mut sim_to_render_time: ResMut<SimulationToRenderTime>,
//     collision_events: EventWriter<CollisionEvent>,
//     contact_force_events: EventWriter<ContactForceEvent>,
//     interpolation_query: Query<(&RapierRigidBodyHandle, &mut TransformInterpolation)>,
// ) where
//     Hooks: 'static + BevyPhysicsHooks,
//     for<'w, 's> SystemParamItem<'w, 's, Hooks>: BevyPhysicsHooks,
// {
//     let context = &mut *context;
// let hooks_adapter = BevyPhysicsHooksAdapter::new(hooks.into_inner());

//     if config.physics_pipeline_active {
//         context.step_simulation(
//             config.gravity,
//             config.timestep_mode,
//             Some((collision_events, contact_force_events)),
//             &hooks_adapter,
//             &time,
//             &mut sim_to_render_time,
//             Some(interpolation_query),
//         );
//         context.deleted_colliders.clear();
//     } else {
//         context.propagate_modified_body_positions_to_colliders();
//     }

//     if config.query_pipeline_active {
//         context.update_query_pipeline();
//     }
// }

#[derive(Resource, Default)]
pub(crate) struct TmpRapierPhrase {
    broad_phase: BroadPhaseMultiSap,
    narrow_phase: NarrowPhase,
}

/// System responsible for using the collision pipeline from
/// bevy_rapier context to detect collisions.
#[derive(SystemParam)]
pub struct CollisionDetectorFromBevyRapierContext<'w, 's> {
    context: ResMut<'w, RapierContext>,
    filter_hook: IgnoredCollidersFilter<'w, 's>,
    tmp_rapier_phrase: ResMut<'w, TmpRapierPhrase>,
}

impl CollisionDetectorFromBevyRapierContext<'_, '_> {
    // Mostly used to avoid borrowing self completely.
    pub fn collider_entity(&self, handle: ColliderHandle) -> Option<Entity> {
        let context = self.context.as_ref();

        context
            .colliders
            .get(handle)
            .map(|c| Entity::from_bits(c.user_data as u64))
    }

    /// Returns a list of pairs of entities that are colliding.
    pub fn get_colliding_entity(&mut self) -> Vec<(Entity, Entity)> {
        self.get_colliding_interactions()
            .map(|pair| {
                (
                    self.collider_entity(pair.collider1)
                        .expect("should keep track of the corresponding entity"),
                    self.collider_entity(pair.collider2)
                        .expect("should keep track of the corresponding entity"),
                )
            })
            .collect()
    }
}

impl CollisionChecker for CollisionDetectorFromBevyRapierContext<'_, '_> {
    fn update_detect(&mut self) {
        let context = self.context.as_mut();
        let tmp_rapier_phrase = self.tmp_rapier_phrase.as_mut();

        let mut collision_pipeline = CollisionPipeline::default();
        collision_pipeline.step(
            context.integration_parameters.prediction_distance(),
            // PREDICTION_DISTANCE, // would prefer IntegrationParameters::DEFAULT_PREDICTION_DISTANCE
            &mut tmp_rapier_phrase.broad_phase,
            &mut tmp_rapier_phrase.narrow_phase,
            &mut context.bodies,
            &mut context.colliders,
            // &mut context.rigid_body_set,
            // &mut context.collider_set,
            None,
            // Some(&mut self.query_pipeline),
            &self.filter_hook,
            // &(),
            &(),
        );
    }

    fn get_narrow_phase(&self) -> &NarrowPhase {
        &self.tmp_rapier_phrase.narrow_phase
    }
}

pub trait CollisionChecker {
    fn update_detect(&mut self);

    fn get_narrow_phase(&self) -> &NarrowPhase;

    #[inline(always)]
    fn get_contact_interactions(&self) -> impl Iterator<Item = &ContactPair> {
        self.get_narrow_phase().contact_graph().interactions()
    }

    #[inline(always)]
    fn get_colliding_interactions(&self) -> impl Iterator<Item = &ContactPair> {
        self.get_contact_interactions()
            .filter(|pair| pair.has_any_active_contact)
    }

    fn has_collision(&self) -> bool {
        self.get_contact_interactions()
            .any(|pair| pair.has_any_active_contact)
    }

    fn print_collision_info(&self) {
        self.get_contact_interactions().for_each(|pair| {
            if let Some(contact) = pair.find_deepest_contact() {
                dbg!(contact);
                dbg!(pair.collider1, pair.collider2);
            }
        });
    }

    // fn get_colliding_pairs(&self) -> Vec<(ColliderHandle, ColliderHandle)> {
    //     self.get_contact_interactions()
    //         .filter_map(|pair| {
    //             dbg!("heee");
    //             pair.find_deepest_contact().map(|_contact| {
    //                 dbg!(_contact);
    //                 (pair.collider1, pair.collider2)
    //             })
    //         })
    //         .collect()
    // }
}

impl SimpleCollisionPipeline {
    pub fn update(&mut self) {
        self.collision_pipeline.step(
            self.integration_parameters.prediction_distance(),
            // PREDICTION_DISTANCE, // would prefer IntegrationParameters::DEFAULT_PREDICTION_DISTANCE
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );
    }

    pub fn has_collision(&self) -> bool {
        self.narrow_phase
            .contact_graph()
            .interactions()
            .any(|pair| pair.has_any_active_contact)
    }

    pub fn print_collision_info(&self) {
        self.narrow_phase
            .contact_graph()
            .interactions()
            .for_each(|pair| {
                if let Some(contact) = pair.find_deepest_contact() {
                    dbg!(contact);
                    dbg!(pair.collider1, pair.collider2);
                }
            });
    }

    pub fn get_colliding_pairs(&self) -> Vec<(ColliderHandle, ColliderHandle)> {
        self.narrow_phase
            .contact_graph()
            .interactions()
            .filter_map(|pair| {
                dbg!("heee");
                pair.find_deepest_contact().map(|_contact| {
                    dbg!(_contact);
                    (pair.collider1, pair.collider2)
                })
            })
            .collect()
    }
}

fn aaa() {
    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    /* Create the ground. */
    let collider_a = ColliderBuilder::cuboid(1.0, 1.0, 1.0)
        .active_collision_types(ActiveCollisionTypes::all())
        .sensor(true)
        .active_events(ActiveEvents::COLLISION_EVENTS)
        .build();

    let a_handle = collider_set.insert(collider_a);

    let collider_b = ColliderBuilder::cuboid(1.0, 1.0, 1.0)
        .active_collision_types(ActiveCollisionTypes::all())
        .sensor(true)
        .active_events(ActiveEvents::COLLISION_EVENTS)
        .build();

    let _ = collider_set.insert(collider_b);

    let integration_parameters = IntegrationParameters::default();
    let mut broad_phase = BroadPhaseMultiSap::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut collision_pipeline = CollisionPipeline::new();
    let physics_hooks = ();

    collision_pipeline.step(
        integration_parameters.prediction_distance(),
        &mut broad_phase,
        &mut narrow_phase,
        &mut rigid_body_set,
        &mut collider_set,
        None,
        &physics_hooks,
        &(),
    );

    let mut hit = false;

    for (_, _, intersecting) in narrow_phase.intersection_pairs_with(a_handle) {
        if intersecting {
            hit = true;
        }
    }

    assert!(hit, "No hit found");
}
