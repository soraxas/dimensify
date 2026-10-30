//! Hosted manifest composition over the same streaming engine used by applications.
use super::ViewerOptions;
use crate::scene::SourceGeometry;
use crate::streaming::runtime::SogStream;
use crate::{LoadPhase, LoadSplat, LoadedSplat, SplatLoaded, SplatScene};
use bevy::prelude::*;

#[derive(Message, Clone)]
pub(super) struct LoadStream {
    pub url: String,
    pub bytes: Vec<u8>,
}
#[derive(Resource, Default)]
pub(super) struct ViewerStreams {
    pending: Option<(Entity, String)>,
    source: Option<LoadStream>,
    pending_source: Option<LoadStream>,
    reload_started: Option<bevy::platform::time::Instant>,
}
impl ViewerStreams {
    pub(super) fn has_pending(
        &self,
        decode: &crate::scene::PendingLoad,
        scene: &SplatScene,
    ) -> bool {
        self.pending.is_some()
            || decode.is_pending()
            // poll_load can queue the completed result before its deferred
            // scene commit; it still owns the selection until that commit.
            || matches!(scene.phase, LoadPhase::Loading(_))
    }
}

/// Stage a validated manifest while retaining the displayed scene.
#[allow(
    clippy::too_many_arguments,
    reason = "Bevy injects each independent system parameter."
)]
pub(super) fn stage(
    mut commands: Commands,
    mut requests: MessageReader<LoadStream>,
    mut files: MessageReader<LoadSplat>,
    options: Res<ViewerOptions>,
    mut pending: ResMut<ViewerStreams>,
    mut scene: ResMut<SplatScene>,
    mut decode: ResMut<crate::scene::PendingLoad>,
    mut roots: Query<&mut Transform, With<SogStream>>,
    mut settings: ResMut<super::settings::ViewerSettings>,
) {
    let reload_requested = std::mem::take(&mut settings.reload_requested);
    let incoming: Vec<_> = requests.read().cloned().collect();
    let file_selected = files.read().count() > 0;
    let new_scene = file_selected || !incoming.is_empty();
    if new_scene {
        if let Some((entity, _)) = pending.pending.take() {
            commands.entity(entity).despawn();
        }
        pending.pending_source = None;
        pending.reload_started = None;
        if settings.applying || reload_requested {
            settings.complete(Err("Renderer reload superseded by a new scene".into()));
        }
        // Retain the current manifest until a replacement commits. File decode
        // can fail while the original stream remains displayed and reloadable.
    }
    let (reload, incoming) = if reload_requested && !new_scene {
        if pending.has_pending(&decode, &scene) {
            settings.complete(Err(
                "A scene load is in progress; retry Apply afterwards".into()
            ));
            (false, Vec::new())
        } else {
            match pending.source.clone() {
                Some(source) => (true, vec![source]),
                None => {
                    settings.complete(Err("No streamed scene to reload".into()));
                    (false, Vec::new())
                }
            }
        }
    } else {
        (false, incoming)
    };
    if let Some((entity, _)) = &pending.pending
        && let Ok(mut transform) = roots.get_mut(*entity)
    {
        *transform = options.up.transform();
    }
    for request in incoming {
        if !reload {
            decode.cancel(&mut scene);
        }
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
                        if reload {
                            settings.draft
                        } else {
                            settings.active
                        },
                        geometry,
                        options.up.transform(),
                        Visibility::Hidden,
                        Name::new("Streamed Gaussian scene"),
                    ))
                    .id();
                pending.pending = Some((entity, name.clone()));
                pending.pending_source = Some(request);
                pending.reload_started = reload.then(bevy::platform::time::Instant::now);
                scene.phase = LoadPhase::Loading(name);
            }
            Err(error) => {
                scene.phase = LoadPhase::Failed(format!("{name}: {error}"));
                if reload {
                    settings.complete(Err(error));
                }
            }
        }
    }
}

/// Commit a scene only after its first renderer-ready generation; preserve the camera.
pub(super) fn poll(
    mut commands: Commands,
    mut pending: ResMut<ViewerStreams>,
    mut scene: ResMut<SplatScene>,
    mut streams: Query<(
        &SogStream,
        &SourceGeometry,
        &Transform,
        &mut crate::runtime::SogStreamRenderSettings,
    )>,
    mut settings: ResMut<super::settings::ViewerSettings>,
    mut loaded: MessageWriter<SplatLoaded>,
) {
    let Some((entity, name)) = pending.pending.clone() else {
        let active = scene.loaded.as_ref().map(|loaded| loaded.entity);
        if let Some(entity) = active
            && let Ok((stream, _, _, _)) = streams.get(entity)
            && let LoadPhase::Ready { count, .. } = &mut scene.phase
        {
            *count = stream.status().displayed;
        }
        return;
    };
    let Ok((stream, geometry, transform, mut configuration)) = streams.get_mut(entity) else {
        pending.pending = None;
        pending.pending_source = None;
        pending.reload_started = None;
        scene.phase = LoadPhase::Failed(format!("{name}: Replacement scene was removed"));
        if settings.applying {
            settings.complete(Err("Replacement scene was removed".into()));
        }
        return;
    };
    let timed_out = pending
        .reload_started
        .is_some_and(|start| start.elapsed().as_secs() >= 30);
    if let Some(error) = stream.terminal_error().or(timed_out.then_some("Replacement could not become ready within 30 seconds; check budget headroom and source availability")) {
        scene.phase = LoadPhase::Failed(format!("{name}: {error}"));
        commands.entity(entity).despawn();
        pending.pending = None;
        pending.pending_source = None;
        pending.reload_started = None;
        if settings.applying { settings.complete(Err(error.into())); }
        return;
    }
    let status = stream.status();
    if status.displayed == 0 {
        return;
    }
    let bounds = geometry.transformed_bounds(*transform);
    // Ordinary loads use the current live compaction choice. Only an explicit
    // renderer reload commits the staged storage configuration to the panel.
    let reloaded = pending.reload_started.is_some();
    if !reloaded {
        configuration.compaction = settings.active.compaction;
    }
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
    pending.source = pending.pending_source.take();
    pending.reload_started = None;
    if reloaded {
        settings.complete(Ok(*configuration));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MANIFEST: &[u8] = br#"{"lodLevels":1,"filenames":["coarse/meta.json"],"tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{"0":{"file":0,"offset":0,"count":1}}}}"#;
    fn staging_app() -> App {
        let mut app = App::new();
        app.init_resource::<ViewerOptions>()
            .init_resource::<ViewerStreams>()
            .init_resource::<super::super::settings::ViewerSettings>()
            .init_resource::<SplatScene>()
            .init_resource::<crate::scene::PendingLoad>()
            .add_message::<LoadStream>()
            .add_message::<LoadSplat>()
            .add_systems(Update, stage);
        app.world_mut().resource_mut::<ViewerStreams>().source = Some(LoadStream {
            url: "https://example.test/old.json".into(),
            bytes: MANIFEST.into(),
        });
        app
    }
    fn request_reload(app: &mut App) {
        let mut settings = app
            .world_mut()
            .resource_mut::<super::super::settings::ViewerSettings>();
        settings.reload_requested = true;
        settings.applying = true;
    }
    #[test]
    fn removed_candidate_releases_the_loading_guard_for_retry() {
        let mut app = staging_app();
        app.add_message::<SplatLoaded>()
            .add_systems(Update, poll.after(stage));
        app.world_mut().write_message(LoadStream {
            url: "https://example.test/new.json".into(),
            bytes: MANIFEST.into(),
        });
        app.update();
        let candidate = app
            .world()
            .resource::<ViewerStreams>()
            .pending
            .as_ref()
            .unwrap()
            .0;
        app.world_mut().despawn(candidate);
        app.update();
        assert!(matches!(
            app.world().resource::<SplatScene>().phase,
            LoadPhase::Failed(_)
        ));
        request_reload(&mut app);
        app.update();
        assert!(
            app.world().resource::<ViewerStreams>().pending.is_some(),
            "removal must not leave Apply permanently blocked"
        );
    }

    #[test]
    fn queued_whole_file_commit_also_blocks_reload() {
        let mut app = staging_app();
        // poll_load has taken the task and queued finish(), but the new file
        // still owns the request until that deferred command commits it.
        app.world_mut().resource_mut::<SplatScene>().phase = LoadPhase::Loading("tiny.ply".into());
        request_reload(&mut app);
        app.update();
        assert!(app.world().resource::<ViewerStreams>().pending.is_none());
        assert!(matches!(
            app.world().resource::<SplatScene>().phase,
            LoadPhase::Loading(_)
        ));
        assert!(
            !app.world()
                .resource::<super::super::settings::ViewerSettings>()
                .applying
        );
    }

    #[test]
    fn reload_does_not_cancel_a_pending_whole_file_decode() {
        let mut app = staging_app();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, crate::scene::request_load.before(stage));
        let old = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<SplatScene>().loaded = Some(LoadedSplat {
            entity: old,
            bounds: (Vec3::ZERO, Vec3::ONE),
        });
        app.world_mut().write_message(LoadSplat {
            name: "tiny.ply".into(),
            bytes: include_bytes!("../../tests/fixtures/tiny.ply")
                .as_slice()
                .into(),
            up: crate::SourceUp::Z,
        });
        app.update();
        // Run the production request system, but leave completion queued so the
        // reload races the real pending decoder independently of task speed.
        assert!(
            app.world()
                .resource::<crate::scene::PendingLoad>()
                .is_pending()
        );
        request_reload(&mut app);
        app.update();
        assert!(
            app.world()
                .resource::<crate::scene::PendingLoad>()
                .is_pending(),
            "Apply must preserve the file selected by the user"
        );
        assert!(app.world().resource::<ViewerStreams>().pending.is_none());
        assert_eq!(
            app.world()
                .resource::<SplatScene>()
                .loaded
                .as_ref()
                .unwrap()
                .entity,
            old
        );
        assert!(
            !app.world()
                .resource::<super::super::settings::ViewerSettings>()
                .applying
        );
    }

    #[test]
    fn reload_request_preserves_an_in_progress_scene() {
        let mut app = staging_app();
        app.world_mut().write_message(LoadStream {
            url: "https://example.test/new.json".into(),
            bytes: MANIFEST.into(),
        });
        app.update();
        let candidate = app
            .world()
            .resource::<ViewerStreams>()
            .pending
            .as_ref()
            .unwrap()
            .0;
        request_reload(&mut app);
        app.update();
        let pending = app.world().resource::<ViewerStreams>();
        assert_eq!(pending.pending.as_ref().unwrap().0, candidate);
        assert_eq!(
            pending.pending_source.as_ref().unwrap().url,
            "https://example.test/new.json"
        );
        let settings = app
            .world()
            .resource::<super::super::settings::ViewerSettings>();
        assert!(!settings.applying);
        assert!(settings.notice.contains("load"));
    }
    #[test]
    fn new_file_and_stream_requests_take_priority_over_same_frame_reload() {
        for file in [false, true] {
            let mut app = staging_app();
            request_reload(&mut app);
            if file {
                app.world_mut().write_message(LoadSplat {
                    name: "new.sog".into(),
                    bytes: b"new file".as_slice().into(),
                    up: crate::SourceUp::Z,
                });
            } else {
                app.world_mut().write_message(LoadStream {
                    url: "https://example.test/new.json".into(),
                    bytes: MANIFEST.into(),
                });
            }
            app.update();
            let pending = app.world().resource::<ViewerStreams>();
            if file {
                assert!(pending.pending.is_none());
            } else {
                assert_eq!(
                    pending.pending_source.as_ref().unwrap().url,
                    "https://example.test/new.json"
                );
            }
            assert!(pending.reload_started.is_none());
            assert!(
                !app.world()
                    .resource::<super::super::settings::ViewerSettings>()
                    .applying
            );
        }
    }
    #[test]
    fn reload_stages_new_configuration_and_preserves_the_displayed_scene() {
        let mut app = App::new();
        app.init_resource::<ViewerOptions>()
            .init_resource::<ViewerStreams>()
            .init_resource::<super::super::settings::ViewerSettings>()
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
        app.world_mut().resource_mut::<ViewerStreams>().source = Some(LoadStream {
            url: "https://example.test/lod-meta.json".into(),
            bytes: MANIFEST.into(),
        });
        // A rejected file replacement must not discard the displayed stream's
        // manifest: the next renderer Apply still reloads that stream.
        app.world_mut().write_message(LoadSplat {
            name: "broken.sog".into(),
            bytes: b"invalid".as_slice().into(),
            up: crate::SourceUp::Z,
        });
        app.update();
        app.world_mut().resource_mut::<SplatScene>().phase =
            LoadPhase::Failed("invalid replacement".into());
        {
            let mut settings = app
                .world_mut()
                .resource_mut::<super::super::settings::ViewerSettings>();
            settings.draft.backend = crate::renderer::SogRendererBackend::Snapshot;
            settings.reload_requested = true;
            settings.applying = true;
        }
        app.update();
        let pending = app
            .world()
            .resource::<ViewerStreams>()
            .pending
            .as_ref()
            .expect("Apply must stage a replacement")
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
        assert_eq!(
            app.world()
                .get::<crate::runtime::SogStreamRenderSettings>(pending)
                .unwrap()
                .backend,
            crate::renderer::SogRendererBackend::Snapshot
        );
        assert_eq!(
            app.world()
                .resource::<super::super::settings::ViewerSettings>()
                .active
                .backend,
            crate::renderer::SogRendererBackend::Packed
        );
        assert!(app.world().get_entity(old).is_ok());
    }

    #[test]
    fn invalid_or_superseded_manifest_preserves_visible_scene_and_retires_pending() {
        let mut app = App::new();
        app.init_resource::<ViewerOptions>()
            .init_resource::<ViewerStreams>()
            .init_resource::<super::super::settings::ViewerSettings>()
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

#[cfg(test)]
mod reload_failure_tests {
    use super::*;
    #[test]
    fn timeout_keeps_the_original_scene_and_configuration() {
        let mut app = App::new();
        app.init_resource::<ViewerOptions>()
            .init_resource::<ViewerStreams>()
            .init_resource::<super::super::settings::ViewerSettings>()
            .init_resource::<SplatScene>()
            .init_resource::<crate::scene::PendingLoad>()
            .add_message::<LoadStream>()
            .add_message::<LoadSplat>()
            .add_message::<SplatLoaded>()
            .add_systems(Update, (stage, poll).chain());
        let original = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<SplatScene>().loaded = Some(LoadedSplat {
            entity: original,
            bounds: (Vec3::ZERO, Vec3::ONE),
        });
        app.world_mut().write_message(LoadStream { url:"https://example.test/lod-meta.json".into(),bytes:br#"{"lodLevels":1,"filenames":["coarse/meta.json"],"tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{"0":{"file":0,"offset":0,"count":1}}}}"#.into() });
        app.update();
        let candidate = app
            .world()
            .resource::<ViewerStreams>()
            .pending
            .as_ref()
            .unwrap()
            .0;
        app.world_mut()
            .resource_mut::<ViewerStreams>()
            .reload_started =
            Some(bevy::platform::time::Instant::now() - std::time::Duration::from_secs(31));
        app.world_mut()
            .resource_mut::<super::super::settings::ViewerSettings>()
            .applying = true;
        app.update();
        assert!(app.world().get_entity(original).is_ok());
        assert!(app.world().get_entity(candidate).is_err());
        assert_eq!(
            app.world()
                .resource::<SplatScene>()
                .loaded
                .as_ref()
                .unwrap()
                .entity,
            original
        );
        let settings = app
            .world()
            .resource::<super::super::settings::ViewerSettings>();
        assert!(!settings.applying);
        assert_eq!(
            settings.active.backend,
            crate::renderer::SogRendererBackend::Packed
        );
        assert!(settings.notice.contains("30 seconds"));
    }
}
