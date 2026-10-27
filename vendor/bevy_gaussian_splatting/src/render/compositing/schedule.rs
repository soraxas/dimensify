//! Keep the ordinary pass's function-set identity when installing its fallback.

use bevy::{
    ecs::{
        schedule::ScheduleCleanupPolicy,
        system::{CombinatorSystem, Combine, RunSystemError, System},
    },
    prelude::*,
};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct CompositingPass;

struct TryEncoded;

impl<A: System<In = (), Out = bool>, B: System<In = (), Out = ()>> Combine<A, B> for TryEncoded {
    type In = ();
    type Out = ();

    fn combine<T>(
        _: (),
        data: &mut T,
        encoded: impl FnOnce((), &mut T) -> Result<bool, RunSystemError>,
        ordinary: impl FnOnce((), &mut T) -> Result<(), RunSystemError>,
    ) -> Result<(), RunSystemError> {
        match encoded((), data) {
            Ok(true) => Ok(()),
            Ok(false) | Err(RunSystemError::Skipped(_)) => ordinary((), data),
            Err(error) => Err(error),
        }
    }
}

pub(super) fn replace_pass<A, B, MA, MB>(
    schedule: &mut Schedule,
    world: &mut World,
    ordinary: A,
    encoded: B,
) -> Result<(), String>
where
    A: IntoSystem<(), (), MA>,
    B: IntoSystem<(), bool, MB>,
{
    let ordinary = IntoSystem::into_system(ordinary);
    let sets = ordinary.default_system_sets();
    let Some(set) = sets.first().copied() else {
        return Err("ordinary transparent pass has no function set".into());
    };
    let removed = schedule
        .remove_systems_in_set(set, world, ScheduleCleanupPolicy::RemoveSystemsOnly)
        .map_err(|error| format!("cannot replace transparent pass: {error}"))?;
    if removed == 0 {
        return Err("ordinary transparent pass is not installed".into());
    }
    // CombinatorSystem retains both inner function sets. Existing and later
    // `.before(ordinary)`/`.after(ordinary)` constraints therefore enclose the
    // complete encoded-or-fallback operation, not an empty anchor.
    schedule.add_systems(
        CombinatorSystem::<TryEncoded, _, _>::new(
            IntoSystem::into_system(encoded),
            ordinary,
            "Gaussian encoded-or-linear transparency".into(),
        )
        .in_set(CompositingPass),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Trace(Vec<&'static str>);
    #[derive(Resource)]
    struct Ready(bool);

    fn opaque(mut trace: ResMut<Trace>) {
        trace.0.push("opaque");
    }
    fn ordinary(mut trace: ResMut<Trace>) {
        trace.0.push("ordinary");
    }
    fn encoded(ready: Res<Ready>, mut trace: ResMut<Trace>) -> bool {
        trace.0.push("attempt");
        if ready.0 {
            trace.0.push("encoded");
        }
        ready.0
    }
    fn transmission(mut trace: ResMut<Trace>) {
        trace.0.push("transmission");
    }
    fn late_plugin(mut trace: ResMut<Trace>) {
        trace.0.push("late plugin");
    }
    fn postprocess(mut trace: ResMut<Trace>) {
        trace.0.push("postprocess");
    }

    #[test]
    fn replacement_preserves_function_ordering_and_draws_exactly_one_pass() {
        for ready in [false, true] {
            let mut world = World::new();
            world.init_resource::<Trace>();
            world.insert_resource(Ready(ready));
            let mut schedule = Schedule::default();
            schedule.add_systems((opaque, ordinary).chain());
            schedule.add_systems(transmission.after(opaque).before(ordinary));
            schedule.add_systems(postprocess.after(ordinary));
            replace_pass(&mut schedule, &mut world, ordinary, encoded).expect("install pass");
            // Restore the core pass's direct opaque->transparent chain edge.
            schedule.configure_sets(CompositingPass.after(opaque));
            // Dependencies registered after installation must still target the
            // whole pass, not only the fallback or an empty function set.
            schedule.add_systems(late_plugin.after(transmission).before(ordinary));
            schedule.run(&mut world);
            assert_eq!(
                world.resource::<Trace>().0,
                vec![
                    "opaque",
                    "transmission",
                    "late plugin",
                    "attempt",
                    if ready { "encoded" } else { "ordinary" },
                    "postprocess",
                ]
            );
        }
    }

    #[test]
    fn empty_function_set_does_not_install_an_unowned_pass() {
        let mut world = World::new();
        world.init_resource::<Trace>();
        world.insert_resource(Ready(true));
        let mut schedule = Schedule::default();
        // Another plugin can mention the function without registering it.
        schedule.add_systems(opaque.before(ordinary));
        assert!(replace_pass(&mut schedule, &mut world, ordinary, encoded).is_err());
        schedule.run(&mut world);
        assert_eq!(world.resource::<Trace>().0, ["opaque"]);
    }
}
