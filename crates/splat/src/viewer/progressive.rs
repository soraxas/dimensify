//! Opt-in progressive PLY selection authority and rollback.
use super::{ViewerOptions, files::LocalFiles};
use crate::progressive::transport::{Download, Event};
use crate::renderer::{progressive::ProgressiveGpu, readiness::ReadySnapshots};
use crate::scene::SourceGeometry;
use crate::{LoadPhase, LoadedSplat, SplatLoaded, SplatScene};
use bevy::prelude::*;

pub(super) struct ProgressivePlyPlugin;
impl Plugin for ProgressivePlyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProgressivePly>()
            .add_systems(Startup, install);
    }
}
// Keep the default whole-file update schedule free of progressive polling.
fn install(world: &mut World) {
    if world.resource::<ViewerOptions>().progressive_ply {
        world.schedule_scope(PostUpdate, |_, schedule| {
            schedule.add_systems(
                poll.after(super::streaming::poll)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
            );
        });
        world.schedule_scope(Update, |_, schedule| {
            schedule.add_systems(
                cancel_superseded
                    .after(super::files::receive)
                    .before(super::streaming::stage)
                    .before(crate::scene::request_load),
            );
        });
    }
}
#[derive(Resource, Default)]
struct ProgressivePly {
    active: Option<Active>,
    // Completed clouds stay owned until the next successful scene replacement.
    retained: Vec<ProgressiveGpu>,
}
struct Active {
    ticket: u64,
    name: String,
    phase_before: LoadPhase,
    generation: u64,
    download: Option<Download>,
    gpu: Option<ProgressiveGpu>,
    count: usize,
    total: usize,
    min: Vec3,
    max: Vec3,
    support: f32,
    previous: Option<LoadedSplat>,
    shown: bool,
    eof: bool,
    complete: bool,
}
pub(super) fn start(world: &mut World, ticket: u64, name: String, url: String) {
    if world.resource::<LocalFiles>().selection() != ticket {
        return;
    }
    world.resource_scope(|world, mut state: Mut<ProgressivePly>| {
        if let Some(mut old) = state.active.take() {
            if old.complete {
                if let Some(gpu) = old.gpu.take() {
                    state.retained.push(gpu);
                }
            } else {
                rollback(world, old);
            }
        }
        let phase_before = match world.resource::<SplatScene>().phase.clone() {
            LoadPhase::Loading(_) => LoadPhase::Failed("Previous load was superseded".into()),
            phase => phase,
        };
        super::streaming::cancel_pending(world);
        world.resource_scope(|world, mut pending: Mut<crate::scene::PendingLoad>| {
            pending.cancel(&mut world.resource_mut::<SplatScene>());
        });
        let limit = world
            .get_resource::<bevy::render::renderer::RenderDevice>()
            .map(|device| device.limits().max_storage_buffer_binding_size);
        world.resource_mut::<SplatScene>().phase = LoadPhase::Loading(name.clone());
        let generation = world.resource::<SplatScene>().generation();
        state.active = Some(Active {
            ticket,
            name,
            phase_before,
            generation,
            download: Some(Download::start(url, limit)),
            gpu: None,
            count: 0,
            total: 0,
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
            support: 0.,
            previous: None,
            shown: false,
            eof: false,
            complete: false,
        });
    });
}
fn rollback(world: &mut World, mut active: Active) {
    let current = world
        .resource::<SplatScene>()
        .loaded
        .as_ref()
        .map(|loaded| loaded.entity);
    let ours = active.gpu.as_ref().map(ProgressiveGpu::entity);
    if active.shown && current == ours {
        if let Some(previous) = active.previous.as_mut() {
            let transform = world.resource::<ViewerOptions>().up.transform();
            if let Some(geometry) = world.get::<SourceGeometry>(previous.entity) {
                previous.bounds = geometry.transformed_bounds(transform);
                world.entity_mut(previous.entity).insert(transform);
            }
        }
        if let Some(previous) = &active.previous
            && let Ok(mut entity) = world.get_entity_mut(previous.entity)
        {
            entity.insert(Visibility::Inherited);
        }
        world.resource_mut::<SplatScene>().loaded = active.previous.take();
    } else if active.shown {
        // A newer successful replacement already owns the scene. Its commit
        // retired our partial; release the hidden rollback copy as well.
        if let Some(previous) = active.previous.take()
            && current != Some(previous.entity)
        {
            crate::scene::retire(world, previous.entity);
        }
    }
    let phase_is_ours = world.resource::<SplatScene>().generation() == active.generation
        && matches!(&world.resource::<SplatScene>().phase, LoadPhase::Loading(name)
            if name == &active.name || name == &format!("{} ({}/{})", active.name,active.count,active.total));
    if phase_is_ours {
        world.resource_mut::<SplatScene>().phase = active.phase_before;
    }
    if let Some(gpu) = active.gpu.take() {
        gpu.retire(world);
    }
}
// Cancel before a newer ordinary load can commit or stage another stream.
fn cancel_superseded(world: &mut World) {
    let ticket = world.resource::<LocalFiles>().selection();
    world.resource_scope(|world, mut state: Mut<ProgressivePly>| {
        if state.active.as_ref().is_some_and(|active| {
            !active.complete
                && (active.ticket != ticket
                    || world.resource::<SplatScene>().generation() != active.generation)
        }) && let Some(active) = state.active.take()
        {
            rollback(world, active);
        }
    });
}
fn poll(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<ProgressivePly>| {
        let mut retained = Vec::new();
        for gpu in state.retained.drain(..) {
            if world.get_entity(gpu.entity()).is_ok() {
                retained.push(gpu);
            } else {
                gpu.retire(world);
            }
        }
        state.retained = retained;
        let Some(mut active) = state.active.take() else {
            return;
        };
        if active.complete {
            if active
                .gpu
                .as_ref()
                .is_some_and(|gpu| world.get_entity(gpu.entity()).is_err())
            {
                if let Some(gpu) = active.gpu.take() {
                    gpu.retire(world);
                }
            } else {
                state.active = Some(active);
            }
            return;
        }
        if world.resource::<LocalFiles>().selection() != active.ticket
            || world.resource::<SplatScene>().generation() != active.generation
        {
            rollback(world, active);
            return;
        }
        let result = advance(world, &mut active);
        match result {
            Ok(true) => state.active = Some(active),
            Ok(false) => {
                rollback(world, active);
            }
            Err(error) => {
                let name = active.name.clone();
                rollback(world, active);
                world.resource_mut::<SplatScene>().phase =
                    LoadPhase::Failed(format!("{name}: {error}"));
            }
        }
    });
}
/// False hands unsupported encodings back to the unchanged whole-file path.
fn advance(world: &mut World, active: &mut Active) -> Result<bool, String> {
    for _ in 0..2 {
        let Some(event) = active.download.as_ref().and_then(Download::pop) else {
            break;
        };
        match event {
            Event::Batch(batch) => {
                if active.gpu.is_none() {
                    active.gpu = Some(ProgressiveGpu::new(world, batch.total)?);
                }
                active.gpu.as_mut().unwrap().append(world, &batch)?;
                active.count += batch.count;
                active.total = batch.total;
                active.min = active.min.min(batch.bounds.0);
                active.max = active.max.max(batch.bounds.1);
                active.support = active.support.max(batch.support);
            }
            Event::Complete => {
                active.eof = true;
                active.download = None;
                break;
            }
            Event::Failed(error) => return Err(error),
            Event::Fallback(bytes) => {
                world.resource::<LocalFiles>().progressive_fallback(
                    active.ticket,
                    active.name.clone(),
                    bytes,
                    world.resource::<ViewerOptions>().up,
                );
                return Ok(false);
            }
        }
    }
    let Some(gpu) = &active.gpu else {
        return Ok(true);
    };
    let entity = gpu.entity();
    let geometry = SourceGeometry {
        bounds: (active.min, active.max),
        support: active.support,
    };
    let transform = world.resource::<ViewerOptions>().up.transform();
    let bounds = geometry.transformed_bounds(transform);
    world.entity_mut(entity).insert((geometry, transform));
    if !active.shown {
        let ready = world
            .get_resource::<ReadySnapshots>()
            .is_some_and(|ready| ready.0.lock().unwrap().contains(&entity));
        if !ready {
            return Ok(true);
        }
        active.previous = world.resource_mut::<SplatScene>().loaded.take();
        if let Some(previous) = &active.previous {
            world.entity_mut(previous.entity).insert(Visibility::Hidden);
        }
        world.entity_mut(entity).insert(Visibility::Inherited);
        active.shown = true;
        world.write_message(SplatLoaded { bounds });
    }
    world.resource_mut::<SplatScene>().loaded = Some(LoadedSplat { entity, bounds });
    if active.eof {
        if active.count != active.total {
            return Err("PLY stream ended before every Gaussian arrived".into());
        }
        if let Some(previous) = active.previous.take() {
            crate::scene::retire(world, previous.entity);
        }
        world.resource_mut::<SplatScene>().phase = LoadPhase::Ready {
            name: active.name.clone(),
            count: active.count,
        };
        active.complete = true;
    } else {
        world.resource_mut::<SplatScene>().phase = LoadPhase::Loading(format!(
            "{} ({}/{})",
            active.name, active.count, active.total
        ));
    }
    Ok(true)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "progressive_tests.rs"]
mod tests;
