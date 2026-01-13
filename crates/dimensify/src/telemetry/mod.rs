use std::collections::HashMap;

use bevy::prelude::*;
use dimensify_protocol::{TelemetryEvent, TelemetryPayload};

/// Telemetry storage is separate from ECS. ECS renders the current time window.
/// Intended for Rerun/Arrow-backed telemetry sources.
#[derive(Default, Resource)]
pub struct TelemetryStore {
    events: Vec<TelemetryEvent>,
    index: TelemetryIndex,
    max_events: usize,
    revision: u64,
}

impl TelemetryStore {
    /// Create a bounded telemetry store.
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Vec::new(),
            index: TelemetryIndex::default(),
            max_events,
            revision: 0,
        }
    }

    /// Push a new telemetry event, dropping the oldest when full.
    pub fn push(&mut self, event: TelemetryEvent) {
        self.events.push(event);
        let idx = self.events.len() - 1;
        self.index.insert(idx, &self.events);

        if self.events.len() > self.max_events {
            self.events.remove(0);
            self.index.rebuild(&self.events);
        }

        self.revision = self.revision.wrapping_add(1);
    }

    /// Iterate over stored telemetry events.
    pub fn iter(&self) -> impl Iterator<Item = &TelemetryEvent> {
        self.events.iter()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn events(&self) -> &[TelemetryEvent] {
        &self.events
    }

    pub fn timeline_bounds(&self, timeline: &str) -> Option<(f64, f64)> {
        let mut min: Option<f64> = None;
        let mut max: Option<f64> = None;
        for event in &self.events {
            if event.time.timeline != timeline {
                continue;
            }
            min = Some(match min {
                Some(v) => v.min(event.time.value),
                None => event.time.value,
            });
            max = Some(match max {
                Some(v) => v.max(event.time.value),
                None => event.time.value,
            });
        }
        match (min, max) {
            (Some(min), Some(max)) => Some((min, max)),
            _ => None,
        }
    }

    /// Return the latest event at or before the given time.
    pub fn latest_at(&self, timeline: &str, path: &str, time: f64) -> Option<&TelemetryEvent> {
        let Some(paths) = self.index.timelines.get(timeline) else {
            return None;
        };
        let Some(indices) = paths.get(path) else {
            return None;
        };

        let pos = indices.binary_search_by(|idx| self.events[*idx].time.value.total_cmp(&time));

        match pos {
            Ok(i) => Some(&self.events[indices[i]]),
            Err(0) => None,
            Err(i) => Some(&self.events[indices[i - 1]]),
        }
    }

    /// List known paths for a timeline.
    pub fn paths(&self, timeline: &str) -> Vec<String> {
        self.index
            .timelines
            .get(timeline)
            .map(|paths| paths.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Default)]
struct TelemetryIndex {
    timelines: HashMap<String, HashMap<String, Vec<usize>>>,
}

impl TelemetryIndex {
    fn insert(&mut self, idx: usize, events: &[TelemetryEvent]) {
        let event = &events[idx];
        let paths = self
            .timelines
            .entry(event.time.timeline.clone())
            .or_default();
        let indices = paths.entry(event.path.clone()).or_default();

        let insert_at = indices
            .binary_search_by(|existing| events[*existing].time.value.total_cmp(&event.time.value))
            .unwrap_or_else(|i| i);
        indices.insert(insert_at, idx);
    }

    fn rebuild(&mut self, events: &[TelemetryEvent]) {
        self.timelines.clear();
        for (idx, event) in events.iter().enumerate() {
            let paths = self
                .timelines
                .entry(event.time.timeline.clone())
                .or_default();
            let indices = paths.entry(event.path.clone()).or_default();
            indices.push(idx);
        }
        for paths in self.timelines.values_mut() {
            for indices in paths.values_mut() {
                indices.sort_by(|a, b| events[*a].time.value.total_cmp(&events[*b].time.value));
            }
        }
    }
}

/// Controls the telemetry playback timeline.
#[derive(Resource, Clone, Debug)]
pub struct TelemetryPlayback {
    pub timeline: String,
    pub time: f64,
    pub mode: TelemetryPlaybackMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TelemetryPlaybackMode {
    Live,
    Fixed,
}

impl Default for TelemetryPlayback {
    fn default() -> Self {
        let timeline = std::env::var("DIMENSIFY_TELEMETRY_TIMELINE")
            .unwrap_or_else(|_| "sim_time".to_string());
        let mode = match std::env::var("DIMENSIFY_TELEMETRY_MODE")
            .unwrap_or_else(|_| "live".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "fixed" => TelemetryPlaybackMode::Fixed,
            _ => TelemetryPlaybackMode::Live,
        };
        let time = std::env::var("DIMENSIFY_TELEMETRY_TIME")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.0);
        Self {
            timeline,
            time,
            mode,
        }
    }
}

/// Latest telemetry values for a timeline at a specific time.
#[derive(Default, Resource)]
pub struct TelemetryState {
    pub timeline: String,
    pub time: f64,
    pub latest: HashMap<String, TelemetryEvent>,
    last_revision: u64,
}

#[derive(Resource, Clone, Debug)]
pub struct TelemetryRecordingState {
    pub enabled: bool,
    pub path: String,
    pub error: Option<String>,
}

impl Default for TelemetryRecordingState {
    fn default() -> Self {
        let path = std::env::var("DIMENSIFY_TELEMETRY_RRD")
            .unwrap_or_else(|_| "dimensify.rrd".to_string());
        Self {
            enabled: false,
            path,
            error: None,
        }
    }
}

/// Controls whether telemetry should drive ECS state during playback.
#[derive(Resource, Clone, Debug)]
pub struct TelemetryEcsSync {
    pub enabled: bool,
}

impl Default for TelemetryEcsSync {
    fn default() -> Self {
        let enabled = std::env::var("DIMENSIFY_TELEMETRY_ECS_SYNC")
            .map(|value| matches!(value.as_str(), "1" | "true" | "on"))
            .unwrap_or(false);
        Self { enabled }
    }
}

#[derive(Resource, Default)]
struct TelemetryEcsSyncState {
    last_revision: u64,
    last_time: f64,
    last_timeline: String,
}

#[derive(Clone, Debug)]
pub enum TelemetrySourceKind {
    Local,
    FileReplay { path: String },
}

#[derive(Resource, Clone, Debug)]
pub struct TelemetrySettings {
    pub source: TelemetrySourceKind,
    pub max_events: usize,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        let source = match std::env::var("DIMENSIFY_TELEMETRY_SOURCE")
            .unwrap_or_else(|_| "local".to_string())
            .as_str()
        {
            "file" => std::env::var("DIMENSIFY_TELEMETRY_FILE")
                .ok()
                .map(|path| TelemetrySourceKind::FileReplay { path })
                .unwrap_or(TelemetrySourceKind::Local),
            _ => TelemetrySourceKind::Local,
        };
        Self {
            source,
            max_events: 10_000,
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<TelemetrySettings>()
        .init_resource::<TelemetryStore>()
        .init_resource::<TelemetryPlayback>()
        .init_resource::<TelemetryState>()
        .init_resource::<TelemetryRecordingState>()
        .init_resource::<TelemetryEcsSync>()
        .init_resource::<TelemetryEcsSyncState>()
        .add_systems(Update, update_playback_time)
        .add_systems(Update, refresh_telemetry_state)
        .add_systems(
            Update,
            apply_telemetry_to_ecs.after(refresh_telemetry_state),
        );

    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(Startup, load_file_replay);

    #[cfg(feature = "telemetry_rrd")]
    {
        app.init_resource::<TelemetryRrdRecorder>()
            .add_systems(Update, record_telemetry_to_rrd);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_file_replay(settings: Res<TelemetrySettings>, mut store: ResMut<TelemetryStore>) {
    let TelemetrySourceKind::FileReplay { path } = &settings.source else {
        return;
    };
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            bevy::log::error!("Failed to read telemetry file {}: {}", path, err);
            return;
        }
    };

    store.max_events = settings.max_events;
    for (line_no, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<TelemetryEvent>(line) {
            Ok(event) => store.push(event),
            Err(err) => {
                bevy::log::warn!("Failed to parse telemetry at line {}: {}", line_no + 1, err);
            }
        }
    }
    bevy::log::info!("Loaded {} telemetry events", store.events.len());
}

/// Telemetry source for live streams or replay files (e.g., Rerun/rrd).
pub trait TelemetrySource {
    fn poll(&mut self) -> Vec<TelemetryEvent>;
}

/// Placeholder source used until a concrete Rerun/Arrow reader is wired.
pub struct EmptyTelemetrySource;

impl TelemetrySource for EmptyTelemetrySource {
    fn poll(&mut self) -> Vec<TelemetryEvent> {
        Vec::new()
    }
}

fn update_playback_time(time: Res<Time>, mut playback: ResMut<TelemetryPlayback>) {
    if playback.mode == TelemetryPlaybackMode::Live {
        playback.time = time.elapsed_secs_f64();
    }
}

fn refresh_telemetry_state(
    store: Res<TelemetryStore>,
    playback: Res<TelemetryPlayback>,
    mut state: ResMut<TelemetryState>,
) {
    if store.revision() == state.last_revision
        && state.time == playback.time
        && state.timeline == playback.timeline
    {
        return;
    }

    state.timeline = playback.timeline.clone();
    state.time = playback.time;
    state.latest.clear();

    for path in store.paths(&playback.timeline) {
        if let Some(event) = store.latest_at(&playback.timeline, &path, playback.time) {
            state.latest.insert(path, event.clone());
        }
    }

    state.last_revision = store.revision();
}

fn apply_telemetry_to_ecs(
    sync: Res<TelemetryEcsSync>,
    store: Res<TelemetryStore>,
    state: Res<TelemetryState>,
    mut sync_state: ResMut<TelemetryEcsSyncState>,
    mut query: Query<(Entity, &Name, &mut Transform)>,
) {
    if !sync.enabled {
        return;
    }

    if sync_state.last_revision == store.revision()
        && sync_state.last_time == state.time
        && sync_state.last_timeline == state.timeline
    {
        return;
    }

    let mut by_name = HashMap::new();
    for (entity, name, _) in &query {
        by_name.insert(name.as_str().to_string(), entity);
    }

    for (path, event) in &state.latest {
        if let Some((entity_name, field)) = parse_entity_transform_path(path) {
            let Some(entity) = by_name.get(&entity_name) else {
                continue;
            };
            let Ok((_, _, mut transform)) = query.get_mut(*entity) else {
                continue;
            };
            match field.as_str() {
                "translation" | "position" => {
                    if let TelemetryPayload::Vec3 { value } = &event.payload {
                        let v: Vec3 = value.clone().into();
                        transform.translation = v;
                    }
                }
                "rotation" => {
                    if let TelemetryPayload::Vec4 { value } = &event.payload {
                        let v: [f32; 4] = value.clone().into();
                        transform.rotation = Quat::from_xyzw(v[0], v[1], v[2], v[3]);
                    }
                }
                "scale" => {
                    if let TelemetryPayload::Vec3 { value } = &event.payload {
                        let v: Vec3 = value.clone().into();
                        transform.scale = v;
                    }
                }
                _ => {}
            }
        }
    }

    sync_state.last_revision = store.revision();
    sync_state.last_time = state.time;
    sync_state.last_timeline = state.timeline.clone();
}

fn parse_entity_transform_path(path: &str) -> Option<(String, String)> {
    let mut parts = path.split('/');
    if parts.next()? != "entity" {
        return None;
    }
    let entity_name = parts.next()?.to_string();
    if parts.next()? != "transform" {
        return None;
    }
    let field = parts.next()?.to_string();
    Some((entity_name, field))
}

#[cfg(feature = "telemetry_rrd")]
#[derive(Default, Resource)]
pub struct TelemetryRrdRecorder {
    recorder: Option<rerun::RecordingStream>,
    last_index: usize,
}

#[cfg(feature = "telemetry_rrd")]
fn record_telemetry_to_rrd(
    store: Res<TelemetryStore>,
    mut recording: ResMut<TelemetryRecordingState>,
    mut recorder: ResMut<TelemetryRrdRecorder>,
) {
    if !recording.enabled {
        recorder.recorder = None;
        recorder.last_index = store.len();
        return;
    }

    if recorder.recorder.is_none() {
        let result = rerun::RecordingStreamBuilder::new("dimensify").save(&recording.path);
        match result {
            Ok(rec) => {
                recorder.recorder = Some(rec);
                recorder.last_index = 0;
                recording.error = None;
            }
            Err(err) => {
                recording.error = Some(err.to_string());
                bevy::log::error!("Failed to create RRD recorder: {}", err);
                recording.enabled = false;
                return;
            }
        }
    }

    let Some(rec) = recorder.recorder.as_ref() else {
        return;
    };

    for event in &store.events()[recorder.last_index..] {
        log_event_to_rerun(rec, event);
    }
    recorder.last_index = store.len();
}

#[cfg(feature = "telemetry_rrd")]
fn log_event_to_rerun(rec: &rerun::RecordingStream, event: &TelemetryEvent) {
    rec.set_duration_secs(event.time.timeline.clone(), event.time.value);
    let path = event.path.as_str();

    match &event.payload {
        TelemetryPayload::Scalar { value } => {
            let _ = rec.log(path, &rerun::Scalars::single(*value));
        }
        TelemetryPayload::Vec2 { value } => {
            let v = value.0;
            let _ = rec.log(path, &rerun::Points2D::new([(v[0], v[1])]));
        }
        TelemetryPayload::Vec3 { value } => {
            let v = value.0;
            let _ = rec.log(path, &rerun::Points3D::new([(v[0], v[1], v[2])]));
        }
        TelemetryPayload::Vec4 { value } => {
            let v = value.0;
            let _ = rec.log(path, &rerun::Scalars::new([v[0], v[1], v[2], v[3]]));
        }
        TelemetryPayload::Text { value } => {
            let _ = rec.log(path, &rerun::TextDocument::new(value.clone()));
        }
        TelemetryPayload::Blob { .. } => {
            bevy::log::warn!(
                "Telemetry blob payload is not recorded to RRD yet (path={})",
                event.path
            );
        }
    }
}
