use potree::convert::repack::repack_single_file_to_multi_file;
/// `OctreeLayout` controls how octree node point-data is stored:
/// `SingleFile` (only `octree.bin`, the original Potree v2 layout),
/// `MultiFile` (default: only `octree/<name>.bin` per node, no `octree.bin`),
/// or `Dual` (both, for readers/tooling that only understand `octree.bin`).
/// Verifies:
/// 1. Each layout writes exactly the files it should — no more, no less.
/// 2. `hierarchy.bin` is byte-identical across all three layouts — the
///    layout only changes which files get written, not the tree/offsets.
/// 3. `metadata.json`'s `octreeLayout` is absent for `SingleFile`,
///    `"multi-file"` for `MultiFile` and `Dual`.
/// 4. Every per-node file matches the corresponding `octree.bin` slice
///    exactly (under `Dual`, where both exist), using the real reader
///    (`Hierarchy::load_entire_hierarchy`) to resolve each node's
///    name/byte_offset/byte_size — proving converter and reader agree on
///    node names end-to-end.
use potree::convert::streaming::{convert_ply_streaming, ConvertPlyOptions, OctreeLayout};
use potree::hierarchy::HierarchyAsync;
use potree::octree::node::NodeType;
use potree::prelude::*;
use std::fs;
use std::io::Write;
use tempfile::tempdir;

fn write_ascii_ply(points: &[[f64; 3]], path: &std::path::Path) {
    let mut file = fs::File::create(path).unwrap();
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

/// 8 points at the corners of [0,1]³ — one per octant, guarantees the tree
/// splits at every level and produces both internal and leaf nodes.
fn corner_points() -> Vec<[f64; 3]> {
    vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 1.0],
    ]
}

fn convert_with_layout(input: &std::path::Path, output: &std::path::Path, layout: OctreeLayout) {
    convert_ply_streaming(
        input,
        output,
        &ConvertPlyOptions {
            name: "test".to_string(),
            max_points_per_node: 2,
            max_depth: 5,
            encoding: "DEFAULT".to_string(),
            layout,
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn each_layout_writes_exactly_the_files_it_should() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("pts.ply");
    write_ascii_ply(&corner_points(), &input);

    for (layout, expect_octree_bin, expect_node_files) in [
        (OctreeLayout::SingleFile, true, false),
        (OctreeLayout::MultiFile, false, true),
        (OctreeLayout::Dual, true, true),
    ] {
        let output = dir.path().join(format!("{layout:?}"));
        fs::create_dir_all(&output).unwrap();
        convert_with_layout(&input, &output, layout);

        assert!(
            output.join("hierarchy.bin").exists(),
            "{layout:?}: missing hierarchy.bin"
        );
        assert!(
            output.join("metadata.json").exists(),
            "{layout:?}: missing metadata.json"
        );
        assert_eq!(
            output.join("octree.bin").exists(),
            expect_octree_bin,
            "{layout:?}: octree.bin existence mismatch"
        );
        let has_node_files = output.join("octree").is_dir()
            && fs::read_dir(output.join("octree"))
                .unwrap()
                .next()
                .is_some();
        assert_eq!(
            has_node_files, expect_node_files,
            "{layout:?}: octree/ node files existence mismatch"
        );
    }
}

#[test]
fn hierarchy_bin_is_byte_identical_across_layouts() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("pts.ply");
    write_ascii_ply(&corner_points(), &input);

    let out_single = dir.path().join("out_single");
    let out_multi = dir.path().join("out_multi");
    let out_dual = dir.path().join("out_dual");
    for dir in [&out_single, &out_multi, &out_dual] {
        fs::create_dir_all(dir).unwrap();
    }

    convert_with_layout(&input, &out_single, OctreeLayout::SingleFile);
    convert_with_layout(&input, &out_multi, OctreeLayout::MultiFile);
    convert_with_layout(&input, &out_dual, OctreeLayout::Dual);

    let hierarchy_single = fs::read(out_single.join("hierarchy.bin")).unwrap();
    let hierarchy_multi = fs::read(out_multi.join("hierarchy.bin")).unwrap();
    let hierarchy_dual = fs::read(out_dual.join("hierarchy.bin")).unwrap();
    assert_eq!(
        hierarchy_single, hierarchy_multi,
        "layout must not perturb hierarchy.bin's byte_offset/byte_size/chunking"
    );
    assert_eq!(hierarchy_single, hierarchy_dual, "same for Dual");

    // SingleFile and Dual both write octree.bin — must be identical too.
    let octree_single = fs::read(out_single.join("octree.bin")).unwrap();
    let octree_dual = fs::read(out_dual.join("octree.bin")).unwrap();
    assert_eq!(
        octree_single, octree_dual,
        "layout must not change octree.bin's content, only whether it's written"
    );
}

#[test]
fn octree_layout_field_reflects_the_layout() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("pts.ply");
    write_ascii_ply(&corner_points(), &input);

    let out_single = dir.path().join("out_single");
    let out_multi = dir.path().join("out_multi");
    let out_dual = dir.path().join("out_dual");
    for dir in [&out_single, &out_multi, &out_dual] {
        fs::create_dir_all(dir).unwrap();
    }

    convert_with_layout(&input, &out_single, OctreeLayout::SingleFile);
    convert_with_layout(&input, &out_multi, OctreeLayout::MultiFile);
    convert_with_layout(&input, &out_dual, OctreeLayout::Dual);

    let read_meta = |dir: &std::path::Path| -> serde_json::Value {
        serde_json::from_slice(&fs::read(dir.join("metadata.json")).unwrap()).unwrap()
    };

    assert!(
        read_meta(&out_single).get("octreeLayout").is_none(),
        "SingleFile metadata.json should have no octreeLayout key"
    );
    assert_eq!(
        read_meta(&out_multi)["octreeLayout"].as_str(),
        Some("multi-file"),
        "MultiFile metadata.json should set octreeLayout"
    );
    assert_eq!(
        read_meta(&out_dual)["octreeLayout"].as_str(),
        Some("multi-file"),
        "Dual metadata.json should set octreeLayout too (per-node files exist)"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn per_node_files_match_octree_bin_slices() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("pts.ply");
    write_ascii_ply(&corner_points(), &input);

    let output = dir.path().join("out");
    fs::create_dir_all(&output).unwrap();
    // Dual, so both octree.bin and octree/<name>.bin exist to compare.
    convert_with_layout(&input, &output, OctreeLayout::Dual);

    let octree_bin = fs::read(output.join("octree.bin")).unwrap();

    // Dogfood the real reader to resolve every node's name/byte_offset/byte_size
    // from hierarchy.bin, exactly as a multi-file-aware asset backend would.
    let hierarchy = Hierarchy::from_path(&output)
        .await
        .expect("failed to load hierarchy from converted output");
    let nodes = hierarchy
        .load_entire_hierarchy()
        .await
        .expect("failed to load entire hierarchy");

    let mut checked = 0usize;
    for node in &nodes {
        if matches!(node.node_type, NodeType::Proxy) || node.byte_size == 0 {
            continue;
        }

        let node_file = output.join("octree").join(format!("{}.bin", node.name));
        let node_bytes = fs::read(&node_file)
            .unwrap_or_else(|e| panic!("missing per-node file {node_file:?}: {e}"));

        assert_eq!(
            node_bytes.len() as u64,
            node.byte_size,
            "node {} file length mismatch",
            node.name
        );

        let start = node.byte_offset as usize;
        let end = start + node.byte_size as usize;
        assert_eq!(
            node_bytes,
            octree_bin[start..end],
            "node {} bytes don't match its octree.bin slice",
            node.name
        );
        checked += 1;
    }
    assert!(checked > 0, "expected at least one non-empty node to check");
}

#[tokio::test(flavor = "current_thread")]
async fn multi_file_only_layout_is_readable() {
    // The default layout (MultiFile, no octree.bin at all) must still be
    // fully readable through the public reader API — this is what every
    // consumer gets by default now.
    let dir = tempdir().unwrap();
    let input = dir.path().join("pts.ply");
    let source = corner_points();
    write_ascii_ply(&source, &input);

    let output = dir.path().join("out");
    fs::create_dir_all(&output).unwrap();
    convert_with_layout(&input, &output, OctreeLayout::MultiFile);
    assert!(
        !output.join("octree.bin").exists(),
        "MultiFile layout should not write octree.bin"
    );

    let hierarchy = Hierarchy::from_path(&output)
        .await
        .expect("failed to load hierarchy from a MultiFile-only dataset");
    let nodes = hierarchy.load_entire_hierarchy().await.unwrap();

    let mut decoded_count = 0usize;
    for node in &nodes {
        if matches!(node.node_type, NodeType::Proxy) || node.num_points == 0 {
            continue;
        }
        let pts = hierarchy.load_points(node).await.unwrap();
        decoded_count += pts.buffer.count;
    }
    assert_eq!(
        decoded_count,
        source.len(),
        "should recover all points from a MultiFile-only dataset"
    );
}

#[test]
fn metadata_without_octree_layout_key_still_deserializes() {
    // Shaped like a real pre-existing Potree v2 metadata.json (e.g. the
    // heidentor sample dataset) that predates this field entirely.
    let json = r#"{
        "version": "2.0",
        "name": "heidentor",
        "description": "",
        "points": 100,
        "projection": "",
        "hierarchy": { "firstChunkSize": 100, "stepSize": 4, "depth": 3 },
        "offset": [0.0, 0.0, 0.0],
        "scale": [0.001, 0.001, 0.001],
        "spacing": 1.0,
        "boundingBox": { "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 1.0] },
        "encoding": "DEFAULT",
        "attributes": []
    }"#;

    let metadata: potree::metadata::Metadata = serde_json::from_str(json).unwrap();
    assert_eq!(metadata.octree_layout, None);
}

#[tokio::test(flavor = "current_thread")]
async fn repacks_single_file_without_mutating_the_source() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("pts.ply");
    write_ascii_ply(&corner_points(), &input);

    let source = dir.path().join("single");
    let output = dir.path().join("multi");
    fs::create_dir_all(&source).unwrap();
    convert_with_layout(&input, &source, OctreeLayout::SingleFile);

    let source_metadata_before = fs::read(source.join("metadata.json")).unwrap();
    let source_octree_before = fs::read(source.join("octree.bin")).unwrap();

    repack_single_file_to_multi_file(&source, &output).unwrap();

    assert_eq!(
        fs::read(source.join("metadata.json")).unwrap(),
        source_metadata_before
    );
    assert_eq!(
        fs::read(source.join("octree.bin")).unwrap(),
        source_octree_before
    );
    assert!(!output.join("octree.bin").exists());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(output.join("metadata.json")).unwrap()
        )
        .unwrap()["octreeLayout"],
        "multi-file"
    );

    let hierarchy = Hierarchy::from_path(&output).await.unwrap();
    let nodes = hierarchy.load_entire_hierarchy().await.unwrap();
    let mut decoded_count = 0usize;
    for node in &nodes {
        if matches!(node.node_type, NodeType::Proxy) || node.num_points == 0 {
            continue;
        }
        decoded_count += hierarchy.load_points(node).await.unwrap().buffer.count;
    }
    assert_eq!(decoded_count, corner_points().len());
}
