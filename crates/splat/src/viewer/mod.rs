//! Standalone local-file splat viewer with native and browser controls.
use crate::SourceUp;
use crate::{LoadPhase, SplatPlugin, SplatScene};
use bevy::{core_pipeline::tonemapping::Tonemapping, prelude::*};
use bevy_gaussian_splatting::GaussianCamera;

mod files;
mod navigation;
mod remote;
mod streaming;
mod ui;
mod url_panel;
#[cfg(target_arch = "wasm32")]
mod web;

#[derive(Message, Default)]
struct FrameSplat;

#[derive(Resource, Default)]
struct ViewerOptions {
    up: SourceUp,
    #[cfg(not(target_arch = "wasm32"))]
    path: Option<std::path::PathBuf>,
}

/// Run the standalone PLY/SOG viewer with local file controls.
/// Native arguments accept one optional path and `--y-up`.
pub fn run() {
    #[cfg(not(target_arch = "wasm32"))]
    let options = {
        let mut options = ViewerOptions::default();
        for argument in std::env::args().skip(1) {
            match argument.as_str() {
                "--help" | "-h" => {
                    println!(
                        "Usage: splat_viewer [--y-up] [scene.ply|scene.sog]\nOpen File or drop a file to load. WASD: fly; Q/E: down/up; left drag: look; right drag: pan; scroll: move; F: frame. Reset returns to the origin."
                    );
                    return;
                }
                "--y-up" => options.up = SourceUp::Y,
                value if value.starts_with('-') => {
                    eprintln!("Unknown option: {value}");
                    std::process::exit(2);
                }
                _ => {
                    if options.path.is_some() {
                        eprintln!("Choose one PLY or SOG file at a time");
                        std::process::exit(2);
                    }
                    options.path = Some(argument.into());
                }
            }
        }
        options
    };
    let window = Window {
        title: "Dimensify Splat Viewer".into(),
        resolution: (1200, 800).into(),
        #[cfg(target_arch = "wasm32")]
        canvas: Some("#dimensify-canvas".into()),
        #[cfg(target_arch = "wasm32")]
        fit_canvas_to_parent: true,
        ..default()
    };
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.025, 0.035, 0.055)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }))
        .add_plugins(bevy::feathers::FeathersPlugins)
        .insert_resource(bevy::feathers::theme::UiTheme(
            bevy::feathers::dark_theme::create_dark_theme(),
        ))
        .add_plugins((SplatPlugin, crate::streaming::runtime::SplatStreamingPlugin))
        .init_resource::<streaming::ViewerStreams>()
        .add_message::<streaming::LoadStream>()
        .add_systems(
            Update,
            streaming::stage
                .after(files::receive)
                .before(crate::scene::request_load),
        )
        .add_systems(
            PostUpdate,
            streaming::poll
                .after(crate::streaming::runtime::update_sog_streams)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        )
        .init_resource::<ViewerOptions>()
        .init_resource::<files::LocalFiles>()
        .init_resource::<url_panel::UrlPanel>()
        .add_plugins(dimensify_camera::viewer::ViewerNavigationPlugin)
        .add_message::<FrameSplat>()
        .add_systems(Startup, (camera, ui::setup, url_panel::setup))
        .add_systems(
            Update,
            (
                url_panel::update,
                files::receive,
                ui::status,
                navigation::sync,
            )
                .chain()
                .before(dimensify_camera::viewer::NavigationSystems),
        );
    #[cfg(not(target_arch = "wasm32"))]
    app.insert_resource(options)
        .add_systems(Startup, files::initial_file)
        .add_systems(Update, files::drops.before(files::receive));
    #[cfg(target_arch = "wasm32")]
    app.add_observer(url_panel::browser_commands)
        .add_systems(Startup, web::install)
        .add_systems(Update, web::receive.before(files::receive));
    app.run();
}

fn camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        GaussianCamera::default(),
        dimensify_camera::viewer::ViewerCamera,
        crate::streaming::runtime::StreamingCamera,
        Msaa::Off,
        Tonemapping::None,
        navigation::home_pose().to_transform(),
        IsDefaultUiCamera,
    ));
}

fn status_text(scene: &SplatScene) -> String {
    match &scene.phase {
        LoadPhase::Empty => {
            "Open a Gaussian PLY or bundled SOG file. Drop one file anywhere to load it.".into()
        }
        LoadPhase::Loading(name) => format!("Loading {name}..."),
        LoadPhase::Ready { name, count } => format!("{name}  |  {count} Gaussians"),
        LoadPhase::Failed(error) => format!("Could not load {error}"),
    }
}
