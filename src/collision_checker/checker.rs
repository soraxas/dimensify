use rapier3d::prelude::{
    ActiveCollisionTypes, ActiveEvents, BroadPhaseMultiSap, ColliderBuilder, ColliderSet,
    CollisionPipeline, IntegrationParameters, IslandManager, NarrowPhase, QueryPipeline,
    RigidBodySet,
};

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
                }
            });
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
