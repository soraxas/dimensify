//! Standalone URDF viewer for local files or HTTP(S) URLs. Run `robosim --help`
//! for controls and options; the browser build reads `?urdf=` and `&package=`.
use dimensify::robosim::{self, RobotModel, RobotSource};
use eyre::Result;
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    use eyre::bail;
    use std::path::PathBuf;

    dimensify::util::initialise()?;
    let mut input = None;
    let mut packages = Vec::new();
    let mut check = false;
    let mut screenshot = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: robosim [robot.urdf | https://.../robot.urdf] [--package name=https://base/]... [--check | --screenshot output.png]\n\nWithout a source, opens the bundled demo arm. GitHub file pages are\nread as raw content. package:// meshes resolve from a matching URL\ndirectory, then each ancestor directory; --package overrides one.\n--check: validate and tick the robot without a window or GPU.\n--screenshot: render offscreen, save a PNG, and exit.\n\nTab selects a joint; Left/Right moves it; R resets all joints.\nDrag to orbit; wheel to zoom; F to frame the robot; Esc to quit."
                );
                return Ok(());
            }
            "--check" => check = true,
            "--screenshot" => {
                screenshot =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        eyre::eyre!("--screenshot requires an output path")
                    })?))
            }
            "--package" => packages.push(
                args.next()
                    .ok_or_else(|| eyre::eyre!("--package requires name=https://base/"))?,
            ),
            _ if arg.starts_with('-') => bail!("Unknown option: {arg}"),
            _ if input.is_none() => input = Some(arg),
            _ => bail!("Expected only one URDF source"),
        }
    }
    if check && screenshot.is_some() {
        bail!("Choose either --check or --screenshot");
    }
    let model = match input {
        Some(input) => {
            let mut source = RobotSource::parse(&input)?;
            for package in &packages {
                source = source.with_package(package)?;
            }
            RobotModel::load(source)?
        }
        None if packages.is_empty() => RobotModel::from_urdf(robosim::DEMO_URDF, Path::new("."))?,
        None => bail!("--package applies only to a URL source"),
    };
    if check {
        robosim::check(model)
    } else {
        robosim::run(model, screenshot)
    }
}

#[cfg(target_arch = "wasm32")]
fn main() -> Result<()> {
    let query = web_sys::window()
        .and_then(|window| window.location().search().ok())
        .unwrap_or_default();
    let query = url::form_urlencoded::parse(query.trim_start_matches('?').as_bytes());
    let mut input = None;
    let mut packages = Vec::new();
    for (key, value) in query {
        match &*key {
            "urdf" => input = Some(value.into_owned()),
            "package" => packages.push(value.into_owned()),
            _ => {}
        }
    }
    let Some(input) = input else {
        return robosim::run(
            RobotModel::from_urdf(robosim::DEMO_URDF, Path::new("."))?,
            None,
        );
    };
    let source = packages
        .iter()
        .try_fold(RobotSource::parse(&input)?, |source, package| {
            source.with_package(package)
        })?;
    let RobotSource::Url { url, packages } = source else {
        eyre::bail!("Enter a full HTTP or HTTPS URDF URL");
    };
    robosim::run_loading(robosim::RobotLoad::start(url, packages))
}
