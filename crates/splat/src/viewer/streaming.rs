//! Hosted manifest composition over the same streaming engine used by applications.
use super::ViewerOptions;
use crate::scene::SourceGeometry;
use crate::streaming::runtime::SogStream;
use crate::{LoadPhase, LoadSplat, LoadedSplat, SplatLoaded, SplatScene};
use bevy::prelude::*;

#[derive(Message)]
pub(super) struct LoadStream {
    pub url: String,
    pub bytes: Vec<u8>,
}
#[derive(Resource, Default)]
pub(super) struct ViewerStreams {
    pending: Option<(Entity, String)>,
}

/// Stage a validated manifest while retaining the displayed scene.
pub(super) fn stage(
    mut commands: Commands,
    mut requests: MessageReader<LoadStream>,
    mut files: MessageReader<LoadSplat>,
    options: Res<ViewerOptions>,
    mut pending: ResMut<ViewerStreams>,
    mut scene: ResMut<SplatScene>,
    mut decode: ResMut<crate::scene::PendingLoad>,
    mut roots: Query<&mut Transform, With<SogStream>>,
) {
    if files.read().count() > 0 {
        if let Some((entity, _)) = pending.pending.take() {
            commands.entity(entity).despawn();
        }
    }
    if let Some((entity, _)) = &pending.pending {
        if let Ok(mut transform) = roots.get_mut(*entity) {
            *transform = options.up.transform();
        }
    }
    for request in requests.read() {
        decode.cancel(&mut scene);
        if let Some((entity, _)) = pending.pending.take() {
            commands.entity(entity).despawn();
        }
        let name = url::Url::parse(&request.url)
            .ok()
            .and_then(|url| {
                url.path_segments()
                    .and_then(|mut parts| parts.next_back())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "SOG stream".into());
        match SogStream::parse(
            &request.bytes,
            &request.url,
            name.clone(),
            "viewer-stream".into(),
        ) {
            Ok(stream) => {
                let bounds = stream.fit_extent();
                let geometry = SourceGeometry {
                    bounds: (
                        bounds.center - bounds.half_extents,
                        bounds.center + bounds.half_extents,
                    ),
                    support: 0.0,
                };
                let entity = commands
                    .spawn((
                        stream,
                        geometry,
                        options.up.transform(),
                        Visibility::Hidden,
                        Name::new("Streamed Gaussian scene"),
                    ))
                    .id();
                pending.pending = Some((entity, name.clone()));
                scene.phase = LoadPhase::Loading(name);
            }
            Err(error) => scene.phase = LoadPhase::Failed(format!("{name}: {error}")),
        }
    }
}

/// Commit a scene only after its first renderer-ready generation; preserve the camera.
pub(super) fn poll(
    mut commands: Commands,
    mut pending: ResMut<ViewerStreams>,
    mut scene: ResMut<SplatScene>,
    streams: Query<(&SogStream, &SourceGeometry, &Transform)>,
    mut loaded: MessageWriter<SplatLoaded>,
) {
    let Some((entity, name)) = pending.pending.clone() else {
        let active = scene.loaded.as_ref().map(|loaded| loaded.entity);
        if let Some(entity) = active
            && let Ok((stream, _, _)) = streams.get(entity)
            && let LoadPhase::Ready { count, .. } = &mut scene.phase
        {
            *count = stream.status().displayed;
        }
        return;
    };
    let Ok((stream, geometry, transform)) = streams.get(entity) else {
        pending.pending = None;
        return;
    };
    if let Some(error) = stream.terminal_error() {
        scene.phase = LoadPhase::Failed(format!("{name}: {error}"));
        commands.entity(entity).despawn();
        pending.pending = None;
        return;
    }
    let status = stream.status();
    if status.displayed == 0 {
        return;
    }
    let bounds = geometry.transformed_bounds(*transform);
    if let Some(previous) = scene.loaded.take() {
        commands.queue(move |world: &mut World| crate::scene::retire(world, previous.entity));
    }
    commands.entity(entity).insert(Visibility::Inherited);
    scene.loaded = Some(LoadedSplat { entity, bounds });
    scene.phase = LoadPhase::Ready {
        name,
        count: status.displayed,
    };
    loaded.write(SplatLoaded { bounds });
    pending.pending = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    const MANIFEST: &[u8] = br#"{"lodLevels":1,"filenames":["coarse/meta.json"],"tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{"0":{"file":0,"offset":0,"count":1}}}}"#;
    #[test]
    fn invalid_or_superseded_manifest_preserves_visible_scene_and_retires_pending() {
        let mut app = App::new();
        app.init_resource::<ViewerOptions>()
            .init_resource::<ViewerStreams>()
            .init_resource::<SplatScene>()
            .init_resource::<crate::scene::PendingLoad>()
            .add_message::<LoadStream>()
            .add_message::<LoadSplat>()
            .add_systems(Update, stage);
        let old = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<SplatScene>().loaded = Some(LoadedSplat {
            entity: old,
            bounds: (Vec3::ZERO, Vec3::ONE),
        });
        app.world_mut().write_message(LoadStream {
            url: "https://example.test/lod-meta.json".into(),
            bytes: MANIFEST.into(),
        });
        app.update();
        let pending = app
            .world()
            .resource::<ViewerStreams>()
            .pending
            .as_ref()
            .unwrap()
            .0;
        assert_ne!(pending, old);
        assert_eq!(
            app.world()
                .resource::<SplatScene>()
                .loaded
                .as_ref()
                .unwrap()
                .entity,
            old
        );
        app.world_mut().write_message(LoadStream {
            url: "https://example.test/invalid.json".into(),
            bytes: b"{}".to_vec(),
        });
        app.update();
        assert!(app.world().get_entity(pending).is_err());
        assert!(app.world().get_entity(old).is_ok());
        assert!(matches!(
            app.world().resource::<SplatScene>().phase,
            LoadPhase::Failed(_)
        ));
    }
}
