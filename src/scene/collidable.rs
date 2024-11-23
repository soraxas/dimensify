use bevy::{ecs::system::SystemParam, prelude::*, utils::HashSet};
use bevy_rapier3d::prelude::*;
use rapier3d::prelude::{PairFilterContext, PhysicsHooks};

/// store the entities that are ignored for collision detection (for this entity)
#[derive(Debug, Component, Default, Clone, Reflect)]
pub struct IgnoredColliders {
    pub ignored_entities: HashSet<Entity>,
}
impl IgnoredColliders {
    pub fn add(&mut self, entity: Entity) {
        self.ignored_entities.insert(entity);
    }
    pub fn with(mut self, entity: Option<Entity>) -> Self {
        if let Some(entity) = entity {
            self.ignored_entities.insert(entity);
        }
        self
    }
}

#[derive(SystemParam)]
pub struct IgnoredCollidersFilter<'w, 's> {
    filters: Query<'w, 's, &'static IgnoredColliders>,
}

impl BevyPhysicsHooks for IgnoredCollidersFilter<'_, '_> {
    fn filter_contact_pair(&self, context: PairFilterContextView) -> Option<SolverFlags> {
        let entity1 = context.collider1();
        let entity2 = context.collider2();

        if let Ok(filter) = self.filters.get(entity1) {
            if filter.ignored_entities.contains(&entity2) {
                return None;
            }
        }

        if let Ok(filter) = self.filters.get(entity2) {
            if filter.ignored_entities.contains(&entity1) {
                return None;
            }
        }

        Some(SolverFlags::COMPUTE_IMPULSES)
    }
}

impl PhysicsHooks for IgnoredCollidersFilter<'_, '_> {
    fn filter_contact_pair(&self, context: &PairFilterContext) -> Option<SolverFlags> {
        // use the BevyPhysicsHooks implementation

        <Self as BevyPhysicsHooks>::filter_contact_pair(
            self,
            PairFilterContextView { raw: context },
        )
    }
}
