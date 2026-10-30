//! Native packed-stream replay options.
use std::path::PathBuf;

pub struct Options {
    pub manifest: PathBuf,
    pub camera: Option<PathBuf>,
    pub output: PathBuf,
    pub preset: String,
    pub transparency: bevy_gaussian_splatting::GaussianTransparency,
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
            transparency: bevy_gaussian_splatting::GaussianTransparency::Sorted,
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
                "--transparency" => {
                    options.transparency =
                        bevy_gaussian_splatting::GaussianTransparency::parse(&value)?
                }
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
            || options.samples == 0
            || options.warmup == 0
        {
            return Err("invalid zero/oversized replay dimensions".into());
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
    #[test]
    fn explicit_stochastic_mode_is_accepted_but_unknown_modes_are_rejected() {
        let args = |mode: &str| {
            vec![
                "--manifest".into(),
                "fixture.json".into(),
                "--transparency".into(),
                mode.into(),
            ]
        };
        assert!(Options::parse(args("stochastic")).is_ok());
        assert!(Options::parse(args("sorted")).is_ok());
        assert!(Options::parse(args("mcmc")).is_err());
    }
}
