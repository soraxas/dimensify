use std::{io::Write, process::Command};

use potree::convert::streaming::{convert_ply_streaming, ConvertPlyOptions, OctreeLayout};
use tempfile::tempdir;

#[test]
fn cli_repacks_a_stock_dataset() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("points.ply");
    let source = dir.path().join("single");
    let output = dir.path().join("multi");
    let mut ply = std::fs::File::create(&input).unwrap();
    writeln!(ply, "ply").unwrap();
    writeln!(ply, "format ascii 1.0").unwrap();
    writeln!(ply, "element vertex 2").unwrap();
    writeln!(ply, "property float x").unwrap();
    writeln!(ply, "property float y").unwrap();
    writeln!(ply, "property float z").unwrap();
    writeln!(ply, "end_header").unwrap();
    writeln!(ply, "0 0 0").unwrap();
    writeln!(ply, "1 1 1").unwrap();
    std::fs::create_dir(&source).unwrap();

    convert_ply_streaming(
        &input,
        &source,
        &ConvertPlyOptions {
            layout: OctreeLayout::SingleFile,
            encoding: "DEFAULT".to_string(),
            ..Default::default()
        },
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_potree_repack"))
        .arg(&source)
        .arg(&output)
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(output.join("octree/r.bin").exists());
    assert!(!output.join("octree.bin").exists());
}
