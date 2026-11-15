"""Build and stage the splat viewer's WebGL2 backend as a local static site."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess

from showcase import ROOT, SITE, run, viewer_assets

# The gallery page refuses to start without WebGPU; WebGL2 needs no such gate.
GATE = """const start = navigator.gpu
        ? init()
        : Promise.reject(new Error("WebGPU is unavailable"));"""


def bindgen(wasm, directory):
    shutil.rmtree(directory, ignore_errors=True)
    run(["mise", "exec", "--", "wasm-bindgen", "--target", "web",
         "--out-dir", str(directory), str(wasm)])


def page(source):
    """Return the gallery viewer page without its WebGPU requirement."""
    markup = source.read_text()
    if GATE not in markup:
        raise ValueError(f"WebGPU start gate not found in {source}")
    return (
        markup.replace(GATE, "const start = init();")
        .replace("JavaScript and WebGPU", "JavaScript and WebGL2")
        .replace("a WebGPU-capable browser", "a WebGL2-capable browser")
    )


def build(output):
    """Build the WebGL2 viewer plus its decode and sort workers into `output`."""
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
    ))
    target = Path(metadata["target_directory"]) / "wasm32-unknown-unknown/release"
    cargo = ["cargo", "build", "--locked", "--release",
             "--target", "wasm32-unknown-unknown"]
    run([*cargo, "-p", "dimensify", "--example", "splat_viewer",
         "--features", "splat,dimensify_splat/webgl"])
    run([*cargo, "-p", "sog_codec", "--features", "worker"])
    # The CPU-sorted backend runs splat_sort in a module worker beside the page.
    run([*cargo, "-p", "splat_sort", "--features", "worker"])
    output.mkdir(parents=True)
    bindgen(target / "examples/splat_viewer.wasm", output / "pkg")
    bindgen(target / "sog_codec.wasm", output / "sog-codec")
    bindgen(target / "splat_sort.wasm", output / "splat-sort")
    shutil.copyfile(
        ROOT / "crates/splat/src/streaming/runtime/sog-decode-worker.mjs",
        output / "sog-decode-worker.mjs",
    )
    shutil.copyfile(
        ROOT / "vendor/bevy_gaussian_splatting/src/render/webgl/sort_worker.mjs",
        output / "sort_worker.mjs",
    )
    (output / "index.html").write_text(page(SITE / "index.html"))
    viewer_assets(output, output / "index.html")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="fresh directory for the site")
    output = parser.parse_args().output.resolve()
    if output.exists():
        raise SystemExit(f"Output already exists; choose a fresh directory: {output}")
    build(output)
    print(f"Serve with: python3 -m http.server 8000 --bind 127.0.0.1 --directory {output}")


if __name__ == "__main__":
    main()
