//! Bevy plugin and per-frame system wiring for streamed SOG scenes.
use super::*;

/// Initialize the shared streaming admission and diagnostic resources.
pub fn install_sog_streaming(app: &mut App) {
    renderer::readiness::install(app);
}

/// Camera positions and scene transforms are read after transform propagation.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_sog_streams(
    mut renderer: SnapshotRenderer,
    time: Res<Time<Real>>,
    settings: Res<SogLodSettings>,
    mut streams: Query<(Entity, &GlobalTransform, &mut SogStream)>,
    cameras: Query<
        (Entity, &GlobalTransform, &Projection, &Camera),
        (With<StreamingCamera>, With<Camera3d>),
    >,
) {
    let Ok((camera, camera_transform, projection, camera_component)) = cameras.single() else {
        return;
    };
    let now = time.elapsed_secs_f64();
    let gpu_limit = renderer.splat_limit();
    for (entity, transform, mut stream) in &mut streams {
        let resolver = stream.resolver.clone();
        if stream.generation == 0 {
            stream.storage = renderer.source_storage();
            stream.activation.storage = stream.storage;
            if renderer.uses_pages()
                && stream.pages.is_none()
                && let Err(error) = stream.enable_pages()
            {
                stream.last_error = Some(error);
                continue;
            }
        }
        stream.cpu_timings = renderer.cpu_timings();
        stream.cache_trace = renderer.cache_trace(entity);
        if stream.pages.is_none() && stream.storage == SourceStorage::Packed {
            let trace = stream.cache_trace.clone();
            let generation = stream.generation;
            stream.sync_resident_sources(|file, ready| renderer.sync_source(entity, file, ready));
            warm_cache::trim(
                &mut stream.entries,
                settings.cache_bytes,
                0,
                |file, bytes| {
                    trace.record(
                        crate::diagnostics::sog_cache::Kind::WarmEviction,
                        file,
                        bytes,
                        None,
                        generation,
                    );
                },
            );
        }
        let ready = renderer.poll(&mut stream.snapshot);
        if ready && stream.snapshot.is_visible() && stream.displayed != stream.submitted {
            stream.displayed = stream.submitted.clone();
            // Releasing old displayed ownership can make a previously rejected
            // replacement union fit without another download or policy tick.
            stream.replacements.dirty |= !stream.replacements.leaves.is_empty();
            let SogStream {
                activation,
                cache_trace,
                displayed,
                ..
            } = &mut *stream;
            activation.activated(cache_trace, displayed);
        }
        let source_capacity = renderer.source_cache_bytes(entity);
        stream.refresh_replacement_admission(&settings, gpu_limit, source_capacity);
        let progress_only = now < stream.next_update && !stream.needs_upload_frame();
        if progress_only {
            stream.poll_loads(now);
            // Bootstrap/environment/page completions retain their policy wake.
            if stream.pages.is_some() || stream.next_update <= now {
                continue;
            }
            let storage = stream.storage;
            stream.progress_requests(
                now,
                &settings,
                source_capacity,
                |url, expected, max, now| {
                    ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                        .with_owner(entity.to_bits())
                },
            );
            if !ready {
                stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::RendererPending);
                continue;
            }
            if stream.submission_error.is_some() {
                stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::SubmissionError);
                continue;
            }
            if !stream.replacements.dirty {
                stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::PolicyWait);
                continue;
            }
            if !stream.progress_replacements(&settings, gpu_limit, source_capacity) {
                continue;
            }
        }
        if !ready {
            stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::RendererPending);
            // Keep the due tick latched, but do no pin rebuilding or camera
            // math while renderer ownership prevents using that selection.
            // Turning away must not discard the previous complete coverage.
            stream.poll_loads(now);
            stream.progress_pages(settings.cache_bytes);
            if stream.pages.is_none() {
                let storage = stream.storage;
                stream.progress_requests(
                    now,
                    &settings,
                    source_capacity,
                    |url, expected, max, now| {
                        ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                            .with_owner(entity.to_bits())
                    },
                );
            }
            continue;
        }
        if stream.pages.is_none() {
            renderer.pin_sources(
                entity,
                stream
                    .manifest
                    .coarse_files()
                    .into_iter()
                    .chain(stream.environment_index()),
            );
        }
        let eye = transform
            .affine()
            .inverse()
            .transform_point3(camera_transform.translation());
        let eye = if eye.is_finite() { eye } else { Vec3::ZERO };
        let forward = transform
            .affine()
            .inverse()
            .transform_vector3(*camera_transform.forward())
            .normalize_or_zero();
        let fov_scale = match projection {
            Projection::Perspective(p) => {
                let tan_v = (p.fov * 0.5).tan();
                tan_v.min(tan_v * p.aspect_ratio) / (std::f32::consts::FRAC_PI_8).tan()
            }
            _ => 1.0,
        };
        let clip_from_local = if settings.effective_visibility() == SogVisibilityPolicy::Frustum
            || stream.diagnostics.is_some()
        {
            Some(
                projection.get_clip_from_view()
                    * camera_transform.to_matrix().inverse()
                    * transform.to_matrix(),
            )
        } else {
            None
        };
        let view = selection::View {
            pixels: camera_component
                .physical_viewport_size()
                .map(|size| screen_space::PixelView {
                    clip_from_local: projection.get_clip_from_view()
                        * camera_transform.to_matrix().inverse()
                        * transform.to_matrix(),
                    viewport: size.as_vec2(),
                }),
            eye,
            forward,
            fov_scale,
            clip_from_local,
            ..Vec3::ZERO.into()
        };
        // A rejected policy candidate never owned a renderer generation. Only
        // the last accepted selection must remain pinned on the next attempt.
        if stream.submission_error.is_some() && !stream.preparing_upload {
            stream.active = stream.submitted.clone();
        }
        let storage = stream.storage;
        if !stream.preparing_upload && !progress_only {
            stream.tick(
                view,
                now,
                &settings,
                gpu_limit,
                source_capacity,
                |url, expected, max, now| {
                    ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                        .with_owner(entity.to_bits())
                },
            );
            // Advance only after a real evaluation. A pending GPU generation
            // defers the due tick; polling it must not consume another 200ms.
            stream.next_update = now + 0.2;
        } else if !progress_only {
            stream.poll_loads(now);
            stream.progress_pages(settings.cache_bytes);
            if stream.pages.is_none() {
                stream.progress_requests(
                    now,
                    &settings,
                    source_capacity,
                    |url, expected, max, now| {
                        ChunkLoad::new_resolved(url, expected, max, now, storage, resolver.clone())
                            .with_owner(entity.to_bits())
                    },
                );
            }
        }
        if stream.active != stream.submitted {
            if stream.active.is_empty() {
                renderer.clear(&mut stream.snapshot);
                stream.submitted.clear();
                stream.displayed.clear();
                stream.submission_error = None;
                stream.rejected_admission = None;
            } else if stream
                .rejected_admission
                .as_ref()
                .is_none_or(|(ranges, limit, source)| {
                    ranges != &stream.active || *limit != gpu_limit || *source != source_capacity
                })
            {
                let SogStream {
                    snapshot,
                    active,
                    entries,
                    pages,
                    ..
                } = &mut *stream;
                let result = if let Some(pages) = pages {
                    pages.split(active).and_then(|ranges| {
                        renderer.submit(snapshot, entity, camera, &ranges, |id| pages.source(id))
                    })
                } else {
                    renderer.submit(snapshot, entity, camera, active, |file| {
                        entries.get(file).and_then(|entry| entry.ready.as_ref())
                    })
                };
                match result {
                    Ok(renderer::Submission::Preparing) => {
                        stream.preparing_upload = true;
                        stream.submission_error = None;
                        stream.trace_guard(
                            crate::diagnostics::sog_cache::ActivationGuard::UploadPreparing,
                        );
                    }
                    Ok(renderer::Submission::Submitted) => {
                        stream.preparing_upload = false;
                        let count: usize = stream.active.iter().map(|r| r.count).sum();
                        debug!(
                            "SOG LOD '{}': {count} selected splats in {} ranges",
                            stream.name,
                            stream.active.len()
                        );
                        stream.submitted = stream.active.clone();
                        let SogStream {
                            activation,
                            cache_trace,
                            submitted,
                            ..
                        } = &mut *stream;
                        activation.submitted(cache_trace, submitted);
                        stream.submission_error = None;
                        stream.rejected_admission = None;
                    }
                    Err(error) => {
                        stream.preparing_upload = false;
                        if stream.submission_error.as_ref() != Some(&error) {
                            warn!("SOG submission: {error}");
                        }
                        if storage != SourceStorage::Packed
                            && (error.contains("GPU source budget")
                                || error.contains("GPU selected slot budget")
                                || error.contains("GPU storage binding limit"))
                        {
                            stream.rejected_admission =
                                Some((stream.active.clone(), gpu_limit, source_capacity));
                        }
                        stream.submission_error = Some(error);
                        stream.trace_guard(
                            crate::diagnostics::sog_cache::ActivationGuard::SubmissionError,
                        );
                    }
                }
            }
        } else {
            // A camera may return to the last accepted selection after a failure.
            stream.submission_error = None;
            stream.trace_guard(crate::diagnostics::sog_cache::ActivationGuard::PolicyWait);
        }
    }
}

/// Install progressive streaming over an already installed Gaussian renderer.
pub struct SplatStreamingPlugin;
impl Plugin for SplatStreamingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SogLodSettings>()
            .init_resource::<crate::runtime::SogProjectionCache>()
            .init_resource::<crate::runtime::SogPagedSources>();
        install_sog_streaming(app);
        app.add_plugins(crate::diagnostics::quality::SplatDiagnosticsPlugin);
        #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
        {
            app.insert_resource(crate::renderer::SogRendererBackend::Snapshot);
            app.world_mut()
                .resource_mut::<crate::runtime::SogRuntimeSettings>()
                .backend = crate::renderer::SogRendererBackend::Snapshot;
        }
        app.add_systems(
            PostUpdate,
            update_sog_streams
                .after(TransformSystems::Propagate)
                .after(bevy::camera::CameraUpdateSystems)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                .before(bevy::camera::visibility::VisibilitySystems::CalculateBounds),
        );
    }
}
