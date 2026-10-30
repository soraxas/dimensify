use std::path::PathBuf;

#[derive(Debug)]
pub struct Options {
    pub input: Option<PathBuf>,
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub samples: usize,
    pub warmup: usize,
    pub smoke: bool,
    pub timestamps: bool,
    pub encoded_compositing: bool,
    pub camera: Option<PathBuf>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            input: None,
            output: "dist/performance-gpu".into(),
            width: 1280,
            height: 720,
            samples: 240,
            warmup: 32,
            smoke: false,
            timestamps: false,
            encoded_compositing: false,
            camera: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--smoke" => options.smoke = true,
                "--gpu-timestamps" => options.timestamps = true,
                "--encoded-compositing" => options.encoded_compositing = true,
                "--camera" => {
                    let value = args.next().ok_or("--camera needs a JSON file")?;
                    if value.starts_with('-') {
                        return Err("--camera needs a JSON file, not an option".into());
                    }
                    options.camera = Some(value.into());
                }
                "--output" => {
                    options.output = args.next().ok_or("--output needs a directory")?.into()
                }
                "--width" | "--height" | "--samples" | "--warmup" => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("{arg} needs a value"))?
                        .parse::<usize>()
                        .map_err(|_| format!("{arg} needs a positive integer"))?;
                    let limit = if arg == "--width" || arg == "--height" {
                        16_384
                    } else {
                        1_000_000
                    };
                    if value == 0 || value > limit {
                        return Err(format!("{arg} must be in 1..={limit}"));
                    }
                    match arg.as_str() {
                        "--width" => options.width = value as u32,
                        "--height" => options.height = value as u32,
                        "--samples" => options.samples = value,
                        _ => options.warmup = value,
                    }
                }
                value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
                _ if options.input.is_some() => {
                    return Err("choose one PLY or bundled SOG file".into());
                }
                _ => options.input = Some(arg.into()),
            }
        }
        if options.samples < 4 {
            return Err("--samples must be at least 4 to exercise camera motion".into());
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|s| (*s).to_owned()))
    }
    #[test]
    fn samples_and_resolution_must_be_positive_and_bounded() {
        for args in [
            vec!["--samples", "0"],
            vec!["--width", "0"],
            vec!["--height", "16385"],
            vec!["--samples", "oops"],
            vec!["--warmup", "0"],
        ] {
            assert!(parse(&args).is_err(), "{args:?}");
        }
    }
    #[test]
    fn explicit_input_and_smoke_keep_the_measurement_configuration() {
        let o = parse(&[
            "scene.ply",
            "--samples",
            "12",
            "--width",
            "640",
            "--height",
            "480",
            "--smoke",
            "--gpu-timestamps",
            "--output",
            "out",
        ])
        .unwrap();
        assert_eq!(o.input, Some("scene.ply".into()));
        assert_eq!((o.samples, o.width, o.height), (12, 640, 480));
        assert!(o.smoke && o.timestamps);
        assert_eq!(o.output, PathBuf::from("out"));
    }
    #[test]
    fn unknown_options_and_multiple_sources_fail() {
        assert!(parse(&["--wat"]).is_err());
        assert!(parse(&["a.ply", "b.sog"]).is_err());
        assert!(parse(&["--output"]).is_err());
    }

    #[test]
    fn moving_cases_require_a_nondegenerate_camera_period() {
        for samples in ["1", "2", "3"] {
            assert!(parse(&["--samples", samples]).is_err());
        }
        assert!(parse(&["--samples", "4"]).is_ok());
    }

    #[test]
    fn explicit_camera_requires_a_path_and_preserves_it() {
        let camera = parse(&["scene.sog", "--camera", "camera.json"]).unwrap();
        assert_eq!(camera.camera, Some("camera.json".into()));
        assert!(parse(&["--camera"]).is_err());
        assert!(parse(&["--camera", "--encoded-compositing"]).is_err());
    }

    #[test]
    fn encoded_compositing_is_opt_in() {
        assert!(!parse(&[]).unwrap().encoded_compositing);
        assert!(
            parse(&["--encoded-compositing"])
                .unwrap()
                .encoded_compositing
        );
    }
}
