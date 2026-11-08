use std::path::PathBuf;

use clap::Parser;
use potree::convert::repack::repack_single_file_to_multi_file;

/// Create a per-node-file Potree v2 dataset without decoding point data.
#[derive(Debug, Parser)]
struct Args {
    /// Stock Potree v2 directory containing metadata.json, hierarchy.bin, and octree.bin.
    source: PathBuf,
    /// New output directory for metadata.json, hierarchy.bin, and octree/<node>.bin.
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    repack_single_file_to_multi_file(&args.source, &args.output)?;
    println!(
        "Wrote multi-file Potree dataset to {}",
        args.output.display()
    );
    Ok(())
}
