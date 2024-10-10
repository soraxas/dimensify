pub mod checker;

use bevy::app::App;
pub use checker::SimpleCollisionPipeline;
use checker::TmpRapierPhrase;
use rapier3d::prelude::{
    ActiveCollisionTypes, ActiveEvents, ColliderBuilder, Group, InteractionGroups,
};

pub mod rapier_helpers;

pub fn plugin(app: &mut App) {
    app.init_resource::<TmpRapierPhrase>();
}
