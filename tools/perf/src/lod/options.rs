//! Native packed-stream replay options.
use std::path::PathBuf;

pub struct Options {
    pub manifest: PathBuf,
    pub camera: Option<PathBuf>,
    pub output: PathBuf,
    pub preset: String,
    pub width: u32,
    pub height: u32,
    pub samples: usize,
    pub warmup: usize,
    pub timestamps: bool,
    pub smoke: bool,
}
impl Options {
    pub fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut args = args.into_iter();
        let mut manifest = None;
        let mut options = Self {
            manifest: PathBuf::new(),
            camera: None,
            output: PathBuf::from("artifacts/native-lod"),
            preset: "default".into(),
            width: 1280,
            height: 720,
            samples: 240,
            warmup: 32,
            timestamps: false,
            smoke: false,
        };
        while let Some(arg) = args.next() {
            if arg == "--gpu-timestamps" {
                options.timestamps = true;
                continue;
            }
            if arg == "--smoke" {
                options.smoke = true;
                continue;
            }
            let value = args.next().ok_or_else(|| format!("{arg} needs a value"))?;
            match arg.as_str() {
                "--manifest" => manifest = Some(PathBuf::from(value)),
                "--camera" => options.camera = Some(value.into()),
                "--output" => options.output = value.into(),
                "--preset" => options.preset = value,
                "--width" => options.width = value.parse().map_err(|_| "invalid width")?,
                "--height" => options.height = value.parse().map_err(|_| "invalid height")?,
                "--samples" => options.samples = value.parse().map_err(|_| "invalid samples")?,
                "--warmup" => options.warmup = value.parse().map_err(|_| "invalid warmup")?,
                _ => return Err(format!("unknown option {arg}")),
            }
        }
        options.manifest = manifest.ok_or("--manifest is required")?;
        if !matches!(options.preset.as_str(), "default" | "constrained") {
            return Err("preset must be default or constrained".into());
        }
        if options.width == 0
            || options.height == 0
            || options.width > 8192
            || options.height > 8192
            || options.samples < 4
            || options.samples > 1_000_000
            || options.warmup == 0
            || options.warmup > 1_000_000
        {
            return Err("invalid replay dimensions or sample/warmup bounds (samples 4..=1000000; warmup 1..=1000000)".into());
        }
        if !options.smoke && options.samples < 240 {
            return Err("publication replay needs at least 240 samples".into());
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Options, String> {
        let mut values = vec!["--manifest".to_owned(), "fixture.json".to_owned()];
        values.extend(args.iter().map(|value| (*value).to_owned()));
        Options::parse(values)
    }
    #[test]
    fn smoke_still_requires_a_nondegenerate_camera_period() {
        for samples in ["1", "2", "3"] {
            assert!(parse(&["--smoke", "--samples", samples]).is_err());
        }
        assert!(parse(&["--smoke", "--samples", "4"]).is_ok());
    }
    #[test]
    fn replay_controls_are_positive_and_bounded() {
        for option in ["--samples", "--warmup"] {
            for value in ["0", "1000001", "18446744073709551615"] {
                assert!(parse(&[option, value]).is_err(), "{option} {value}");
            }
            assert!(parse(&[option, "1000000"]).is_ok());
        }
    }
}
