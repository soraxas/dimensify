use super::ViewerOptions;
use crate::loading::{LoadingStage, WholeFileTimings};
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
    metadata_url: Option<String>,
}

impl ReadResult {
    fn download(
        ticket: u64,
        resource: &super::remote::ResourceUrl,
        result: Result<super::remote::Download, String>,
        up: SourceUp,
    ) -> Self {
        let mut metadata_url = None;
        let file = result.map(|download| {
            if !matches!(
                resource.format,
                super::remote::DataFormat::Ply | super::remote::DataFormat::Sog
            ) {
                metadata_url = Some(download.url.into());
            }
            Some((resource.name.clone(), download.bytes, up))
        });
        Self {
            ticket,
            file,
            metadata_url,
        }
    }
}

#[derive(Resource)]
pub(super) struct LocalFiles {
    sender: mpsc::Sender<ReadResult>,
    receiver: Mutex<mpsc::Receiver<ReadResult>>,
    ticket: u64,
    pub(super) timings: WholeFileTimings,
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
            timings: WholeFileTimings::default(),
            notice: None,
            hovering: false,
        }
    }
}

impl LocalFiles {
    /// Download a direct remote resource through the same latest-selection queue.
    pub(super) fn open_url(&mut self, input: &str, up: SourceUp) {
        self.open_url_with_format(input, up, super::remote::DataFormat::Auto);
    }

    pub(super) fn open_url_with_format(
        &mut self,
        input: &str,
        up: SourceUp,
        format: super::remote::DataFormat,
    ) {
        self.ticket += 1;
        let resource = match if format == super::remote::DataFormat::Auto {
            super::remote::ResourceUrl::parse(input)
        } else {
            super::remote::ResourceUrl::parse_with_format(input, format)
        } {
            Ok(resource) => resource,
            Err(error) => {
                self.notice = Some(error);
                return;
            }
        };
        let ticket = self.ticket;
        let sender = self.sender.clone();
        self.notice = Some(format!("Downloading {}...", resource.name));
        let timings = self.timings.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || {
            let _read = timings.start(LoadingStage::InputRead);
            let result = super::remote::fetch(&resource);
            let _ = sender.send(ReadResult::download(ticket, &resource, result, up));
        });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let _read = timings.start(LoadingStage::InputRead);
            let result = super::remote::fetch(&resource).await;
            let _ = sender.send(ReadResult::download(ticket, &resource, result, up));
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
            let timings = self.timings.clone();
            AsyncComputeTaskPool::get()
                .spawn(async move {
                    let file = rfd::AsyncFileDialog::new()
                        .add_filter("Gaussian splats", &["ply", "sog"])
                        .pick_file()
                        .await;
                    let result = file
                        .map(|f| {
                            timings.measure(LoadingStage::InputRead, || {
                                read_file(f.path().to_owned(), up)
                            })
                        })
                        .transpose();
                    let _ = sender.send(ReadResult {
                        ticket,
                        metadata_url: None,
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
        let timings = self.timings.clone();
        std::thread::spawn(move || {
            let _ = sender.send(ReadResult {
                ticket,
                metadata_url: None,
                file: timings
                    .measure(LoadingStage::InputRead, || read_file(path, up))
                    .map(Some),
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
        let timings = self.timings.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let _read = timings.start(LoadingStage::InputRead);
            let result = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                .await
                .map(|buffer| Some((file.name(), js_sys::Uint8Array::new(&buffer).to_vec(), up)))
                .map_err(|error| format!("{}: {error:?}", file.name()));
            let _ = sender.send(ReadResult {
                ticket,
                metadata_url: None,
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
                if let Some(url) = result.metadata_url {
                    streams.write(super::streaming::LoadStream { url, bytes });
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
    fn downloads_route_metadata_with_the_final_url_and_files_with_the_requested_name() {
        use super::super::remote::{DataFormat, Download, ResourceUrl};
        for (format, stream) in [
            (DataFormat::Auto, true),
            (DataFormat::SogUnbundled, true),
            (DataFormat::SogStream, true),
            (DataFormat::Ply, false),
            (DataFormat::Sog, false),
        ] {
            let resource =
                ResourceUrl::parse_with_format("https://source.test/scene.json", format).unwrap();
            let final_url = "https://cdn.test/redirected/metadata.json?token=kept";
            let result = ReadResult::download(
                0,
                &resource,
                Ok(Download {
                    bytes: vec![1, 2, 3],
                    url: url::Url::parse(final_url).unwrap(),
                }),
                SourceUp::Z,
            );
            let files = LocalFiles::default();
            files.sender.send(result).unwrap();
            let mut app = App::new();
            app.insert_resource(files)
                .insert_resource(ViewerOptions {
                    up: SourceUp::Y,
                    ..default()
                })
                .add_message::<LoadSplat>()
                .add_message::<super::super::streaming::LoadStream>()
                .add_systems(Update, receive);
            app.update();
            let splats: Vec<_> = app
                .world()
                .resource::<Messages<LoadSplat>>()
                .iter_current_update_messages()
                .collect();
            let streams: Vec<_> = app
                .world()
                .resource::<Messages<super::super::streaming::LoadStream>>()
                .iter_current_update_messages()
                .collect();
            if stream {
                assert!(splats.is_empty());
                assert_eq!(streams.len(), 1);
                assert_eq!(streams[0].url, final_url);
                assert_eq!(streams[0].bytes, vec![1, 2, 3]);
            } else {
                assert!(streams.is_empty());
                assert_eq!(splats.len(), 1);
                assert_eq!(splats[0].name, resource.name);
                assert_eq!(splats[0].up, SourceUp::Y);
                assert_eq!(splats[0].bytes.as_ref(), &[1, 2, 3]);
            }
        }
    }

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
                metadata_url: None,
                file: Ok(Some(("https:x.sog".into(), vec![2], SourceUp::Z))),
            })
            .unwrap();
        files
            .sender
            .send(ReadResult {
                ticket: 1,
                metadata_url: None,
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
        assert_eq!(requests[0].name, "https:x.sog");
        assert_eq!(requests[0].up, SourceUp::NegativeX);
        assert!(app.world().resource::<LocalFiles>().notice.is_none());
    }
}
