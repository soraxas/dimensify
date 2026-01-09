use dimensify_protocol::WorldCommand;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum StreamSet {
    Load,
}
use bevy::prelude::*;

#[derive(Clone, Debug)]
pub enum DataSource {
    Local,
    FileReplay { path: String },
    Db { addr: String },
}

#[derive(Resource, Clone, Debug)]
pub struct TelemetrySettings {
    pub source: DataSource,
}

impl Default for TelemetrySettings {
    fn default() -> Self {
        let source = match std::env::var("DIMENSIFY_DATA_SOURCE")
            .unwrap_or_else(|_| "local".to_string())
            .as_str()
        {
            "file" => std::env::var("DIMENSIFY_FILE")
                .ok()
                .map(|path| DataSource::FileReplay { path })
                .unwrap_or(DataSource::Local),
            "db" => std::env::var("DIMENSIFY_DB_ADDR")
                .ok()
                .map(|addr| DataSource::Db { addr })
                .unwrap_or(DataSource::Local),
            _ => DataSource::Local,
        };
        Self { source }
    }
}

#[derive(Resource, Default)]
pub struct CommandLog {
    pub commands: Vec<WorldCommand>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<TelemetrySettings>()
        .init_resource::<CommandLog>();

    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(Startup, load_file_replay.in_set(StreamSet::Load));
}

#[cfg(not(target_arch = "wasm32"))]
fn load_file_replay(settings: Res<TelemetrySettings>, mut command_log: ResMut<CommandLog>) {
    let DataSource::FileReplay { path } = &settings.source else {
        return;
    };
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            bevy::log::error!("Failed to read replay file {}: {}", path, err);
            return;
        }
    };

    let mut commands = Vec::new();
    for (line_no, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<WorldCommand>(line) {
            Ok(command) => commands.push(command),
            Err(err) => {
                bevy::log::warn!("Failed to parse command at line {}: {}", line_no + 1, err);
            }
        }
    }

    command_log.commands = commands;
    bevy::log::info!("Loaded {} replay commands", command_log.commands.len());
}
