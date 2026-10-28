#!/usr/bin/env python3
"""Package the shared production Gaussian visual testbed for a browser."""
import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="dist/visual-web", type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    subprocess.run(["cargo", "build", "--locked", "--profile", "ci", "-p", "dimensify_perf", "--features", "visual", "--bin", "visual", "--target", "wasm32-unknown-unknown"], check=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"]))
    wasm = Path(metadata["target_directory"]) / "wasm32-unknown-unknown/ci/visual.wasm"
    subprocess.run(["wasm-bindgen", "--target", "web", "--out-dir", str(args.out), str(wasm)], check=True)
    (args.out / "index.html").write_text('''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="icon" href="data:,"><title>Dimensify Gaussian visual testbed</title>
<style>html,body{margin:0;background:black}canvas{display:block;width:256px;height:256px}</style></head>
<body><canvas id="dimensify-visual-canvas" width="256" height="256" aria-label="Gaussian visual testbed"></canvas>
<script type="module">import init from './visual.js';init().catch(error=>{console.error(error);window.__DIMENSIFY_VISUAL__={schema:'dimensify-visual-status/1',status:'failed',error:String(error)};});</script></body></html>
''')
    print(args.out / "index.html")


if __name__ == "__main__":
    main()
