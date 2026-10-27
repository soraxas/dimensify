use super::ViewerOptions;
use crate::{LoadSplat, SourceUp};
use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::{tasks::AsyncComputeTaskPool, window::FileDragAndDrop};
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};

type SelectedFile = Result<Option<(String, Vec<u8>, SourceUp)>, String>;
struct ReadResult {
    ticket: u64,
    file: SelectedFile,
}

#[derive(Resource)]
pub(super) struct LocalFiles {
    sender: mpsc::Sender<ReadResult>,
    receiver: Mutex<mpsc::Receiver<ReadResult>>,
    ticket: u64,
    /// Status while selecting/reading a file, or a local filesystem failure.
    pub notice: Option<String>,
    /// Whether a file is hovering over the viewer window or canvas.
    pub hovering: bool,
}
impl Default for LocalFiles {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver: Mutex::new(receiver),
            ticket: 0,
            notice: None,
            hovering: false,
        }
    }
}

impl LocalFiles {
    /// Download a direct remote resource through the same latest-selection queue.
    pub(super) fn open_url(&mut self, input: &str, up: SourceUp) {
        self.ticket += 1;
        let resource = match super::remote::ResourceUrl::parse(input) {
            Ok(resource) => resource,
            Err(error) => {
                self.notice = Some(error);
                return;
            }
        };
        let ticket = self.ticket;
        let sender = self.sender.clone();
        self.notice = Some(format!("Downloading {}...", resource.name));
        let name = if resource.name.to_ascii_lowercase().ends_with(".json") {
            resource.url.to_string()
        } else {
            resource.name.clone()
        };
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let file = super::remote::fetch(&resource).map(|bytes| Some((name, bytes, up)));
            let _ = sender.send(ReadResult { ticket, file });
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let file = super::remote::fetch(&resource)
                .await
                .map(|bytes| Some((name, bytes, up)));
            let _ = sender.send(ReadResult { ticket, file });
        });
    }

    /// Start a system picker without blocking the render loop.
    pub(super) fn choose(&mut self, up: SourceUp) {
        self.ticket += 1;
        let ticket = self.ticket;
        self.notice = Some("Choose a Gaussian PLY or bundled SOG file...".into());
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ticket;
            if let Err(error) = super::web::choose(up) {
                self.notice = Some(format!("Could not open file picker: {error:?}"));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let sender = self.sender.clone();
            AsyncComputeTaskPool::get()
                .spawn(async move {
                    let file = rfd::AsyncFileDialog::new()
                        .add_filter("Gaussian splats", &["ply", "sog"])
                        .pick_file()
                        .await;
                    let result = file.map(|f| read_file(f.path().to_owned(), up)).transpose();
                    let _ = sender.send(ReadResult {
                        ticket,
                        file: result,
                    });
                })
                .detach();
        }
    }

    /// Read a dropped/command-line file on a background thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn open(&mut self, path: PathBuf, up: SourceUp) {
        self.ticket += 1;
        let ticket = self.ticket;
        let sender = self.sender.clone();
        self.notice = Some(format!("Reading {}...", path.display()));
        std::thread::spawn(move || {
            let _ = sender.send(ReadResult {
                ticket,
                file: read_file(path, up).map(Some),
            });
        });
    }

    /// Read browser File bytes and publish them through the shared completion path.
    #[cfg(target_arch = "wasm32")]
    pub(super) fn open_browser(&mut self, file: web_sys::File, up: SourceUp) {
        self.ticket += 1;
        let ticket = self.ticket;
        let sender = self.sender.clone();
        if file.size() > super::super::decode::MAX_BYTES as f64 {
            self.notice = Some("File exceeds the 256 MiB input limit".into());
            return;
        }
        self.notice = Some(format!("Reading {}...", file.name()));
        wasm_bindgen_futures::spawn_local(async move {
            let result = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                .await
                .map(|buffer| Some((file.name(), js_sys::Uint8Array::new(&buffer).to_vec(), up)))
                .map_err(|error| format!("{}: {error:?}", file.name()));
            let _ = sender.send(ReadResult {
                ticket,
                file: result,
            });
        });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: PathBuf, up: SourceUp) -> Result<(String, Vec<u8>, SourceUp), String> {
    use std::io::Read;
    let file = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > super::super::decode::MAX_BYTES as u64 {
        return Err("File exceeds the 256 MiB input limit".into());
    }
    let mut bytes = Vec::new();
    file.take(super::super::decode::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > super::super::decode::MAX_BYTES {
        return Err("File exceeds the 256 MiB input limit".into());
    }
    Ok((
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        bytes,
        up,
    ))
}

/// Open a command-line file after the app and task pools are initialized.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn initial_file(options: Res<ViewerOptions>, mut files: ResMut<LocalFiles>) {
    if let Some(path) = &options.path {
        files.open(path.clone(), options.up);
    }
}

/// Route native window drops through the same background reader as the picker.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn drops(
    mut events: MessageReader<FileDragAndDrop>,
    options: Res<ViewerOptions>,
    mut files: ResMut<LocalFiles>,
) {
    let mut paths = Vec::new();
    for event in events.read() {
        match event {
            FileDragAndDrop::DroppedFile { path_buf, .. } => {
                files.hovering = false;
                paths.push(path_buf.clone());
            }
            FileDragAndDrop::HoveredFile { .. } => files.hovering = true,
            FileDragAndDrop::HoveredFileCanceled { .. } => files.hovering = false,
        }
    }
    if paths.len() == 1 {
        files.open(paths.remove(0), options.up);
    } else if paths.len() > 1 {
        files.notice = Some("Drop one PLY or SOG file at a time".into());
    }
}

/// Publish completed local reads, ignoring results superseded by another selection.
pub(super) fn receive(
    mut files: ResMut<LocalFiles>,
    options: Res<ViewerOptions>,
    mut requests: MessageWriter<LoadSplat>,
    mut streams: MessageWriter<super::streaming::LoadStream>,
) {
    let results: Vec<_> = files
        .receiver
        .lock()
        .expect("file results")
        .try_iter()
        .collect();
    for result in results {
        if result.ticket != files.ticket {
            continue;
        }
        match result.file {
            Ok(Some((name, bytes, _))) => {
                files.notice = None;
                if name.starts_with("http")
                    && super::remote::ResourceUrl::parse(&name)
                        .is_ok_and(|url| url.name.to_ascii_lowercase().ends_with(".json"))
                {
                    streams.write(super::streaming::LoadStream { url: name, bytes });
                    continue;
                }
                requests.write(LoadSplat {
                    name,
                    bytes: bytes.into(),
                    up: options.up,
                });
            }
            Ok(None) => files.notice = None,
            Err(error) => files.notice = Some(format!("Could not open {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superseded_downloads_are_ignored_and_current_axis_wins_at_completion() {
        let mut files = LocalFiles {
            ticket: 2,
            ..default()
        };
        files
            .sender
            .send(ReadResult {
                ticket: 2,
                file: Ok(Some(("new.sog".into(), vec![2], SourceUp::Z))),
            })
            .unwrap();
        files
            .sender
            .send(ReadResult {
                ticket: 1,
                file: Ok(Some(("old.ply".into(), vec![1], SourceUp::Y))),
            })
            .unwrap();
        files.notice = Some("Downloading new.sog...".into());
        let mut app = App::new();
        app.insert_resource(files)
            .insert_resource(ViewerOptions {
                up: SourceUp::NegativeX,
                ..default()
            })
            .add_message::<LoadSplat>()
            .add_message::<super::super::streaming::LoadStream>()
            .add_systems(Update, receive);
        app.update();
        let requests = app.world().resource::<Messages<LoadSplat>>();
        let requests: Vec<_> = requests.iter_current_update_messages().collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].name, "new.sog");
        assert_eq!(requests[0].up, SourceUp::NegativeX);
        assert!(app.world().resource::<LocalFiles>().notice.is_none());
    }
}
