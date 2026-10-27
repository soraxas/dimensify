#!/usr/bin/env python3
"""Verify an isolated source export resolves the active headless packages."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--keep", type=Path, help="Export into a new directory and keep it")
parser.add_argument(
    "--test", action="store_true", help="Run the exported headless test suites"
)
args = parser.parse_args()
source = Path(__file__).resolve().parents[1]
temporary = (
    None if args.keep else tempfile.TemporaryDirectory(prefix="dimensify-export-")
)
export = args.keep.resolve() if args.keep else Path(temporary.name) / "source"


def ignore_generated(directory, names):
    ignored = set(shutil.ignore_patterns(
        ".git", ".gitrepo", "target", "node_modules", ".venv", "__pycache__", "dist"
    )(directory, names))
    if Path(directory).name == "splat-web":
        ignored.update({"pkg", "sog-codec", "sog-decode-worker.mjs", "examples"} & set(names))
    return ignored


try:
    shutil.copytree(
        source,
        export,
        ignore=ignore_generated,
    )
    # The export contains the same trusted configuration as this checkout.
    # Scope trust to this verification process, without changing global trust.
    os.environ["MISE_TRUSTED_CONFIG_PATHS"] = os.pathsep.join(filter(None, [
        os.environ.get("MISE_TRUSTED_CONFIG_PATHS"), str(export),
    ]))
    result = subprocess.run(
        ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"],
        cwd=export,
        text=True,
        capture_output=True,
    )
    assert result.returncode == 0, result.stderr
    metadata = json.loads(result.stdout)
    subprocess.run([sys.executable, "scripts/showcase.py", "--check"], cwd=export, check=True)
    packages = {p["id"]: p for p in metadata["packages"]}
    names = {packages[p]["name"] for p in metadata["workspace_members"]}
    required = {"dimensify_transport", "dimensify_session", "dimensify_hub", "dimensify_splat", "dimensify_camera", "dimensify_interpolation"}
    assert required <= names, f"Missing standalone packages: {required - names}"
    for p in packages.values():
        if p["source"] is None:
            assert (
                Path(p["manifest_path"]).resolve().is_relative_to(export.resolve())
            ), p["manifest_path"]
    tree = subprocess.check_output(
        [
            "cargo",
            "tree",
            "--offline",
            "-p",
            "dimensify_hub",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ],
        cwd=export,
        text=True,
    )
    prohibited = {"bevy_render", "bevy_winit", "wgpu", "urdf-rs", "rapier3d", "avian3d"}
    linked = {line.split()[0] for line in tree.splitlines() if line.strip()}
    assert not prohibited & linked, f"Headless graph includes: {prohibited & linked}"
    for package, features in [("dimensify_camera", []),
                              ("dimensify_camera", ["--features", "inspect"]),
                              ("dimensify_splat", [])]:
        policy_tree = subprocess.check_output(
            ["cargo", "tree", "--offline", "--locked", "-p", package,
             "--no-default-features", "--edges", "normal", "--prefix", "none",
             "--format", "{p}", *features], cwd=export, text=True,
        )
        policy_linked = {line.split()[0] for line in policy_tree.splitlines() if line.strip()}
        policy_prohibited = prohibited | ({"reqwest", "bevy_clipboard"} if package == "dimensify_splat" else set())
        forbidden = policy_prohibited & policy_linked
        assert not forbidden, f"{package} core graph includes: {forbidden}"
    if args.test:
        subprocess.run(
            [
                "cargo",
                "test",
                "--offline",
                "--locked",
                "-p",
                "dimensify_transport",
                "-p",
                "dimensify_session",
                "-p",
                "dimensify_hub",
                "--features",
                "dimensify_session/server,dimensify_transport/server",
            ],
            cwd=export,
            check=True,
        )
    print(f"Standalone packages and headless dependency boundary verified: {export}")
finally:
    if temporary:
        temporary.cleanup()
