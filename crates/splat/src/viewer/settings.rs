//! Session-local viewer controls over the existing renderer and streaming APIs.
use crate::{
    SplatScene,
    renderer::{SogRendererBackend, gpu::GpuBudget},
    runtime::{SogStreamRenderSettings, SplatTransparency},
    streaming::{SogLodPolicy, SogLodSettings, SogVisibilityPolicy, runtime::SogStream},
};
use bevy::{
    feathers::{controls::FeathersButton, theme::ThemedText},
    input::mouse::{MouseScrollUnit, MouseWheel},
    picking::hover::Hovered,
    prelude::*,
    ui::InteractionDisabled,
    ui_widgets::Activate,
};
use bevy_gaussian_splatting::{
    CloudSettings, GaussianTransparency, RadixSortDepthBits, sort::SortMode,
};

const MIB: u64 = 1024 * 1024;

#[derive(Resource)]
pub(super) struct ViewerSettings {
    pub open: bool,
    advanced: bool,
    pub active: SogStreamRenderSettings,
    pub draft: SogStreamRenderSettings,
    pub reload_requested: bool,
    pub applying: bool,
    pub notice: String,
    bits: RadixSortDepthBits,
    cpu_sort: bool,
}
impl Default for ViewerSettings {
    fn default() -> Self {
        let active = SogStreamRenderSettings {
            backend: SogRendererBackend::default(),
            projection_cache: false,
            compaction: false,
        };
        Self {
            open: false,
            advanced: false,
            active,
            draft: active,
            reload_requested: false,
            applying: false,
            notice: String::new(),
            bits: RadixSortDepthBits::Bits32,
            cpu_sort: false,
        }
    }
}
impl ViewerSettings {
    pub fn complete(&mut self, result: Result<SogStreamRenderSettings, String>) {
        match result {
            Ok(settings) => {
                self.active = settings;
                self.notice = "Renderer settings applied".into();
            }
            Err(error) => self.notice = error,
        }
        self.applying = false;
        self.reload_requested = false;
    }
    #[cfg(all(feature = "viewer-perf", target_arch = "wasm32"))]
    pub(super) fn observed(&self) -> serde_json::Value {
        serde_json::json!({
            "open": self.open, "advanced": self.advanced,
            "active_backend": crate::runtime::backend_name(self.active.backend),
            "pending_backend": crate::runtime::backend_name(self.draft.backend),
            "projection_cache": self.active.projection_cache,
            "pending_projection_cache": self.draft.projection_cache,
            "compaction_requested": self.active.compaction,
            "radix_bits": format!("{:?}", self.bits),
            "cpu_sort_requested": self.cpu_sort,
            "applying": self.applying, "notice": self.notice,
        })
    }
    fn sorting(&self, streaming: bool, mode: GaussianTransparency) -> &'static str {
        if mode == GaussianTransparency::Stochastic {
            "Skipped (stochastic coverage)"
        } else if self.cpu_sort
            && (!streaming || self.active.backend == SogRendererBackend::Snapshot)
        {
            "CPU standard"
        } else {
            "GPU radix"
        }
    }
    fn reload_needed(&self) -> bool {
        self.draft.backend != self.active.backend
            || self.draft.projection_cache != self.active.projection_cache
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Transparency,
    Policy,
    Footprint,
    Splats,
    Cache,
    Sources,
    Selection,
    Concurrency,
    Visibility,
    Advanced,
    Backend,
    Projection,
    Compaction,
    Precision,
    Sort,
    Apply,
    Reset,
    Close,
    Scroll,
}
#[derive(Component, Clone, Copy)]
pub(super) struct Control(Field, i32);
#[cfg(all(feature = "viewer-perf", target_arch = "wasm32"))]
impl Control {
    pub(super) fn observed(
        &self,
        node: &ComputedNode,
        transform: &UiGlobalTransform,
        disabled: bool,
    ) -> serde_json::Value {
        let center = transform.translation * node.inverse_scale_factor();
        serde_json::json!({ "field": format!("{:?}", self.0), "direction": self.1,
            "x": center.x, "y": center.y, "disabled": disabled,
            "width": node.size().x * node.inverse_scale_factor(),
            "height": node.size().y * node.inverse_scale_factor() })
    }
}

#[derive(Component)]
pub(super) struct Label(Field);
#[derive(Component)]
pub(super) struct Panel;
#[derive(Component)]
pub(super) struct AdvancedRow;
#[derive(Component)]
pub(super) struct Summary;

fn button(parent: &mut ChildSpawnerCommands, value: &str, control: Control) {
    let entity = parent.target_entity();
    parent
        .commands_mut()
        .spawn_scene(bsn! {
            @FeathersButton
            Node { min_width: px(44), min_height: px(44), padding: UiRect::horizontal(px(10)) }
            on(activate)
        })
        .insert((control, ChildOf(entity)))
        .with_children(|p| {
            p.spawn((Text::new(value), ThemedText));
        });
}

pub(super) fn setup(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            Hovered::default(),
            ScrollPosition::default(),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                right: px(12),
                top: px(76),
                bottom: px(104),
                width: px(420),
                max_width: percent(94),
                padding: UiRect::all(px(14)),
                row_gap: px(8),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            GlobalZIndex(20),
            BackgroundColor(Color::srgb(0.07, 0.11, 0.17)),
        ))
        .observe(drag_scroll)
        .with_children(|panel| {
            panel.spawn((
                Text::new("Gaussian settings"),
                TextFont::from_font_size(20.),
                ThemedText,
            ));
            button(panel, "Close", Control(Field::Close, 0));
            for (field, advanced) in [
                (Field::Transparency, false),
                (Field::Policy, false),
                (Field::Footprint, false),
                (Field::Splats, false),
                (Field::Cache, false),
                (Field::Sources, false),
                (Field::Selection, false),
                (Field::Concurrency, false),
                (Field::Visibility, false),
                (Field::Advanced, false),
                (Field::Backend, true),
                (Field::Projection, true),
                (Field::Compaction, true),
                (Field::Precision, true),
                (Field::Sort, true),
            ] {
                let mut row = panel.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    flex_shrink: 0.,
                    ..default()
                });
                if advanced {
                    row.insert(AdvancedRow);
                }
                row.with_children(|row| {
                    row.spawn((
                        Label(field),
                        Text::new(""),
                        TextFont::from_font_size(14.),
                        ThemedText,
                    ));
                    row.spawn(Node {
                        column_gap: px(6),
                        ..default()
                    })
                    .with_children(|actions| {
                        if matches!(
                            field,
                            Field::Footprint
                                | Field::Splats
                                | Field::Cache
                                | Field::Sources
                                | Field::Selection
                                | Field::Concurrency
                        ) {
                            button(actions, "-", Control(field, -1));
                            button(actions, "+", Control(field, 1));
                        } else {
                            button(actions, "Change", Control(field, 1));
                        }
                    });
                });
            }
            button(panel, "Apply and reload scene", Control(Field::Apply, 0));
            button(panel, "Reset settings", Control(Field::Reset, 0));
            panel.spawn((
                Summary,
                Text::new(""),
                TextFont::from_font_size(13.),
                ThemedText,
            ));
            button(panel, "Scroll up", Control(Field::Scroll, -1));
            button(panel, "Scroll down", Control(Field::Scroll, 1));
        });
}

fn stream_active(scene: &SplatScene, streams: &Query<&SogStream>) -> bool {
    scene
        .loaded
        .as_ref()
        .is_some_and(|s| streams.contains(s.entity))
}
fn enabled(
    field: Field,
    state: &ViewerSettings,
    streaming: bool,
    mode: GaussianTransparency,
) -> bool {
    if matches!(field, Field::Close | Field::Scroll | Field::Advanced) {
        return true;
    }
    if state.applying {
        return false;
    }
    match field {
        Field::Apply => state.reload_needed(),
        Field::Backend => true,
        Field::Projection => state.draft.backend == SogRendererBackend::Packed,
        Field::Compaction | Field::Precision => {
            mode == GaussianTransparency::Sorted
                && !(state.cpu_sort
                    && (!streaming || state.active.backend == SogRendererBackend::Snapshot))
        }
        Field::Sort => {
            mode == GaussianTransparency::Sorted
                && (!streaming || state.active.backend == SogRendererBackend::Snapshot)
        }
        Field::Policy
        | Field::Splats
        | Field::Cache
        | Field::Sources
        | Field::Selection
        | Field::Concurrency => streaming,
        Field::Footprint => streaming,
        Field::Visibility => streaming,
        _ => true,
    }
}

fn step(value: u64, direction: i32, increment: u64, min: u64, max: u64) -> u64 {
    if direction > 0 {
        value.saturating_add(increment).clamp(min, max)
    } else {
        value.saturating_sub(increment).clamp(min, max)
    }
}

#[allow(clippy::too_many_arguments)]
fn activate(
    event: On<Activate>,
    controls: Query<&Control>,
    mut state: ResMut<ViewerSettings>,
    mut lod: ResMut<SogLodSettings>,
    mut budget: ResMut<GpuBudget>,
    mut mode: ResMut<SplatTransparency>,
    scene: Res<SplatScene>,
    streams: Query<&SogStream>,
    mut panels: Query<&mut ScrollPosition, With<Panel>>,
) {
    let Ok(&Control(field, d)) = controls.get(event.entity) else {
        return;
    };
    let streaming = stream_active(&scene, &streams);
    let applicable = if field == Field::Sort {
        streaming
    } else {
        streaming || scene.loaded.is_none()
    };
    if !enabled(field, &state, applicable, mode.0) {
        return;
    }
    match field {
        Field::Close => state.open = false,
        Field::Scroll => {
            for mut p in &mut panels {
                p.0.y = (p.0.y + d as f32 * 250.).max(0.);
            }
        }
        Field::Transparency => {
            mode.0 = if mode.0 == GaussianTransparency::Sorted {
                GaussianTransparency::Stochastic
            } else {
                GaussianTransparency::Sorted
            }
        }
        Field::Policy => {
            lod.lod_policy = if lod.lod_policy == SogLodPolicy::ScreenSpace {
                SogLodPolicy::ReferenceDistance
            } else {
                SogLodPolicy::ScreenSpace
            }
        }
        Field::Footprint if lod.lod_policy == SogLodPolicy::ScreenSpace => {
            lod.target_pixels = step(lod.target_pixels as u64, d, 1, 1, 16) as u32
        }
        Field::Splats => {
            lod.splat_budget =
                step(lod.splat_budget as u64, d, 250_000, 250_000, 16_000_000) as usize
        }
        Field::Cache => {
            lod.cache_bytes =
                step(lod.cache_bytes as u64, d, 16 * MIB, 16 * MIB, 4096 * MIB) as usize
        }
        Field::Sources => {
            budget.source_bytes = step(budget.source_bytes, d, 16 * MIB, 16 * MIB, 4096 * MIB)
        }
        Field::Selection => {
            budget.selected_bytes_per_scene = step(
                budget.selected_bytes_per_scene,
                d,
                16 * MIB,
                16 * MIB,
                4096 * MIB,
            )
        }
        Field::Concurrency => {
            lod.concurrent_chunks = step(lod.concurrent_chunks as u64, d, 1, 1, 8) as usize
        }
        Field::Visibility if lod.lod_policy == SogLodPolicy::ReferenceDistance => {
            lod.visibility = if lod.visibility == SogVisibilityPolicy::All {
                SogVisibilityPolicy::Frustum
            } else {
                SogVisibilityPolicy::All
            }
        }
        Field::Advanced => state.advanced = !state.advanced,
        Field::Backend => {
            state.draft.backend = match state.draft.backend {
                SogRendererBackend::Packed => SogRendererBackend::GpuResident,
                SogRendererBackend::GpuResident => SogRendererBackend::Snapshot,
                SogRendererBackend::Snapshot => SogRendererBackend::Packed,
            };
            if state.draft.backend != SogRendererBackend::Packed {
                state.draft.projection_cache = false;
            }
        }
        Field::Projection => state.draft.projection_cache = !state.draft.projection_cache,
        Field::Compaction => {
            state.active.compaction = !state.active.compaction;
            state.draft.compaction = state.active.compaction;
        }
        Field::Precision => {
            state.bits = match state.bits {
                RadixSortDepthBits::Bits32 => RadixSortDepthBits::Bits16,
                RadixSortDepthBits::Bits16 => RadixSortDepthBits::Bits24,
                RadixSortDepthBits::Bits24 => RadixSortDepthBits::Bits32,
            }
        }
        Field::Sort => state.cpu_sort = !state.cpu_sort,
        Field::Apply => {
            if streaming {
                state.reload_requested = true;
                state.applying = true;
                state.notice = "Preparing replacement; current scene remains visible".into();
            } else {
                let mut next = state.draft;
                next.compaction = state.active.compaction;
                state.active = next;
                state.notice =
                    "Applies to the next streamed SOG scene; whole-file storage is unchanged"
                        .into();
            }
        }
        Field::Reset => {
            *lod = SogLodSettings::default();
            *budget = GpuBudget::default();
            mode.0 = GaussianTransparency::Sorted;
            state.bits = RadixSortDepthBits::Bits32;
            state.cpu_sort = false;
            state.active.compaction = false;
            state.draft = ViewerSettings::default().active;
            if !streaming {
                state.active = state.draft;
            }
            state.notice = if state.reload_needed() {
                "Live defaults restored; Apply reloads the default backend"
            } else {
                "Default settings restored"
            }
            .into();
        }
        _ => {}
    }
}

/// Update current and pending clouds after stream publication, without touching appearance.
pub(super) fn apply_clouds(
    state: Res<ViewerSettings>,
    mut configs: Query<&mut SogStreamRenderSettings>,
    parents: Query<&ChildOf>,
    mut clouds: Query<(Entity, &mut CloudSettings)>,
    entries: Query<Ref<bevy_gaussian_splatting::sort::SortedEntriesHandle>>,
    transforms: Query<Ref<GlobalTransform>>,
    mut triggers: Query<
        &mut bevy_gaussian_splatting::sort::SortTrigger,
        With<bevy_gaussian_splatting::GaussianCamera>,
    >,
) {
    // Pending clouds must inherit live quality choices before their first draw.
    // Controls are frozen during Apply, so these choices remain stable.
    for mut config in &mut configs {
        if config.compaction != state.active.compaction {
            config.compaction = state.active.compaction;
        }
    }
    let mut needs_cpu_sort = false;
    for (entity, mut cloud) in &mut clouds {
        let mut root = entity;
        while let Ok(parent) = parents.get(root) {
            root = parent.parent();
        }
        let streamed_backend = configs.get(root).ok().map(|c| c.backend);
        let cpu =
            state.cpu_sort && streamed_backend.is_none_or(|b| b == SogRendererBackend::Snapshot);
        let sort = if cpu && cloud.transparency == GaussianTransparency::Sorted {
            SortMode::Std
        } else {
            SortMode::Radix
        };
        if sort == SortMode::Std {
            needs_cpu_sort |= cloud.sort_mode != sort
                || entries.get(entity).is_ok_and(|handle| handle.is_added())
                || transforms
                    .get(entity)
                    .is_ok_and(|transform| transform.is_changed());
        }
        if cloud.sort_mode != sort {
            cloud.sort_mode = sort;
        }
        if cloud.radix_sort_depth_bits != state.bits {
            cloud.radix_sort_depth_bits = state.bits;
        }
        let compact =
            state.active.compaction && !cpu && cloud.transparency == GaussianTransparency::Sorted;
        if cloud.visibility_compaction != compact {
            cloud.visibility_compaction = compact;
        }
    }
    if needs_cpu_sort {
        // Mode changes and new/late buffers must sort even at a stationary camera.
        for mut trigger in &mut triggers {
            trigger.needs_sort = true;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn update(
    mut commands: Commands,
    state: Res<ViewerSettings>,
    lod: Res<SogLodSettings>,
    budget: Res<GpuBudget>,
    mode: Res<SplatTransparency>,
    scene: Res<SplatScene>,
    streams: Query<&SogStream>,
    quality: Res<crate::diagnostics::quality::SogQualityReport>,
    mut panel: Query<&mut Node, (With<Panel>, Without<AdvancedRow>)>,
    mut advanced: Query<&mut Node, (With<AdvancedRow>, Without<Panel>)>,
    mut labels: Query<(&Label, &mut Text), Without<Summary>>,
    mut summary: Query<&mut Text, (With<Summary>, Without<Label>)>,
    controls: Query<(Entity, &Control)>,
) {
    let streaming = stream_active(&scene, &streams);
    for mut p in &mut panel {
        let display = if state.open {
            Display::Flex
        } else {
            Display::None
        };
        if p.display != display {
            p.display = display;
        }
    }
    if !state.open {
        return;
    }
    for mut row in &mut advanced {
        row.display = if state.advanced {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (label, mut text) in &mut labels {
        let value = match label.0 {
            Field::Transparency => format!(
                "Rendering: {}{}",
                mode.0.name(),
                if mode.0 == GaussianTransparency::Stochastic {
                    " (experimental; visible grain, no resolve)"
                } else {
                    ""
                }
            ),
            Field::Policy => format!("LOD policy: {:?}", lod.lod_policy),
            Field::Footprint => format!(
                "Target footprint: {} px (screen-space only)",
                lod.target_pixels
            ),
            Field::Splats => format!("Requested splat ceiling: {} / scene", lod.splat_budget),
            Field::Cache => format!(
                "CPU decoded-cache ceiling: {} MiB / scene",
                lod.cache_bytes as u64 / MIB
            ),
            Field::Sources => format!(
                "GPU source ceiling: {} MiB globally",
                budget.source_bytes / MIB
            ),
            Field::Selection => format!(
                "GPU selection ceiling: {} MiB / scene",
                budget.selected_bytes_per_scene / MIB
            ),
            Field::Concurrency => format!("Concurrent source loads: {}", lod.concurrent_chunks),
            Field::Visibility => format!(
                "LOD visibility: {:?}{}",
                lod.effective_visibility(),
                if lod.lod_policy == SogLodPolicy::ScreenSpace {
                    " (required by screen-space)"
                } else {
                    ""
                }
            ),
            Field::Advanced => format!(
                "Advanced controls: {}",
                if state.advanced { "shown" } else { "hidden" }
            ),
            Field::Backend => format!(
                "Stream backend: {}; pending {}",
                crate::runtime::backend_name(state.active.backend),
                crate::runtime::backend_name(state.draft.backend)
            ),
            Field::Projection => format!(
                "Projection cache (experimental, packed only): {}",
                state.draft.projection_cache
            ),
            Field::Compaction => format!(
                "Visible-center compaction (sorted radix only): {}",
                state.active.compaction
            ),
            Field::Precision => format!("Radix depth precision: {} bits", state.bits.bits()),
            Field::Sort => format!(
                "Sorting: {} (CPU available for whole-file/Snapshot only)",
                state.sorting(streaming, mode.0)
            ),
            _ => String::new(),
        };
        if text.0 != value {
            **text = value;
        }
    }
    for (entity, control) in &controls {
        let policy_enabled = match control.0 {
            Field::Footprint => lod.lod_policy == SogLodPolicy::ScreenSpace,
            Field::Visibility => lod.lod_policy == SogLodPolicy::ReferenceDistance,
            _ => true,
        };
        if policy_enabled
            && enabled(
                control.0,
                &state,
                if control.0 == Field::Sort {
                    streaming
                } else {
                    streaming || scene.loaded.is_none()
                },
                mode.0,
            )
        {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
    let status = if streaming {
        let status = scene
            .loaded
            .as_ref()
            .and_then(|s| streams.get(s.entity).ok())
            .map(SogStream::status)
            .unwrap();
        format!(
            "Requested {} | effective {} | selected {} | displayed {}\n{}\n{}\n{}",
            status.requested,
            status.effective,
            status.selected,
            status.displayed,
            quality.activity,
            status.limiting_reason,
            quality.memory_summary
        )
    } else if scene.loaded.is_none() {
        "No scene loaded. LOD settings configure the next streamed scene.".into()
    } else {
        "Whole-file import: streamed LOD and memory ceilings do not limit this file. Whole-file memory totals unavailable.".into()
    };
    let value = format!(
        "{}\n{}\n{}\n{}",
        state.notice,
        status,
        quality.sh_description,
        "Allocation counters are partial; they are not total VRAM. Stochastic rendering has no spatial resolve or accumulation."
    );
    for mut text in &mut summary {
        if text.0 != value {
            **text = value.clone();
        }
    }
}

pub(super) fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    mut panel: Query<(&Hovered, &mut ScrollPosition), With<Panel>>,
) {
    for event in wheel.read() {
        for (hover, mut scroll) in &mut panel {
            if hover.get() {
                scroll.0.y = (scroll.0.y
                    - event.y
                        * if event.unit == MouseScrollUnit::Line {
                            30.
                        } else {
                            1.
                        })
                .max(0.);
            }
        }
    }
}

fn drag_scroll(
    event: On<Pointer<Drag>>,
    controls: Query<(), With<Control>>,
    parents: Query<&ChildOf>,
    mut panels: Query<&mut ScrollPosition, With<Panel>>,
) {
    let mut target = event.entity;
    loop {
        if controls.contains(target) {
            return;
        }
        if let Ok(parent) = parents.get(target) {
            target = parent.parent();
        } else {
            break;
        }
    }
    for mut scroll in &mut panels {
        scroll.0.y = (scroll.0.y - event.delta.y).max(0.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MANIFEST: &[u8] = br#"{"lodLevels":1,"filenames":["coarse/meta.json"],"tree":{"bound":{"min":[-1,-1,-1],"max":[1,1,1]},"lods":{"0":{"file":0,"offset":0,"count":1}}}}"#;
    fn app(streaming: bool) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::scene::ScenePlugin,
        ))
        .init_asset::<Font>()
        .init_resource::<ViewerSettings>()
        .init_resource::<SogLodSettings>()
        .init_resource::<GpuBudget>()
        .init_resource::<SplatTransparency>()
        .init_resource::<SplatScene>()
        .init_resource::<crate::diagnostics::quality::SogQualityReport>()
        .add_systems(Startup, setup)
        .add_systems(Update, update);
        if streaming {
            let entity = app
                .world_mut()
                .spawn(
                    SogStream::parse(
                        MANIFEST,
                        "https://example.test/lod-meta.json",
                        "test".into(),
                        "test".into(),
                    )
                    .unwrap(),
                )
                .id();
            app.world_mut().resource_mut::<SplatScene>().loaded = Some(crate::LoadedSplat {
                entity,
                bounds: (Vec3::ZERO, Vec3::ONE),
            });
        }
        app.update();
        app
    }
    fn click(app: &mut App, field: Field, direction: i32) {
        let button = app
            .world_mut()
            .query::<(Entity, &Control)>()
            .iter(app.world())
            .find(|(_, c)| c.0 == field && (direction == 0 || c.1 == direction))
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: button });
        app.update();
    }
    #[test]
    fn budgets_modify_existing_resources_and_stay_within_bounds() {
        let mut app = app(true);
        app.world_mut()
            .resource_mut::<SogLodSettings>()
            .splat_budget = 250_000;
        click(&mut app, Field::Splats, -1);
        assert_eq!(
            app.world().resource::<SogLodSettings>().splat_budget,
            250_000
        );
        click(&mut app, Field::Splats, 1);
        assert_eq!(
            app.world().resource::<SogLodSettings>().splat_budget,
            500_000
        );
        app.world_mut()
            .resource_mut::<SogLodSettings>()
            .concurrent_chunks = 8;
        click(&mut app, Field::Concurrency, 1);
        assert_eq!(
            app.world().resource::<SogLodSettings>().concurrent_chunks,
            8
        );
        click(&mut app, Field::Sources, -1);
        assert_eq!(app.world().resource::<GpuBudget>().source_bytes, 496 * MIB);
    }
    #[test]
    fn advanced_edit_does_not_change_active_backend_until_apply_completes() {
        let mut app = app(true);
        click(&mut app, Field::Backend, 1);
        assert_eq!(
            app.world().resource::<ViewerSettings>().active.backend,
            SogRendererBackend::Packed
        );
        assert_eq!(
            app.world().resource::<ViewerSettings>().draft.backend,
            SogRendererBackend::GpuResident
        );
        click(&mut app, Field::Apply, 0);
        assert!(app.world().resource::<ViewerSettings>().reload_requested);
        click(&mut app, Field::Backend, 1);
        assert_eq!(
            app.world().resource::<ViewerSettings>().draft.backend,
            SogRendererBackend::GpuResident
        );
        app.world_mut()
            .resource_mut::<ViewerSettings>()
            .complete(Err("failed".into()));
        assert_eq!(
            app.world().resource::<ViewerSettings>().active.backend,
            SogRendererBackend::Packed
        );
        assert!(!app.world().resource::<ViewerSettings>().applying);
    }
    #[test]
    fn whole_file_disables_stream_budgets_but_allows_future_backend_configuration() {
        let mut app = app(false);
        let entity = app.world_mut().spawn_empty().id();
        app.world_mut().resource_mut::<SplatScene>().loaded = Some(crate::LoadedSplat {
            entity,
            bounds: (Vec3::ZERO, Vec3::ONE),
        });
        click(&mut app, Field::Splats, -1);
        assert_eq!(
            app.world().resource::<SogLodSettings>().splat_budget,
            4_000_000
        );
        click(&mut app, Field::Backend, 1);
        click(&mut app, Field::Apply, 0);
        assert_eq!(
            app.world().resource::<ViewerSettings>().active.backend,
            SogRendererBackend::GpuResident
        );
        assert!(!app.world().resource::<ViewerSettings>().applying);
        assert!(app.world().get_entity(entity).is_ok());
    }
    #[test]
    fn settings_can_be_chosen_before_loading_and_reset_uses_platform_defaults() {
        let mut app = app(false);
        click(&mut app, Field::Splats, -1);
        assert_eq!(
            app.world().resource::<SogLodSettings>().splat_budget,
            3_750_000
        );
        click(&mut app, Field::Reset, 0);
        assert_eq!(
            app.world().resource::<SogLodSettings>().splat_budget,
            4_000_000
        );
        assert_eq!(
            app.world().resource::<GpuBudget>().source_bytes,
            GpuBudget::default().source_bytes
        );
    }
    #[test]
    fn new_clouds_inherit_precision_and_stochastic_requires_radix() {
        let mut app = app(false);
        app.add_systems(PostUpdate, apply_clouds);
        click(&mut app, Field::Precision, 1);
        click(&mut app, Field::Sort, 1);
        let entity = app
            .world_mut()
            .spawn(CloudSettings {
                global_opacity: 0.4,
                ..default()
            })
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<CloudSettings>(entity)
                .unwrap()
                .radix_sort_depth_bits,
            RadixSortDepthBits::Bits16
        );
        assert_eq!(
            app.world().get::<CloudSettings>(entity).unwrap().sort_mode,
            SortMode::Std
        );
        app.world_mut()
            .get_mut::<CloudSettings>(entity)
            .unwrap()
            .transparency = GaussianTransparency::Stochastic;
        app.update();
        let cloud = app.world().get::<CloudSettings>(entity).unwrap();
        assert_eq!(cloud.sort_mode, SortMode::Radix);
        assert!(cloud.stochastic());
        assert_eq!(cloud.global_opacity, 0.4);
    }
    #[test]
    fn stationary_cpu_sort_orders_replacements_even_when_handles_arrive_late() {
        use bevy_gaussian_splatting::sort::{
            SortConfig, SortTrigger, SortedEntries, SortedEntriesHandle, std_sort::std_sort,
        };
        use bevy_gaussian_splatting::{
            Gaussian3d, GaussianCamera, PlanarGaussian3d, PlanarGaussian3dHandle,
        };
        let mut app = app(false);
        app.init_asset::<PlanarGaussian3d>()
            .init_asset::<SortedEntries>()
            .init_resource::<SortConfig>()
            .add_systems(Update, std_sort::<Gaussian3d>)
            .add_systems(PostUpdate, apply_clouds);
        app.world_mut().resource_mut::<ViewerSettings>().cpu_sort = true;
        app.world_mut()
            .spawn((GaussianCamera::default(), SortTrigger::default()));
        for late_handle in [false, true] {
            let cloud = PlanarGaussian3d::from(vec![
                Gaussian3d {
                    position_visibility: [0., 0., -1., 1.].into(),
                    ..default()
                },
                Gaussian3d {
                    position_visibility: [0., 0., -4., 1.].into(),
                    ..default()
                },
            ]);
            let handle = app
                .world_mut()
                .resource_mut::<Assets<PlanarGaussian3d>>()
                .add(cloud);
            let entity = app
                .world_mut()
                .spawn((
                    PlanarGaussian3dHandle(handle),
                    CloudSettings::default(),
                    GlobalTransform::IDENTITY,
                ))
                .id();
            if late_handle {
                // The previous cloud can consume the mode-switch trigger before
                // upstream inserts this replacement's sorted-entry handle.
                app.update();
                app.update();
            }
            let entries = app
                .world_mut()
                .resource_mut::<Assets<SortedEntries>>()
                .add(SortedEntries::new(1, 2));
            app.world_mut()
                .entity_mut(entity)
                .insert(SortedEntriesHandle(entries.clone()));
            app.update();
            app.update();
            let sorted = app
                .world()
                .resource::<Assets<SortedEntries>>()
                .get(&entries)
                .unwrap();
            assert_eq!(
                sorted
                    .sorted
                    .iter()
                    .map(|entry| entry.index)
                    .collect::<Vec<_>>(),
                [1, 0],
                "stationary CPU view must sort a new cloud far-to-near"
            );
        }
    }
}
