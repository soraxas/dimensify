use std::collections::VecDeque;

use bevy::prelude::*;
use dimensify_protocol::TelemetryEvent;

/// Telemetry storage is separate from ECS. ECS renders the current time window.
/// Intended for Rerun/Arrow-backed telemetry sources.
#[derive(Default, Resource)]
pub struct TelemetryStore {
    events: VecDeque<TelemetryEvent>,
    max_events: usize,
}

impl TelemetryStore {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: VecDeque::new(),
            max_events,
        }
    }

    pub fn push(&mut self, event: TelemetryEvent) {
        self.events.push_back(event);
        while self.events.len() > self.max_events {
            self.events.pop_front();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &TelemetryEvent> {
        self.events.iter()
    }
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
        .init_resource::<TelemetryStore>();

    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(Startup, load_file_replay);
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
