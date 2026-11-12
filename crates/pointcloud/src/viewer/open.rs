//! Visible source loading for the native and browser point-cloud viewer.
use super::*;
use bevy::input_focus::InputFocus;
pub(super) mod files;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

#[derive(Resource, Default)]
pub(super) struct OpenPanel {
    pub(super) open: bool,
    pub(super) up: PointcloudUp,
    error: Option<String>,
    pub(super) submit: bool,
}

#[derive(Message)]
enum Request {
    Url(String, PointcloudUp),
    Demo,
    Source(Box<PointcloudSource>, PointcloudUp, u64),
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<OpenPanel>()
        .init_resource::<files::Files>()
        .add_message::<Request>()
        .add_observer(ui::action)
        .add_systems(Startup, (ui::setup, files::setup))
        .add_systems(
            Update,
            (
                ui::update,
                files::receive,
                load,
                ui::sync,
                ui::scroll,
                files::overlay,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            ui::submit.after(bevy::text::EditableTextSystems),
        );
    #[cfg(target_arch = "wasm32")]
    app.add_observer(ui::browser_commands)
        .add_systems(Startup, web::install)
        .add_systems(Update, web::receive.before(files::receive));
    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(Update, files::drops.before(files::receive));
}

pub(super) fn request_url(value: &str, up: PointcloudUp) -> Result<LoadPointcloud, String> {
    let value = value.trim();
    let parsed = url::Url::parse(value)
        .map_err(|_| "Enter an HTTP(S) PLY or Potree dataset URL.".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(
            "Use an HTTP(S) URL, or choose a local PLY file or Potree folder with Open.".into(),
        );
    }
    let path = parsed.path().to_ascii_lowercase();
    if [".las", ".laz", ".copc", ".sog", "cloud.js"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
    {
        return Err("Open a PLY file or a Potree 2 dataset directory/metadata.json URL. LAS/LAZ/COPC import is not available.".into());
    }
    let mut request = if path.ends_with(".ply") {
        LoadPointcloud::new(PointcloudSource::PlyUrl(parsed.to_string()))
    } else {
        LoadPointcloud::potree(parsed.to_string())
    };
    request.up = up;
    Ok(request)
}

#[allow(clippy::too_many_arguments)]
fn load(
    mut requests: MessageReader<Request>,
    mut commands: Commands,
    roots: Query<Entity, With<ViewerCloud>>,
    mut panel: ResMut<OpenPanel>,
    mut source: ResMut<ViewerSource>,
    mut settings: ResMut<settings::Settings>,
    mut focus: ResMut<InputFocus>,
    mut files: ResMut<files::Files>,
) {
    for request in requests.read() {
        if let Request::Source(_, _, ticket) = request
            && *ticket != files.ticket
        {
            continue;
        }
        files.invalidate();
        panel.submit = false;
        let request = match request {
            Request::Url(url, up) => match request_url(url, *up) {
                Ok(request) => request,
                Err(error) => {
                    panel.error = Some(error);
                    continue;
                }
            },
            Request::Demo => fixture_source(),
            Request::Source(source, up, _) => {
                let mut request = LoadPointcloud::new(source.as_ref().clone());
                request.up = *up;
                request
            }
        };
        let Ok(root) = roots.single() else {
            panel.error = Some("The viewer is not ready. Try again.".into());
            continue;
        };
        commands
            .entity(root)
            .insert((request.clone(), FrameWhenLoaded));
        *settings = settings::Settings::new(request.settings.splats.clone());
        source.0 = request;
        panel.error = None;
        panel.open = false;
        focus.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (App, Entity) {
        let mut app = App::new();
        app.insert_resource(ViewerSource(fixture_source()))
            .insert_resource(settings::Settings::new(fixture_source().settings.splats))
            .init_resource::<InputFocus>();
        app.init_resource::<files::Files>()
            .init_resource::<OpenPanel>()
            .add_message::<Request>()
            .add_systems(Update, (ui::submit, load).chain());
        let root = app.world_mut().spawn(ViewerCloud).id();
        (app, root)
    }

    #[test]
    fn opening_a_url_reuses_the_cloud_root_and_requests_framing() {
        let (mut app, root) = app();
        app.world_mut().resource_mut::<OpenPanel>().open = true;
        app.world_mut().write_message(Request::Url(
            "  https://example.org/cloud/metadata.json?token=one%26two  ".into(),
            PointcloudUp::Z,
        ));
        app.update();
        let request = app
            .world()
            .get::<LoadPointcloud>(root)
            .expect("Load must issue a source replacement");
        let PointcloudSource::PotreeUrl(url) = &request.source else {
            panic!("expected Potree URL")
        };
        assert_eq!(
            url,
            "https://example.org/cloud/metadata.json?token=one%26two"
        );
        assert_eq!(request.up, PointcloudUp::Z);
        assert!(app.world().get::<FrameWhenLoaded>(root).is_some());
        assert!(!app.world().resource::<OpenPanel>().open);
    }

    #[test]
    fn ply_urls_preserve_query_and_use_the_file_loader() {
        let request = request_url("https://example.org/cloud.PLY?download=1", PointcloudUp::Z)
            .expect("PLY URL");
        assert_eq!(request.up, PointcloudUp::Z);
        let PointcloudSource::PlyUrl(url) = request.source else {
            panic!("expected PLY source")
        };
        assert_eq!(url, "https://example.org/cloud.PLY?download=1");
    }

    #[test]
    fn unsupported_or_invalid_urls_preserve_the_current_cloud() {
        for url in [
            "",
            "not a URL",
            "file:///tmp/cloud",
            "https://example.org/cloud.laz",
        ] {
            let (mut app, root) = app();
            app.world_mut().resource_mut::<OpenPanel>().open = true;
            app.world_mut()
                .write_message(Request::Url(url.into(), PointcloudUp::Y));
            app.update();
            assert!(
                app.world().get::<LoadPointcloud>(root).is_none(),
                "invalid URL replaced the source: {url}"
            );
            assert!(
                app.world().resource::<OpenPanel>().error.is_some(),
                "missing validation error: {url}"
            );
            assert!(app.world().resource::<OpenPanel>().open);
            assert!(matches!(
                app.world().resource::<ViewerSource>().0.source,
                PointcloudSource::Mesh(_)
            ));
        }
    }

    #[test]
    fn demo_restores_the_fixture_after_a_failed_url() {
        let (mut app, root) = app();
        app.world_mut().resource_mut::<OpenPanel>().error = Some("previous error".into());
        app.world_mut().write_message(Request::Demo);
        app.update();
        assert!(matches!(
            app.world().get::<LoadPointcloud>(root).unwrap().source,
            PointcloudSource::Mesh(_)
        ));
        assert!(app.world().resource::<OpenPanel>().error.is_none());
    }

    #[test]
    fn enter_that_opens_the_panel_does_not_also_load() {
        let (mut app, root) = app();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, ui::update.before(ui::submit));
        let mut field = bevy::text::EditableText::new("https://example.org/cloud/");
        // This focused keyboard test does not install TextPlugin; its initial
        // cursor-position edit is unrelated to the panel activation contract.
        field.pending_edits.clear();
        app.world_mut().spawn((ui::UrlField, field));
        app.world_mut().resource_mut::<OpenPanel>().open = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert!(app.world().get::<LoadPointcloud>(root).is_none());
        assert!(app.world().resource::<OpenPanel>().open);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::Enter);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert!(app.world().get::<LoadPointcloud>(root).is_some());
    }

    #[test]
    fn pending_replacement_does_not_frame_previous_source_bounds() {
        let (mut app, root) = app();
        app.init_resource::<NavigationConfig>()
            .add_message::<CameraAction>()
            .add_systems(PostUpdate, frame_loaded);
        let mut status = PointcloudStatus::loading(0., 30.);
        status.bounds = Some(bevy::camera::primitives::Aabb::from_min_max(
            Vec3::splat(100.),
            Vec3::splat(102.),
        ));
        app.world_mut()
            .entity_mut(root)
            .insert((status, GlobalTransform::default()));
        app.world_mut().write_message(Request::Url(
            "https://example.org/new-cloud/".into(),
            PointcloudUp::Y,
        ));
        app.update();
        assert!(
            app.world().get::<FrameWhenLoaded>(root).is_some(),
            "pending replacement must retain its framing request"
        );
        assert!(
            app.world().resource::<Messages<CameraAction>>().is_empty(),
            "old bounds must not reset the new camera home"
        );
    }
}
