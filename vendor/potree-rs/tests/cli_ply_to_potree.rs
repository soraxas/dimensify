//! End-to-end tests for the `ply_to_potree` CLI binary itself.
//!
//! Unlike `tests/streaming_*.rs`, which call `convert_ply_streaming` directly
//! as a library function, these tests spawn the actual compiled binary. That
//! exercises clap argument parsing, the CLI's own flag defaults (which have
//! already diverged from the library's `ConvertPlyOptions::default()` once:
//! the binary defaults `--encoding` to `BROTLI`, the library defaults to
//! `DEFAULT`), output-directory creation, and process exit codes/stderr on
//! bad input -- none of which the library-level tests can see.

use potree::hierarchy::HierarchyAsync;
use potree::prelude::*;
use serde_json::Value;
use std::io::Write;
use std::process::Command;
use tempfile::tempdir;

fn ply_to_potree_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ply_to_potree"))
}

fn write_ascii_ply(points: &[[f64; 3]], path: &std::path::Path) {
    let mut file = std::fs::File::create(path).unwrap();
    writeln!(file, "ply").unwrap();
    writeln!(file, "format ascii 1.0").unwrap();
    writeln!(file, "element vertex {}", points.len()).unwrap();
    writeln!(file, "property double x").unwrap();
    writeln!(file, "property double y").unwrap();
    writeln!(file, "property double z").unwrap();
    writeln!(file, "end_header").unwrap();
    for p in points {
        writeln!(file, "{} {} {}", p[0], p[1], p[2]).unwrap();
    }
}

fn sample_points() -> Vec<[f64; 3]> {
    let mut points = Vec::new();
    for i in 0..200 {
        let t = i as f64;
        points.push([t % 10.0, (t * 1.3) % 10.0, (t * 0.7) % 5.0]);
    }
    points
}

#[test]
fn cli_converts_ply_and_metadata_reflects_flags() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.ply");
    write_ascii_ply(&sample_points(), &input);
    let output = dir.path().join("out");

    let result = ply_to_potree_bin()
        .arg(&input)
        .arg(&output)
        .arg("--name")
        .arg("my-cloud")
        .arg("--projection")
        .arg("EPSG:4326")
        .arg("--scale")
        .arg("0.01")
        .arg("--max-points-per-node")
        .arg("50")
        .arg("--encoding")
        .arg("DEFAULT")
        .output()
        .expect("failed to run ply_to_potree binary");

    assert!(
        result.status.success(),
        "binary exited with failure: stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("Wrote Potree output to"),
        "unexpected stdout: {stdout}"
    );

    for f in ["metadata.json", "hierarchy.bin"] {
        assert!(output.join(f).exists(), "missing output file {f}");
    }
    // Default layout is `multi-file`: no octree.bin, one file per node instead.
    assert!(
        !output.join("octree.bin").exists(),
        "default layout should not write octree.bin"
    );
    assert!(
        std::fs::read_dir(output.join("octree")).unwrap().next().is_some(),
        "default layout should write per-node files under octree/"
    );

    let metadata_json = std::fs::read_to_string(output.join("metadata.json")).unwrap();
    let metadata: Value = serde_json::from_str(&metadata_json).unwrap();

    assert_eq!(metadata["name"], "my-cloud");
    assert_eq!(metadata["projection"], "EPSG:4326");
    assert_eq!(metadata["encoding"], "DEFAULT");

    let position = metadata["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "position")
        .expect("metadata missing position attribute");
    for s in position["scale"].as_array().unwrap() {
        assert_eq!(s.as_f64().unwrap(), 0.01, "--scale flag was not applied");
    }
}

#[test]
fn cli_default_encoding_is_brotli() {
    // The CLI's own clap default (`--encoding` defaults to `"BROTLI"`) diverges
    // from `ConvertPlyOptions::default()` (`"DEFAULT"`). This pins the CLI's
    // actual behavior so that divergence can't silently drift further.
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.ply");
    write_ascii_ply(&sample_points(), &input);
    let output = dir.path().join("out");

    let result = ply_to_potree_bin()
        .arg(&input)
        .arg(&output)
        .output()
        .expect("failed to run ply_to_potree binary");

    assert!(result.status.success());

    let metadata_json = std::fs::read_to_string(output.join("metadata.json")).unwrap();
    let metadata: Value = serde_json::from_str(&metadata_json).unwrap();
    assert_eq!(metadata["encoding"], "BROTLI");
}

#[test]
fn cli_creates_missing_output_directory() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.ply");
    write_ascii_ply(&sample_points(), &input);
    // Nested, does not exist yet.
    let output = dir.path().join("does/not/exist/yet");

    let result = ply_to_potree_bin()
        .arg(&input)
        .arg(&output)
        .output()
        .expect("failed to run ply_to_potree binary");

    assert!(result.status.success());
    assert!(output.join("metadata.json").exists());
}

#[test]
fn cli_rejects_missing_input_file() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("does-not-exist.ply");
    let output = dir.path().join("out");

    let result = ply_to_potree_bin()
        .arg(&input)
        .arg(&output)
        .output()
        .expect("failed to run ply_to_potree binary");

    assert!(
        !result.status.success(),
        "expected non-zero exit for a missing input file"
    );
}

#[test]
fn cli_rejects_non_ply_input() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("not-a-ply.txt");
    std::fs::write(&input, b"this is not a ply file\n").unwrap();
    let output = dir.path().join("out");

    let result = ply_to_potree_bin()
        .arg(&input)
        .arg(&output)
        .output()
        .expect("failed to run ply_to_potree binary");

    assert!(
        !result.status.success(),
        "expected non-zero exit for a non-PLY input file"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cli_output_is_readable_by_the_library_reader() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.ply");
    let points = sample_points();
    write_ascii_ply(&points, &input);
    let output = dir.path().join("out");

    let result = ply_to_potree_bin()
        .arg(&input)
        .arg(&output)
        .arg("--max-points-per-node")
        .arg("30")
        .output()
        .expect("failed to run ply_to_potree binary");
    assert!(result.status.success());

    let hierarchy = Hierarchy::from_path(&output)
        .await
        .expect("failed to load hierarchy written by the CLI");
    let nodes = hierarchy
        .load_entire_hierarchy()
        .await
        .expect("failed to load full hierarchy");

    let mut decoded = 0usize;
    for node in &nodes {
        if node.num_points == 0 {
            continue;
        }
        let pts = hierarchy.load_points(node).await.unwrap();
        decoded += pts.buffer.count;
    }

    assert_eq!(
        decoded,
        points.len(),
        "point count read back through the library should match the CLI's input"
    );
}
