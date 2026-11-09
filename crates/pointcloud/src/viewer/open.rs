//! Visible source loading for the native and browser point-cloud viewer.
use super::*;
use bevy::input_focus::InputFocus;
mod ui;

#[derive(Resource, Default)]
pub(super) struct OpenPanel {
    pub(super) open: bool,
    up: PointcloudUp,
    error: Option<String>,
}

#[derive(Message)]
enum Request {
    Url(String, PointcloudUp),
    Demo,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<OpenPanel>()
        .add_message::<Request>()
        .add_observer(ui::action)
        .add_systems(Startup, ui::setup)
        .add_systems(Update, (ui::update, load, ui::sync, ui::scroll).chain());
    #[cfg(target_arch = "wasm32")]
    app.add_observer(ui::browser_commands);
}

fn request_url(value: &str, up: PointcloudUp) -> Result<LoadPointcloud, String> {
    let value = value.trim();
    let parsed =
        url::Url::parse(value).map_err(|_| "Enter an HTTP(S) Potree dataset URL.".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("Use an HTTP(S) URL. Serve local Potree directories over HTTP.".into());
    }
    let path = parsed.path().to_ascii_lowercase();
    if [".ply", ".las", ".laz", ".copc", ".sog", "cloud.js"]
        .iter()
        .any(|suffix| path.ends_with(suffix))
    {
        return Err("This viewer opens Potree 2 datasets. Use a dataset directory or metadata.json URL; PLY/LAS/COPC import is not available.".into());
    }
    let mut request = LoadPointcloud::potree(parsed.to_string());
    request.up = up;
    Ok(request)
}

fn load(
    mut requests: MessageReader<Request>,
    mut commands: Commands,
    roots: Query<Entity, With<ViewerCloud>>,
    mut panel: ResMut<OpenPanel>,
    mut source: ResMut<ViewerSource>,
    mut settings: ResMut<settings::Settings>,
    mut focus: ResMut<InputFocus>,
) {
    for request in requests.read() {
        let request = match request {
            Request::Url(url, up) => match request_url(url, *up) {
                Ok(request) => request,
                Err(error) => {
                    panel.error = Some(error);
                    continue;
                }
            },
            Request::Demo => fixture_source(),
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
        app.init_resource::<OpenPanel>()
            .add_message::<Request>()
            .add_systems(Update, load);
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
    fn unsupported_or_invalid_urls_preserve_the_current_cloud() {
        for url in [
            "",
            "not a URL",
            "file:///tmp/cloud",
            "https://example.org/cloud.ply?download=1",
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
            .add_systems(Update, ui::update.before(load));
        app.world_mut().spawn((
            ui::UrlField,
            bevy::text::EditableText::new("https://example.org/cloud/"),
        ));
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
