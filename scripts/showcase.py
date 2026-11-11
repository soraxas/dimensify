"""Discover examples from Cargo metadata and build their static browser showcase."""

import argparse
import html
import json
import math
from pathlib import Path
import shlex
import shutil
import subprocess
import struct

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "splat-web"


def discover(metadata, root):
    """Include every workspace example; metadata opts runnable targets into WASM."""
    root = Path(root).resolve()
    entries = []
    for package in metadata["packages"]:
        if package["id"] not in metadata["workspace_members"]:
            continue
        configured = (package.get("metadata") or {}).get("showcase", [])
        overrides = {
            (item["target"], item.get("kind", "example")): item for item in configured
        }
        actual = {
            (target["name"], kind)
            for target in package["targets"]
            for kind in target["kind"]
        }
        if len(overrides) != len(configured) or not overrides.keys() <= actual:
            raise ValueError(
                f"Invalid or duplicate showcase targets in {package['name']}"
            )
        for target in package["targets"]:
            kind = "example" if "example" in target["kind"] else "bin"
            config = overrides.get((target["name"], kind), {})
            if "example" not in target["kind"] and not config:
                continue
            if config.get("hidden", False):
                continue
            source = Path(target["src_path"]).resolve().relative_to(root).as_posix()
            features = config.get("features", target.get("required-features", []))
            enabled = set(features)
            pending = list(features)
            while pending:
                feature = pending.pop()
                for implied in package.get("features", {}).get(feature, []):
                    if (
                        implied in package.get("features", {})
                        and implied not in enabled
                    ):
                        enabled.add(implied)
                        pending.append(implied)
            if not set(target.get("required-features", [])) <= enabled:
                raise ValueError(f"Missing required features for {target['name']}")
            browser = config.get("browser", False)
            release_profile = config.get("release_profile", "release")
            if release_profile not in {"release", "wasm-release"}:
                raise ValueError(f"Invalid showcase release profile: {release_profile}")
            targets = config.get(
                "targets", ["browser", "native"] if browser else ["local"]
            )
            if ("browser" in targets) != browser:
                raise ValueError(
                    f"Browser target requires explicit browser=true: {target['name']}"
                )
            identifier = config.get("id", f"{package['name']}-{target['name']}")
            command = ["cargo", "run", "--locked", "-p", package["name"]]
            if features:
                command += ["--features", ",".join(features)]
            command += [f"--{kind}", target["name"]]
            command += config.get("args", [])
            title = config.get("title", target["name"].replace("_", " ").capitalize())
            lines = (root / source).read_text().splitlines()
            description = next(
                (line[3:].strip() for line in lines if line.startswith("//!")),
                "Runnable Cargo example.",
            )
            entries.append(
                dict(
                    id=identifier,
                    title=title,
                    package=package["name"],
                    targets=targets,
                    description=config.get("description", description),
                    command=shlex.join(command),
                    source=source,
                    note=config.get(
                        "note",
                        "Local example; add showcase metadata to describe its runtime.",
                    ),
                    href=config.get(
                        "href", f"../examples/{package['name']}/{target['name']}/"
                    )
                    if browser
                    else None,
                    build=dict(
                        package=package["name"],
                        target=target["name"],
                        kind=kind,
                        features=features,
                        sog_worker=config.get("sog_worker", False),
                        release_profile=release_profile,
                    ),
                )
            )
    if len({entry["id"] for entry in entries}) != len(entries):
        raise ValueError("Showcase IDs must be unique")
    return sorted(
        entries, key=lambda entry: (entry["href"] is None, entry["title"].lower())
    )


def catalog(entries):
    """Serialize the public catalog without private build bookkeeping."""
    public = [
        {key: value for key, value in entry.items() if key != "build"}
        for entry in entries
    ]
    for entry in public:
        if entry["id"] == "splat" and entry["href"]:
            entry["href"] = "../splat-viewer/"
    return (
        "// Generated by scripts/showcase.py; edit Cargo showcase metadata instead.\nexport const examples = "
        + json.dumps(public, indent=2)
        + ";\n"
    )


def run(command, cwd=ROOT):
    print("+ " + shlex.join(str(part) for part in command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


# CI builds each group in its own job and compiler-cache lane. The two
# fat-LTO release builds (splat and robot) get a group each so neither sits
# behind the other; any new browser entry joins "examples" automatically.
BUILD_GROUPS = ("splat", "robot", "pointcloud", "examples")
GROUP_OWNERS = {
    "splat": "splat",
    "robot": "robot",
    "pointcloud": "pointcloud",
    "pointcloud-fixture": "pointcloud",
}


def build_group(entries, group):
    """Return the browser entries one CI build group owns."""
    if group not in BUILD_GROUPS:
        raise ValueError(f"Unknown showcase group: {group}")
    return [
        entry
        for entry in entries
        if entry["href"] and GROUP_OWNERS.get(entry["id"], "examples") == group
    ]


def build(entries, metadata, profile):
    """Build each opted-in WASM target and package its browser entrypoint."""
    bindgen_flags = (
        []
        if profile == "dev"
        else ["--remove-name-section", "--remove-producers-section"]
    )
    for entry in entries:
        if not entry["href"]:
            continue
        item = entry["build"]
        selected_profile = (
            item.get("release_profile", "release") if profile == "release" else profile
        )
        output_profile = "debug" if selected_profile == "dev" else selected_profile
        target_dir = (
            Path(metadata["target_directory"]) / "wasm32-unknown-unknown" / output_profile
        )
        command = [
            "cargo",
            "build",
            "--locked",
            "-p",
            item["package"],
            f"--{item['kind']}",
            item["target"],
            "--target",
            "wasm32-unknown-unknown",
            "--profile",
            selected_profile,
        ]
        if item["features"]:
            command += ["--features", ",".join(item["features"])]
        run(command)
        page = (SITE / "gallery" / entry["href"]).resolve()
        if not page.is_relative_to(SITE.resolve()):
            raise ValueError("Browser entrypoint escapes the showcase site")
        existing_page = page.suffix == ".html"
        directory = page.parent if existing_page else page
        directory.mkdir(parents=True, exist_ok=True)
        wasm = (
            target_dir
            / ("examples" if item["kind"] == "example" else "")
            / (item["target"] + ".wasm")
        )
        shutil.rmtree(directory / "pkg", ignore_errors=True)
        run(
            [
                "mise",
                "exec",
                "--",
                "wasm-bindgen",
                *bindgen_flags,
                "--target",
                "web",
                "--out-dir",
                str(directory / "pkg"),
                str(wasm),
            ]
        )
        if not existing_page:
            title = html.escape(entry["title"])
            module = json.dumps(f"./pkg/{item['target']}.js")
            (directory / "index.html").write_text(f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>{title} - Dimensify</title><link rel="icon" href="data:,"><style>html,body{{margin:0;width:100%;height:100%;overflow:hidden;background:#0c1118;color:#e8eef5;font-family:system-ui}}canvas{{display:block;width:100%;height:100%;touch-action:none}}#failure{{position:absolute;inset:24px;display:none}}</style></head><body><canvas id="dimensify-canvas" aria-label="{title}"></canvas><p id="failure" role="alert"></p><script type="module">import init from {module};init().catch(error=>{{console.error(error);const message=document.querySelector("#failure");message.textContent="This example could not start. Check WebGPU support and serve the built assets over HTTPS or localhost.";message.style.display="block";}});</script></body></html>
''')
        if item["sog_worker"]:
            run(
                [
                    "cargo",
                    "build",
                    "--locked",
                    "-p",
                    "sog_codec",
                    "--features",
                    "worker",
                    "--release",
                    "--target",
                    "wasm32-unknown-unknown",
                ]
            )
            shutil.rmtree(directory / "sog-codec", ignore_errors=True)
            run(
                [
                    "mise",
                    "exec",
                    "--",
                    "wasm-bindgen",
                    *bindgen_flags,
                    "--target",
                    "web",
                    "--out-dir",
                    str(directory / "sog-codec"),
                    str(
                        Path(metadata["target_directory"])
                        / "wasm32-unknown-unknown/release/sog_codec.wasm"
                    ),
                ]
            )
            shutil.copyfile(
                ROOT / "crates/splat/src/streaming/runtime/sog-decode-worker.mjs",
                directory / "sog-decode-worker.mjs",
            )
        if entry["id"] == "splat":
            viewer_assets(SITE, SITE / "index.html")


def viewer_assets(site, host):
    """Add the named viewer route and tiny generated demo; never store model bytes in Git."""
    viewer = site / "splat-viewer"
    viewer.mkdir(parents=True, exist_ok=True)
    # Keep loader/worker URLs relative to the existing asset root on both local
    # hosting and Pages project subpaths. Query options use the actual page URL.
    markup = host.read_text().replace("<head>", '<head>\n    <base href="../" />', 1)
    (viewer / "index.html").write_text(markup)
    scenes = site / "scenes"
    scenes.mkdir(exist_ok=True)
    names = [
        "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2",
        "scale_0", "scale_1", "scale_2", "opacity",
        "rot_0", "rot_1", "rot_2", "rot_3",
    ]
    names += [f"f_rest_{index}" for index in range(45)]
    count = 3 * 128
    header = f"ply\nformat binary_little_endian 1.0\nelement vertex {count}\n"
    header += "".join(f"property float {name}\n" for name in names) + "end_header\n"
    data = bytearray(header.encode("ascii"))
    for ring in range(3):
        for index in range(128):
            angle = index * math.tau / 128
            a, b = 1.8 * math.cos(angle), 1.8 * math.sin(angle)
            position = [(a, b, 0), (0, a, b), (b, 0, a)][ring]
            rgb = [(0.95, 0.18, 0.12), (0.15, 0.85, 0.35), (0.2, 0.4, 1.0)][ring]
            dc = [(channel - 0.5) / 0.28209479177387814 for channel in rgb]
            values = [*position, *dc, -2.2, -3.2, -3.2, 1.5, 1, 0, 0, 0, *([0] * 45)]
            data.extend(struct.pack("<59f", *values))
    (scenes / "rings.ply").write_bytes(data)


def package_site(entries, site, output):
    """Stage a complete static site, rejecting missing or invalid build outputs."""
    site, output = Path(site).resolve(), Path(output).resolve()
    if output.exists():
        raise FileExistsError(
            f"Output already exists; choose a fresh directory: {output}"
        )
    files = {
        Path("gallery") / name
        for name in ["index.html", "gallery.mjs", "gallery.css", "launch.mjs", "build-link.html", "build-link.mjs"]
    }
    published = []
    for entry in entries:
        public = dict(entry)
        published.append(public)
        if not entry["href"]:
            continue
        page = (site / "gallery" / entry["href"]).resolve()
        if not page.is_relative_to(site):
            raise ValueError("Browser entrypoint escapes the showcase site")
        page = page if page.suffix == ".html" else page / "index.html"
        directory = page.parent
        files.add(page.relative_to(site))
        target = entry["build"]["target"]
        for name in [f"pkg/{target}.js", f"pkg/{target}_bg.wasm"]:
            files.add((directory / name).relative_to(site))
        # Include wasm-bindgen's imported snippets and declarations, not just
        # the top-level loader. build() creates these packages from clean output.
        files.update(
            path.relative_to(site)
            for path in (directory / "pkg").rglob("*")
            if path.is_file()
        )
        if entry["build"]["sog_worker"]:
            files.update(
                path.relative_to(site)
                for path in (directory / "sog-codec").rglob("*")
                if path.is_file()
            )
            for name in [
                "sog-decode-worker.mjs",
                "sog-codec/sog_codec.js",
                "sog-codec/sog_codec_bg.wasm",
            ]:
                files.add((directory / name).relative_to(site))
        # Keep the viewer's assets at their original relative URLs while making
        # the public root an entry point into the complete example gallery.
        if page == site / "index.html":
            public["href"] = "../viewer.html"
    for name in files:
        source = site / name
        if not source.is_file():
            raise ValueError(f"Missing showcase asset: {name}")
        if source.suffix == ".wasm":
            with source.open("rb") as wasm:
                if wasm.read(8) != b"\x00asm\x01\x00\x00\x00":
                    raise ValueError(f"Invalid WebAssembly asset: {name}")
    output.mkdir(parents=True)
    for name in sorted(files):
        destination = output / ("viewer.html" if name == Path("index.html") else name)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(site / name, destination)
    (output / "gallery/catalog.mjs").write_text(catalog(published))
    if (output / "viewer.html").is_file():
        viewer_assets(output, output / "viewer.html")
    (output / "index.html").write_text(
        '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="refresh" content="0;url=./gallery/"><title>Dimensify Examples</title></head><body><a href="./gallery/">Explore Dimensify examples</a></body></html>\n'
    )
    (output / ".nojekyll").touch()
    print(f"Published site staged at {output}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--catalog", action="store_true", help="Refresh catalog without compiling"
    )
    mode.add_argument(
        "--check", action="store_true", help="Check catalog freshness without writing"
    )
    mode.add_argument(
        "--list", action="store_true", help="Print discovered entries without writing"
    )
    parser.add_argument(
        "--only", action="append", help="Build only this example ID (repeatable)"
    )
    parser.add_argument(
        "--group",
        choices=BUILD_GROUPS,
        help="Build one CI group; new browser examples join the examples group",
    )
    parser.add_argument(
        "--profile", choices=["dev", "release", "wasm-release"], default="dev"
    )
    parser.add_argument(
        "--output", type=Path, help="Stage a complete Pages site in a new directory"
    )
    parser.add_argument(
        "--package-only",
        action="store_true",
        help="Stage existing builds without compiling",
    )
    args = parser.parse_args()
    if args.output and args.output.exists():
        parser.error("Output already exists; choose a fresh directory")
    if args.output and (
        args.only or args.group or args.catalog or args.check or args.list
    ):
        parser.error("--output requires the complete gallery")
    if args.group and args.only:
        parser.error("Choose --group or --only, not both")
    if args.package_only and not args.output:
        parser.error("--package-only requires --output")
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=ROOT,
            text=True,
        )
    )
    entries = discover(metadata, ROOT)
    output = catalog(entries)
    path = SITE / "gallery/catalog.mjs"
    if args.list:
        print(output)
        return
    if args.check:
        if not path.exists() or path.read_text() != output:
            raise SystemExit("Showcase catalog is stale. Run just showcase-catalog.")
        print(f"Showcase catalog current: {len(entries)} examples")
        return
    selected = build_group(entries, args.group) if args.group else entries
    if args.only:
        selected = [entry for entry in entries if entry["id"] in args.only]
        if {entry["id"] for entry in selected} != set(args.only):
            raise SystemExit("Unknown showcase ID; use --list")
    if not args.catalog and not args.package_only:
        build(selected, metadata, args.profile)
    path.write_text(output)
    if args.output:
        package_site(entries, SITE, args.output)
    print(
        f"Showcase catalog: {len(entries)} examples. Serve splat-web/ and open /gallery/."
    )


if __name__ == "__main__":
    main()
