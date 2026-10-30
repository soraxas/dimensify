"""Build, verify and serve the browser benchmark site."""

from contextlib import contextmanager
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import threading
from urllib.parse import unquote, urlsplit

from perf_scenes_common import clean_env, digest, inside, write


@contextmanager
def serve(site, assets):
    class Handler(SimpleHTTPRequestHandler):
        def translate_path(self, path):
            path = unquote(urlsplit(path).path)
            if path.startswith("/fixtures/"):
                return str(inside(assets, path.removeprefix("/fixtures/")))
            return super().translate_path(path)

        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(
        ("127.0.0.1", 0), partial(Handler, directory=str(site))
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def build_web(checkout, output):
    checkout = checkout.resolve()
    output.mkdir(parents=True, exist_ok=False)
    before = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=checkout, text=True
    ).strip()
    dirty = bool(
        subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=checkout, text=True
        ).strip()
    )
    subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--release",
            "-p",
            "dimensify",
            "--features",
            "splat-viewer-perf",
            "--example",
            "splat_viewer",
            "--target",
            "wasm32-unknown-unknown",
        ],
        cwd=checkout,
        env=clean_env(),
        check=True,
    )
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=checkout,
            env=clean_env(),
        )
    )
    wasm = (
        Path(metadata["target_directory"])
        / "wasm32-unknown-unknown/release/examples/splat_viewer.wasm"
    )
    subprocess.run(
        [
            "wasm-bindgen",
            "--target",
            "web",
            "--out-dir",
            str(output / "pkg"),
            str(wasm),
        ],
        env=clean_env(),
        check=True,
    )
    subprocess.run(
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
        ],
        cwd=checkout,
        env=clean_env(),
        check=True,
    )
    codec = (
        Path(metadata["target_directory"])
        / "wasm32-unknown-unknown/release/sog_codec.wasm"
    )
    subprocess.run(
        [
            "wasm-bindgen",
            "--target",
            "web",
            "--out-dir",
            str(output / "sog-codec"),
            str(codec),
        ],
        env=clean_env(),
        check=True,
    )
    shutil.copy2(
        checkout / "crates/splat/src/streaming/runtime/sog-decode-worker.mjs",
        output / "sog-decode-worker.mjs",
    )
    shutil.copy2(checkout / "splat-web/index.html", output / "index.html")
    after = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=checkout, text=True
    ).strip()
    dirty |= (
        bool(
            subprocess.check_output(
                ["git", "status", "--porcelain"], cwd=checkout, text=True
            ).strip()
        )
        or before != after
    )
    write(
        output / "build.json",
        {
            "commit": before,
            "sourceState": "modified" if dirty else "clean",
            "profile": "release",
            "feature": "splat-viewer-perf",
            "wasm_sha256": digest(output / "pkg/splat_viewer_bg.wasm"),
            "files": {
                str(path.relative_to(output)): digest(path)
                for path in output.rglob("*")
                if path.is_file()
            },
        },
    )
    verify_site(output)


def verify_site(site):
    build = json.loads((site / "build.json").read_text())
    required = {
        "index.html",
        "pkg/splat_viewer.js",
        "pkg/splat_viewer_bg.wasm",
        "sog-decode-worker.mjs",
        "sog-codec/sog_codec.js",
        "sog-codec/sog_codec_bg.wasm",
    }
    if build.get("feature") != "splat-viewer-perf" or not required <= set(
        build["files"]
    ):
        raise ValueError(
            "Browser package incomplete: require viewer and SOG worker/codec artifacts"
        )
    if any(digest(inside(site, name)) != sha for name, sha in build["files"].items()):
        raise ValueError("Served build bytes differ from recorded build")
    if digest(site / "pkg/splat_viewer_bg.wasm") != build["wasm_sha256"]:
        raise ValueError("Served WASM identity mismatch")
    return build
