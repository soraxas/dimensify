use std::path::PathBuf;

use clap::Parser;
use potree::convert::streaming::{convert_ply_streaming, ConvertPlyOptions, OctreeLayout};

/// Convert a PLY file into Potree format (hierarchy.bin, metadata.json, and
/// octree node data laid out per --layout)
#[derive(Debug, Parser)]
struct Args {
    /// Input PLY file
    input: PathBuf,

    /// Output directory to write Potree files into
    output: PathBuf,

    /// Optional pointcloud name (defaults to input file stem)
    #[arg(long)]
    name: Option<String>,

    /// Projection string to place in metadata.json
    #[arg(long)]
    projection: Option<String>,

    /// Uniform scale factor to apply to coordinates
    #[arg(long, default_value_t = 0.001)]
    scale: f64,

    /// Maximum number of points to keep in each node (before splitting or sampling).
    /// Default matches the C++ PotreeConverter's maxPointsPerNode.
    #[arg(long, default_value_t = 10_000)]
    max_points_per_node: usize,

    /// Maximum octree depth
    #[arg(long, default_value_t = 20)]
    max_depth: u32,

    /// Output encoding: DEFAULT (raw AoS) or BROTLI (SoA + Brotli compression)
    #[arg(long, default_value = "BROTLI")]
    encoding: String,

    /// Octree node storage layout: `multi-file` (default, one file per node
    /// under octree/, no octree.bin — avoids Chrome's Range-request cache
    /// bug with no storage overhead), `single-file` (only octree.bin, the
    /// original Potree v2 layout, works with stock Potree tooling), or
    /// `dual` (writes both octree.bin and octree/<name>.bin for every node —
    /// works everywhere, at ~2x point-data storage).
    #[arg(long, value_enum, default_value = "multi-file")]
    layout: OctreeLayout,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input = args.input;
    let output = args.output;

    let name = args.name.unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("pointcloud")
            .to_string()
    });

    std::fs::create_dir_all(&output)?;

    convert_ply_streaming(
        &input,
        &output,
        &ConvertPlyOptions {
            name,
            projection: args.projection.unwrap_or_default(),
            target_scale: [args.scale; 3],
            max_points_per_node: args.max_points_per_node,
            max_depth: args.max_depth,
            encoding: args.encoding,
            layout: args.layout,
        },
    )?;

    println!("Wrote Potree output to {}", output.display());

    Ok(())
}
