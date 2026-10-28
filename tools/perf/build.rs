//! Embed provenance of the executable rather than guessing from the runtime PATH.
use std::{env, path::PathBuf, process::Command};

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn export(key: &str, value: &str) {
    println!(
        "cargo:rustc-env=DIMENSIFY_PERF_BUILD_{key}={}",
        value.replace(['\r', '\n'], " ")
    );
}

fn main() {
    for path in [
        ".",
        "../../crates/splat",
        "../../vendor/bevy_gaussian_splatting",
        "../../vendor/sog_codec",
        "../../vendor/splat_sort",
        "../../Cargo.toml",
        "../../Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    for path in ["HEAD", "index"] {
        if let Some(path) = git(&["rev-parse", "--git-path", path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    export(
        "COMMIT",
        &git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unavailable".into()),
    );
    export(
        "DIRTY",
        &git(&["status", "--porcelain"]).map_or_else(
            || "unavailable".into(),
            |status| (!status.is_empty()).to_string(),
        ),
    );
    for key in ["PROFILE", "OPT_LEVEL"] {
        export(key, &env::var(key).unwrap_or_else(|_| "unavailable".into()));
    }
    let rustc = env::var_os("RUSTC")
        .map(PathBuf::from)
        .unwrap_or_else(|| "rustc".into());
    let version = Command::new(rustc)
        .args(["--version", "--verbose"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".into());
    export("RUSTC", &version);
}
