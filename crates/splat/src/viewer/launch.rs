//! Validated, bookmarkable startup options for the standalone viewer.

use super::navigation::{NavigationMode, home_pose};
use crate::SourceUp;
use bevy::prelude::*;
use bevy_gaussian_splatting::GaussianTransparency;
use dimensify_camera::CameraPose;

#[derive(Default)]
pub(super) struct Launch {
    pub url: Option<String>,
    pub format: super::remote::DataFormat,
    pub up: SourceUp,
    pub pose: Option<CameraPose>,
    pub fov: Option<f32>,
    pub mode: NavigationMode,
    pub transparency: GaussianTransparency,
    pub frame: bool,
    pub embed: bool,
}

impl Launch {
    /// Validate the complete query before any resource or download is changed.
    pub fn parse(query: &str, page: &str) -> Result<Self, String> {
        let mut values = std::collections::HashMap::new();
        for (key, value) in url::form_urlencoded::parse(query.trim_start_matches('?').as_bytes()) {
            if !matches!(
                key.as_ref(),
                "url"
                    | "format"
                    | "up"
                    | "orientation"
                    | "eye"
                    | "target"
                    | "fov"
                    | "mode"
                    | "transparency"
                    | "frame"
                    | "embed"
            ) {
                continue; // Other consumers own parameters such as perf=1.
            }
            if values.insert(key.to_string(), value.to_string()).is_some() {
                return Err(format!("Duplicate viewer option: {key}"));
            }
        }
        let value = |key: &str| values.get(key).map(String::as_str);
        let mut launch = Self {
            format: value("format").unwrap_or("auto").parse()?,
            ..default()
        };
        if let Some(input) = value("url") {
            if input.trim().is_empty() {
                return Err("Scene URL is empty".into());
            }
            let page = url::Url::parse(page).map_err(|_| "Invalid viewer page URL")?;
            let resource = page.join(input.trim()).map_err(|_| "Invalid scene URL")?;
            launch.url = Some(
                super::remote::ResourceUrl::parse_with_format(resource.as_str(), launch.format)?
                    .url
                    .into(),
            );
        }
        if let Some(preset) = value("orientation") {
            if value("up").is_some() {
                return Err("Use either orientation or up, not both".into());
            }
            launch.up = match preset {
                "3dgs" => SourceUp::Gaussian,
                "y-up" => SourceUp::Y,
                "z-up" => SourceUp::Z,
                _ => return Err("orientation must be 3dgs, y-up or z-up".into()),
            };
        }
        if let Some(axis) = value("up") {
            launch.up = match axis {
                "x" => SourceUp::X,
                "y" => SourceUp::Y,
                "z" => SourceUp::Z,
                "-x" => SourceUp::NegativeX,
                "-y" => SourceUp::NegativeY,
                "-z" => SourceUp::NegativeZ,
                _ => return Err("up must be x, y, z, -x, -y or -z".into()),
            };
        }
        if let Some(mode) = value("mode") {
            launch.mode = match mode {
                "fly" => NavigationMode::Fly,
                "orbit" => NavigationMode::Orbit,
                _ => return Err("mode must be fly or orbit".into()),
            };
        }
        if let Some(mode) = value("transparency") {
            launch.transparency = GaussianTransparency::parse(mode).map_err(String::from)?;
        }
        if let Some(fov) = value("fov") {
            let fov: f32 = fov.parse().map_err(|_| "fov must be a number in degrees")?;
            if !fov.is_finite() || !(1. ..=179.).contains(&fov) {
                return Err("fov must be between 1 and 179 degrees".into());
            }
            launch.fov = Some(fov.to_radians());
        }
        match (value("eye"), value("target")) {
            (Some(eye), Some(target)) => {
                let eye = vector(eye)?;
                let target = vector(target)?;
                if eye.distance(target) < 1e-4 {
                    return Err("eye and target must be different positions".into());
                }
                launch.pose = Some(CameraPose::looking(
                    eye,
                    target,
                    launch.fov.unwrap_or(home_pose().fov),
                ));
            }
            (None, None) => {}
            _ => return Err("Supply both eye and target as x,y,z coordinates".into()),
        }
        launch.frame = boolean(value("frame"), launch.url.is_some())? && launch.pose.is_none();
        launch.embed = boolean(value("embed"), false)?;
        Ok(launch)
    }

    pub fn apply(&self, world: &mut World) {
        let mut pose = self.pose.unwrap_or_else(home_pose);
        pose.fov = self.fov.unwrap_or(pose.fov);
        {
            let mut options = world.resource_mut::<super::ViewerOptions>();
            options.up = self.up;
            options.pose = Some(pose);
            options.embed = self.embed;
            options.initial_frame = self.frame;
        }
        world
            .resource_mut::<dimensify_camera::viewer::NavigationConfig>()
            .home = pose;
        if self.pose.is_some() {
            world
                .resource_mut::<dimensify_camera::OrbitState>()
                .seed_from(&pose);
        }
        *world.resource_mut::<NavigationMode>() = self.mode;
        world.resource_mut::<crate::runtime::SplatTransparency>().0 = self.transparency;
    }
}

fn boolean(value: Option<&str>, default: bool) -> Result<bool, String> {
    match value {
        None => Ok(default),
        Some("1") => Ok(true),
        Some("0") => Ok(false),
        _ => Err("frame and embed must be 0 or 1".into()),
    }
}

fn vector(value: &str) -> Result<Vec3, String> {
    let numbers = value
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Camera coordinates must be three finite numbers")?;
    if numbers.len() != 3 || numbers.iter().any(|n| !n.is_finite() || n.abs() > 1e7) {
        return Err("Camera coordinates must be x,y,z, each within +/-10000000".into());
    }
    Ok(Vec3::new(numbers[0], numbers[1], numbers[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_presets_are_explicit_and_legacy_axes_remain_distinct() {
        for (name, expected) in [
            ("3dgs", SourceUp::Gaussian),
            ("y-up", SourceUp::Y),
            ("z-up", SourceUp::Z),
        ] {
            assert_eq!(
                Launch::parse(&format!("orientation={name}"), "https://viewer.test/")
                    .unwrap()
                    .up,
                expected
            );
        }
        assert_eq!(
            Launch::parse("up=-y", "https://viewer.test/").unwrap().up,
            SourceUp::NegativeY
        );
        for query in [
            "orientation=left-handed",
            "orientation=3dgs&up=-y",
            "orientation=3dgs&orientation=y-up",
        ] {
            assert!(
                Launch::parse(query, "https://viewer.test/").is_err(),
                "{query}"
            );
        }
    }

    #[test]
    fn explicit_format_supports_extensionless_endpoints_and_rejects_unknown_types() {
        assert!(Launch::parse("format=invalid", "https://viewer.test/").is_err());
        assert!(
            Launch::parse(
                "url=https://assets.test/download?token=private&format=sog-unbundled",
                "https://viewer.test/"
            )
            .is_ok()
        );
        assert!(Launch::parse("format=sog&format=ply", "https://viewer.test/").is_err());
    }

    #[test]
    fn applying_startup_options_sets_home_without_locking_camera_override() {
        let mut world = World::new();
        world.init_resource::<super::super::ViewerOptions>();
        world.init_resource::<dimensify_camera::viewer::NavigationConfig>();
        world.init_resource::<dimensify_camera::OrbitState>();
        world.init_resource::<NavigationMode>();
        world.init_resource::<crate::runtime::SplatTransparency>();
        let launch = Launch::parse(
            "up=y&eye=4,3,6&target=0,0,0&fov=60&mode=orbit&embed=1",
            "https://host.test/",
        )
        .unwrap();
        launch.apply(&mut world);
        let config = world.resource::<dimensify_camera::viewer::NavigationConfig>();
        assert!(
            config.override_pose.is_none(),
            "startup must not pin the camera every frame"
        );
        assert_eq!(config.home.position, Vec3::new(4., 3., 6.));
        assert!((config.home.fov.to_degrees() - 60.).abs() < 1e-4);
        assert!(
            (world.resource::<dimensify_camera::OrbitState>().distance - 61_f32.sqrt()).abs()
                < 1e-5
        );
        assert_eq!(*world.resource::<NavigationMode>(), NavigationMode::Orbit);
        let options = world.resource::<super::super::ViewerOptions>();
        assert!(options.embed && !options.initial_frame);
        assert_eq!(options.up, SourceUp::Y);

        // An ordinary empty link preserves the navigation plugin's defaults.
        let mut defaults = World::new();
        defaults.init_resource::<super::super::ViewerOptions>();
        defaults.init_resource::<dimensify_camera::viewer::NavigationConfig>();
        defaults.init_resource::<dimensify_camera::OrbitState>();
        defaults.init_resource::<NavigationMode>();
        defaults.init_resource::<crate::runtime::SplatTransparency>();
        let distance = defaults.resource::<dimensify_camera::OrbitState>().distance;
        Launch::parse("", "https://host.test/")
            .unwrap()
            .apply(&mut defaults);
        assert_eq!(
            defaults.resource::<dimensify_camera::OrbitState>().distance,
            distance
        );
    }

    #[test]
    fn signed_scene_url_and_explicit_camera_survive_query_decoding() {
        let launch = Launch::parse("?url=https%3A%2F%2Fassets.test%2Fscene.sog%3Ftoken%3Da%2Bb&up=-z&eye=1,2,3&target=0,0,0&fov=60&mode=orbit&frame=1&embed=1", "https://viewer.test/splat-viewer/").unwrap();
        assert_eq!(
            launch.url.as_deref(),
            Some("https://assets.test/scene.sog?token=a+b")
        );
        assert_eq!(launch.up, crate::SourceUp::NegativeZ);
        assert_eq!(launch.mode, dimensify_camera::viewer::NavigationMode::Orbit);
        assert!(launch.embed);
        assert!(!launch.frame, "authored camera wins over auto framing");
        let pose = launch.pose.unwrap();
        assert_eq!(pose.position, bevy::prelude::Vec3::new(1., 2., 3.));
        assert!(pose.focus().length() < 1e-5);
        assert!((pose.fov.to_degrees() - 60.).abs() < 1e-4);
    }

    #[test]
    fn default_links_stay_empty_and_scene_links_frame_once() {
        let empty = Launch::parse("?perf=1", "http://localhost:8080/").unwrap();
        assert!(empty.url.is_none() && empty.pose.is_none() && !empty.frame && !empty.embed);
        let scene = Launch::parse(
            "?url=../scenes/rings.ply",
            "https://host.test/project/splat-viewer/",
        )
        .unwrap();
        assert_eq!(
            scene.url.as_deref(),
            Some("https://host.test/project/scenes/rings.ply")
        );
        assert!(scene.frame);
        assert!(
            !Launch::parse(
                "?url=https://assets.test/a.sog&frame=0",
                "https://host.test/"
            )
            .unwrap()
            .frame
        );
    }

    #[test]
    fn invalid_known_options_are_rejected_atomically_without_echoing_secrets() {
        for query in [
            "up=w",
            "up=z&up=y",
            "embed=yes",
            "mode=walk",
            "frame=2",
            "eye=0,0,0",
            "target=0,0,0",
            "eye=0,0,0&target=0,0,0",
            "eye=NaN,0,0&target=0,0,-1",
            "eye=1e30,0,0&target=0,0,0",
            "eye=0,0,0,1&target=0,0,-1",
            "fov=0",
            "fov=180",
            "url=javascript:secret.sog",
            "url=https://host.test/not-a-scene.txt",
            "url=",
            "transparency=invalid",
        ] {
            let error = Launch::parse(query, "https://viewer.test/")
                .err()
                .expect(query);
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn all_source_axes_and_existing_transparency_option_are_supported() {
        for (name, up) in [
            ("x", crate::SourceUp::X),
            ("y", crate::SourceUp::Y),
            ("z", crate::SourceUp::Z),
            ("-x", crate::SourceUp::NegativeX),
            ("-y", crate::SourceUp::NegativeY),
            ("-z", crate::SourceUp::NegativeZ),
        ] {
            let launch = Launch::parse(
                &format!("up={name}&transparency=stochastic"),
                "https://host.test/",
            )
            .unwrap();
            assert_eq!(launch.up, up);
            assert_eq!(
                launch.transparency,
                bevy_gaussian_splatting::GaussianTransparency::Stochastic
            );
        }
    }
}
