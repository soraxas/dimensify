//! Replacement admission must not wait for whole-plan optimizer convergence.
use super::*;

#[test]
fn pressure_policy_publishes_a_covered_replacement_on_the_due_tick() {
    let (mut app, root, camera) = stream_app(SogLodSettings {
        splat_budget: 10,
        cache_bytes: 64 * chunk::DecodedChunk::bytes_per_splat(),
        ..default()
    });
    app.world_mut()
        .entity_mut(root)
        .insert((relocation_stream(), GlobalTransform::IDENTITY));
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(-9., 0., 0.)));
    let old = activate_snapshot(&mut app, root);
    app.world_mut()
        .entity_mut(camera)
        .insert(GlobalTransform::from_translation(Vec3::new(9., 0., 0.)));

    advance_stream(&mut app);

    let stream = app.world().get::<SogStream>(root).unwrap();
    assert_eq!(
        stream
            .submitted
            .iter()
            .map(|range| range.file)
            .collect::<Vec<_>>(),
        [0, 0],
        "an admissible covered replacement must not wait for another optimizer pass"
    );
    assert!(
        stream.replacements.wants(2),
        "the accepted target owns progress"
    );
    assert_eq!(
        stream
            .displayed
            .iter()
            .map(|range| range.file)
            .collect::<Vec<_>>(),
        [1, 0],
        "submission must not retire the visible generation"
    );
    assert_eq!(
        app.world().get::<Visibility>(old),
        Some(&Visibility::Inherited)
    );
    assert!(
        stream.entries[2].loading.is_none(),
        "old owners still consume headroom"
    );
}
