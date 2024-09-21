use bevy::prelude::*;

use crate::dimensify::{clear, DimensifyActionFlags, Plugins};
use crate::graphics::ResetWorldGraphicsEvent;
use crate::physics::{DeserializedPhysicsSnapshot, PhysicsEvents, PhysicsSnapshot, PhysicsState};
use crate::{graphics::GraphicsManager, harness::RunState};
use crate::{mouse, ui, DimensifyState};

use na;
use rapier3d::pipeline::QueryPipeline;

use crate::harness::Harness;

#[derive(Event)]
pub(crate) enum SnapshotEvent {
    TakeSnapshot,
    RestoreSnapshot,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_event::<SnapshotEvent>()
        .add_systems(Update, snapshot_event.run_if(on_event::<SnapshotEvent>()));
}

fn snapshot_event(
    mut commands: Commands,
    mut state: ResMut<DimensifyState>,
    mut harness: ResMut<Harness>,
    mut graphics: ResMut<GraphicsManager>,
    mut plugins: ResMut<Plugins>,
    mut snapshot_event: EventReader<SnapshotEvent>,
    mut reset_graphic_event: EventWriter<ResetWorldGraphicsEvent>,
) {
    for event in snapshot_event.read() {
        match event {
            SnapshotEvent::TakeSnapshot => {
                state.snapshot = PhysicsSnapshot::new(
                    harness.state.timestep_id,
                    &harness.physics.broad_phase,
                    &harness.physics.narrow_phase,
                    &harness.physics.islands,
                    &harness.physics.bodies,
                    &harness.physics.colliders,
                    &harness.physics.impulse_joints,
                    &harness.physics.multibody_joints,
                )
                .ok();

                if let Some(snap) = &state.snapshot {
                    snap.print_snapshot_len();
                }
            }
            SnapshotEvent::RestoreSnapshot => {
                if let Some(snapshot) = &state.snapshot {
                    if let Ok(DeserializedPhysicsSnapshot {
                        timestep_id,
                        broad_phase,
                        narrow_phase,
                        island_manager,
                        bodies,
                        colliders,
                        impulse_joints,
                        multibody_joints,
                    }) = snapshot.restore()
                    {
                        clear(&mut commands, &mut graphics, &mut plugins);

                        for plugin in &mut plugins.0 {
                            plugin.clear_graphics(&mut graphics, &mut commands);
                        }

                        harness.state.timestep_id = timestep_id;
                        harness.physics.broad_phase = broad_phase;
                        harness.physics.narrow_phase = narrow_phase;
                        harness.physics.islands = island_manager;
                        harness.physics.bodies = bodies;
                        harness.physics.colliders = colliders;
                        harness.physics.impulse_joints = impulse_joints;
                        harness.physics.multibody_joints = multibody_joints;
                        harness.physics.query_pipeline = QueryPipeline::new();

                        reset_graphic_event.send(ResetWorldGraphicsEvent);
                    }
                }
            }
        }
    }
}
