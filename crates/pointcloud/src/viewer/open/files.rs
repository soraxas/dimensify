//! Platform file selection feeds the same source replacement path as URLs.
use super::*;
use std::sync::{Mutex, mpsc};

type Selection = (u64, PointcloudUp, Result<Option<PointcloudSource>, String>);
#[derive(Resource)]
pub(in crate::viewer) struct Files {
    sender: mpsc::Sender<Selection>,
    receiver: Mutex<mpsc::Receiver<Selection>>,
    pub(super) ticket: u64,
    pub(in crate::viewer) notice: Option<String>,
    pub(super) hovering: bool,
}
impl Default for Files {
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
impl Files {
    pub(super) fn invalidate(&mut self) {
        self.ticket += 1;
        self.notice = None;
    }
    pub(in crate::viewer) fn choose(&mut self, folder: bool, up: PointcloudUp) {
        self.invalidate();
        self.notice = Some(
            if folder {
                "Choose a Potree dataset folder..."
            } else {
                "Choose a PLY file..."
            }
            .into(),
        );
        #[cfg(target_arch = "wasm32")]
        if let Err(e) = super::web::choose(folder, up, self.ticket) {
            self.notice = Some(format!("Cannot open picker: {e:?}"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let ticket = self.ticket;
            let sender = self.sender.clone();
            bevy::tasks::AsyncComputeTaskPool::get()
                .spawn(async move {
                    let dialog = rfd::AsyncFileDialog::new();
                    let selected = if folder {
                        dialog.pick_folder().await
                    } else {
                        dialog.add_filter("Point cloud", &["ply"]).pick_file().await
                    };
                    let result = selected
                        .map(|file| native_source(file.path().to_owned()))
                        .transpose();
                    let _ = sender.send((ticket, up, result));
                })
                .detach();
        }
    }
    #[cfg(target_arch = "wasm32")]
    pub(super) fn browser_selection(
        &mut self,
        ticket: Option<u64>,
        selected: Vec<(String, crate::PointcloudFile)>,
        up: PointcloudUp,
    ) {
        let ticket = match ticket {
            Some(ticket) if ticket != self.ticket => return,
            Some(ticket) => ticket,
            None => {
                self.invalidate();
                self.ticket
            }
        };
        let result = if selected.is_empty() {
            Ok(None)
        } else {
            selected_source(selected).map(Some)
        };
        let _ = self.sender.send((ticket, up, result));
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn selected_source(
    mut selected: Vec<(String, crate::PointcloudFile)>,
) -> Result<PointcloudSource, String> {
    if selected.len() == 1 && selected[0].0.to_ascii_lowercase().ends_with(".ply") {
        let (name, file) = selected.remove(0);
        return Ok(PointcloudSource::PlyFile { name, file });
    }
    crate::PotreeFiles::new(selected).map(PointcloudSource::PotreeFiles)
}
#[cfg(not(target_arch = "wasm32"))]
fn native_source(path: std::path::PathBuf) -> Result<PointcloudSource, String> {
    if path.is_dir() || path.file_name().is_some_and(|n| n == "metadata.json") {
        Ok(PointcloudSource::PotreePath(path))
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ply"))
    {
        Ok(PointcloudSource::PlyPath(path))
    } else {
        Err("Open a PLY file or a whole Potree 2 folder".into())
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn drops(
    mut events: MessageReader<bevy::window::FileDragAndDrop>,
    mut files: ResMut<Files>,
    panel: Res<OpenPanel>,
) {
    let mut paths = Vec::new();
    for event in events.read() {
        match event {
            bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } => {
                files.hovering = false;
                paths.push(path_buf.clone());
            }
            bevy::window::FileDragAndDrop::HoveredFile { .. } => files.hovering = true,
            bevy::window::FileDragAndDrop::HoveredFileCanceled { .. } => files.hovering = false,
        }
    }
    if paths.is_empty() {
        return;
    }
    files.invalidate();
    let result = if paths.len() == 1 {
        native_source(paths.remove(0)).map(Some)
    } else {
        Err("Drop one PLY file or one complete Potree folder".into())
    };
    let _ = files.sender.send((files.ticket, panel.up, result));
}
pub(super) fn receive(
    mut files: ResMut<Files>,
    mut panel: ResMut<OpenPanel>,
    mut requests: MessageWriter<Request>,
) {
    let results: Vec<_> = files
        .receiver
        .lock()
        .expect("file results")
        .try_iter()
        .collect();
    for (ticket, up, result) in results {
        if ticket != files.ticket {
            continue;
        }
        files.notice = None;
        match result {
            Ok(Some(source)) => {
                requests.write(Request::Source(Box::new(source), up, ticket));
            }
            Ok(None) => {}
            Err(error) => {
                files.notice = Some(error.clone());
                panel.error = Some(error);
                panel.open = true;
            }
        }
    }
}

#[derive(Component)]
pub(super) struct DropOverlay;
pub(super) fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            DropOverlay,
            GlobalZIndex(60),
            Pickable::IGNORE,
            style::panel(),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: UiRect::all(px(24)),
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((
                style::text(&assets, "Drop a PLY file or a complete Potree folder", 24.),
                Pickable::IGNORE,
            ));
        });
}
pub(super) fn overlay(files: Res<Files>, mut nodes: Query<&mut Node, With<DropOverlay>>) {
    for mut node in &mut nodes {
        node.display = if files.hovering {
            Display::Flex
        } else {
            Display::None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_ply_bytes_are_not_mistaken_for_potree_metadata() {
        let source = selected_source(vec![(
            "scan.PLY".into(),
            crate::PointcloudFile::from_bytes(vec![1, 2]),
        )])
        .unwrap();
        assert!(matches!(source, PointcloudSource::PlyFile { .. }));
    }
    #[test]
    fn superseded_picker_completion_cannot_replace_a_newer_source() {
        let mut app = App::new();
        let mut files = Files::default();
        files.invalidate();
        files
            .sender
            .send((
                0,
                PointcloudUp::Y,
                Ok(Some(PointcloudSource::Mesh(fixture_mesh()))),
            ))
            .unwrap();
        app.insert_resource(files)
            .init_resource::<OpenPanel>()
            .add_message::<Request>()
            .add_systems(Update, receive);
        app.update();
        assert!(app.world().resource::<Messages<Request>>().is_empty());
    }
}
