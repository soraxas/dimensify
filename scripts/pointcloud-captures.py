#!/usr/bin/env python3
"""Package windowless point-cloud viewer captures for Pixel Eagle review."""

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import struct
import subprocess
import tempfile
import zlib


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("reporter", ROOT / "scripts/visual-pixeleagle.py")
reporter = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reporter)
SIZES = {"desktop": (1000, 760), "narrow": (360, 640)}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def png_from_ppm(data, width, height):
    header = f"P6\n{width} {height}\n255\n".encode()
    if not data.startswith(header) or len(data) != len(header) + width * height * 3:
        raise ValueError("Missing or malformed PPM readback")
    rgb = data[len(header):]
    rgba = bytearray()
    for row in range(height):
        rgba.append(0)
        for offset in range(row * width * 3, (row + 1) * width * 3, 3):
            rgba.extend(rgb[offset:offset + 3])
            rgba.append(255)

    def chunk(kind, value):
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rgba)) + chunk(b"IEND", b""))


def source_identity():
    sources = {}
    for directory in ("crates/pointcloud", "crates/camera", "crates/ui", "vendor/bevy_pointcloud", "vendor/potree-rs"):
        for source in sorted((ROOT / directory / "src").rglob("*")):
            if source.is_file():
                sources[str(source.relative_to(ROOT))] = sha256(source.read_bytes())
        manifest = ROOT / directory / "Cargo.toml"
        sources[str(manifest.relative_to(ROOT))] = sha256(manifest.read_bytes())
    for name in ("Cargo.toml", "Cargo.lock", ".cargo/config.toml", "rust-toolchain.toml", "scripts/pointcloud-captures.py"):
        source = ROOT / name
        if source.is_file():
            sources[name] = sha256(source.read_bytes())
    return {
        "gitHead": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "workingTreeDirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
        "sourceFiles": sources,
        "sourceFingerprint": sha256(json.dumps(sources, sort_keys=True).encode()),
        "os": platform.platform(),
        "scope": "native offscreen production viewer; browser and physical-device behavior unverified",
    }


def capture(output):
    before = source_identity()
    with tempfile.TemporaryDirectory(prefix="pointcloud-viewer-") as temporary:
        raw = Path(temporary) / "captures"
        import os
        command = ["cargo", "test", "--locked", "--profile", "ci", "-p", "dimensify_pointcloud",
                   "--features", "viewer", "--lib", "viewer::tests::viewer_images_and_keyboard_work_offscreen",
                   "--", "--ignored", "--nocapture", "--exact"]
        subprocess.run(command, cwd=ROOT, check=True,
                       env={**os.environ, "DIMENSIFY_POINTCLOUD_VIEWER_CAPTURE": str(raw)})
        after = source_identity()
        if before != after:
            raise ValueError("Source inputs changed during build/capture; discard this evidence")
        before["captureCommand"] = command
        (raw / "provenance.json").write_text(json.dumps(before, indent=2) + "\n")
        package(raw, output)


def package(raw, output):
    # Preserve capture-side identity when repackaging; never relabel old pixels
    # with the checkout that happens to run the packager.
    provenance = json.loads((raw / "provenance.json").read_text())
    scene = (raw / "scene.json").read_bytes()
    prepared = []
    for label, (width, height) in SIZES.items():
        adapter = (raw / f"{label}-adapter.txt").read_text().splitlines()
        if len(adapter) != 4 or not adapter[0] or not adapter[1]:
            raise ValueError("Missing observed render adapter identity")
        metadata = {
            "lane": f"pointcloud-viewer-{adapter[1].lower()}-{label}",
            "target": "native", "backend": adapter[1], "adapter": adapter[0],
            "driver": " / ".join(adapter[2:]).strip(" / ") or "unreported; " + provenance["os"],
            "browser": "none", "viewport": {"width": width, "height": height},
            "sceneHash": sha256(scene),
            "settingsHash": sha256((raw / f"{label}-settings.json").read_bytes()),
        }
        manifest = {"schema": "dimensify-visual-captures/1", "suite": "pointcloud-viewer",
                    "metadata": metadata, "captures": []}
        files = {}
        for case in reporter.POINTCLOUD_CASES:
            name = f"{label}-{case}"
            files[f"{case}.png"] = png_from_ppm((raw / f"{name}.ppm").read_bytes(), width, height)
            files[f"{case}.txt"] = (raw / f"{name}.txt").read_bytes()
            manifest["captures"].append({"name": case, "file": f"{case}.png", "width": width, "height": height})
        prepared.append((label, manifest, files))
    output.mkdir()
    for label, manifest, files in prepared:
        directory = output / label
        directory.mkdir()
        for name, data in files.items():
            (directory / name).write_bytes(data)
        (directory / "captures.json").write_text(json.dumps(manifest, indent=2) + "\n")
        (directory / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
        (directory / "scene.json").write_bytes(scene)
        (directory / "settings.json").write_bytes((raw / f"{label}-settings.json").read_bytes())
        reporter.load_captures(directory)
    print(f"Validated six viewer PNGs for Pixel Eagle review: {output}")
    print("No service upload, baseline approval or comparison occurred.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--raw", type=Path, help="Repackage existing capture-side provenance and images")
    mode.add_argument("--capture", action="store_true", help="Build, capture and package with source identity checks")
    parser.add_argument("--out", type=Path, required=True, help="New output directory")
    args = parser.parse_args()
    if args.capture:
        capture(args.out.resolve())
    else:
        package(args.raw.resolve(), args.out.resolve())
