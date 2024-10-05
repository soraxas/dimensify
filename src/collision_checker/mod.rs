mod checker;

pub use checker::SimpleCollisionPipeline;
use rapier3d::prelude::{
    ActiveCollisionTypes, ActiveEvents, ColliderBuilder, Group, InteractionGroups,
};


pub mod bevy_rapier_helpers;
pub mod rapier_helpers;