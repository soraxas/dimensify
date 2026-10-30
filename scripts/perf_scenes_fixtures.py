"""Verify prepared fixture sets and bind saved measurements to them."""

import hashlib
import json
from pathlib import Path, PurePosixPath
import re

from perf_scenes_common import canonical, digest, inside


def fixture(path):
    """Recheck the complete prepared asset set, not just the entrypoint."""
    data = json.loads(Path(path).read_text())
    if data["schema"] != "inlier-gaussian-fixture/1" or not re.fullmatch(
        r"[a-z0-9-]+", data["dataset_id"]
    ):
        raise ValueError("Unsupported fixture manifest")
    root = Path(data["root"]).resolve()
    rows = sorted(
        (
            {key: row[key] for key in ("path", "bytes", "sha256")}
            for row in data["files"]
        ),
        key=lambda row: row["path"],
    )
    if not rows or len({row["path"] for row in rows}) != len(rows):
        raise ValueError("Empty or duplicate fixture files")
    for row in rows:
        actual = inside(root, row["path"])
        if actual.stat().st_size != row["bytes"] or digest(actual) != row["sha256"]:
            raise ValueError(f"Fixture size/hash mismatch: {row['path']}")
    identity = "sha256:" + hashlib.sha256(canonical(rows).encode()).hexdigest()
    if data["asset_identity"] != identity or data["entrypoint_relative"] not in {
        row["path"] for row in rows
    }:
        raise ValueError("Fixture set identity or entrypoint mismatch")
    data["entrypoint"] = str(inside(root, data["entrypoint_relative"]))
    if data.get("format") == "sog-v2-hierarchy":
        hierarchy_dependencies(
            root, data["entrypoint_relative"], {row["path"] for row in rows}
        )
    return data


def hierarchy_dependencies(root, entrypoint, verified):
    """Require metadata and every referenced texture in the hashed file set."""

    def reference(parent, name):
        if not isinstance(name, str) or any(char in name for char in "?#%"):
            raise ValueError("Unsafe hierarchy dependency path")
        inside(root, name)
        relative = str(PurePosixPath(parent) / name)
        path = inside(root, relative)
        if relative not in verified:
            raise ValueError(f"Unverified hierarchy dependency: {relative}")
        return path

    manifest = json.loads(inside(root, entrypoint).read_text())
    chunks = list(manifest["filenames"])
    if manifest.get("environment") is not None:
        chunks.append(manifest["environment"])
    for name in chunks:
        path = reference(PurePosixPath(entrypoint).parent, name)
        meta = json.loads(path.read_text())
        parent = (PurePosixPath(entrypoint).parent / name).parent
        for field in ("means", "quats", "scales", "sh0", "shN"):
            if field == "shN" and meta.get(field) is None:
                continue
            for texture in meta[field]["files"]:
                reference(parent, texture)


def bind_fixture(summary, data, reports):
    if (
        summary["dataset_id"] != data["dataset_id"]
        or summary["asset_identity"] != data["asset_identity"]
    ):
        raise ValueError("Saved fixture does not match measurement identity")
    if summary["kind"] == "native":
        if len(data["files"]) != 1 or any(
            report["source"]["sha256"] != data["files"][0]["sha256"]
            for report in reports
        ):
            raise ValueError("Native measurements do not match the verified fixture")
    elif summary["kind"] == "native-lod":
        source_hash = next(
            row["sha256"]
            for row in data["files"]
            if row["path"] == data["entrypoint_relative"]
        )
        if data.get("format") != "sog-v2-hierarchy" or any(
            report["source"]["sha256"] != source_hash for report in reports
        ):
            raise ValueError(
                "Native LOD measurements do not match the verified hierarchy"
            )
    else:
        source_hash = next(
            row["sha256"]
            for row in data["files"]
            if row["path"] == data["entrypoint_relative"]
        )
        identity = (
            "sha256:" + source_hash
            if data.get("format") == "sog-v2-bundle"
            else data["asset_identity"]
        )
        if any(
            report["metadata"]["asset"]["identity"] != identity for report in reports
        ):
            raise ValueError("Browser measurements do not match the verified fixture")
