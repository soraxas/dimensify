//! Standalone native URDF viewer. Run `robosim --help` for controls and options.
use dimensify::robosim::{self, RobotModel};
use eyre::{Result, bail};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    dimensify::util::initialise()?;
    let mut path = None;
    let mut check = false;
    let mut screenshot = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!(
                    "Usage: robosim [robot.urdf] [--check | --screenshot output.png]\n\nWithout a path, opens the bundled demo arm.\n--check: validate and tick the robot without a window or GPU.\n--screenshot: render offscreen, save a PNG, and exit.\n\nTab selects a joint; Left/Right moves it; R resets all joints.\nDrag to orbit; wheel to zoom; F to frame the robot; Esc to quit."
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
            _ if arg.starts_with('-') => bail!("Unknown option: {arg}"),
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => bail!("Expected only one URDF path"),
        }
    }
    if check && screenshot.is_some() {
        bail!("Choose either --check or --screenshot");
    }
    let model = match path {
        Some(path) => RobotModel::from_file(&path)?,
        None => RobotModel::from_urdf(robosim::DEMO_URDF, Path::new("."))?,
    };
    if check {
        robosim::check(model)
    } else {
        robosim::run(model, screenshot)
    }
}
