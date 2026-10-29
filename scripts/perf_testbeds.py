"""Readable environment labels; render workloads have their own identities."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import uuid


def slug(value):
    value = re.sub(r"[^a-z0-9]+", "-", str(value).lower()).strip("-")
    if not value:
        raise ValueError("Missing testbed environment label")
    return value


def hardware(value):
    value = re.sub(r"\([RrTt][Mm]?\)", "", value)
    value = re.sub(r"\s*@\s*[0-9.]+\s*GHz.*$", "", value, flags=re.I)
    value = re.sub(r"\b(?:Intel|AMD|Apple|NVIDIA|GeForce|Platinum|Processor|CPU)\b", "", value, flags=re.I)
    value = re.sub(r"\b\d+-Core\b", "", value, flags=re.I)
    return slug(value)


def os_label(value):
    value = re.sub(r"\bDarwin\b", "macOS", value, flags=re.I)
    value = re.sub(r"\s+LTS\b", "", value, flags=re.I)
    return slug(value)


def architecture(value):
    return {"x86_64": "x64", "aarch64": "arm64"}.get(value, slug(value))


def checked_name(parts):
    name = "-".join(part for part in parts if part)
    if len(name) > 64:
        raise ValueError("Readable testbed name exceeds Bencher's 64-character limit; shorten the environment labels")
    return name


def cpu_name(system, arch, cpu, rust, suite="cpu"):
    version = re.search(r"rustc\s+(\S+)", rust)
    return checked_name(["pipeline-cpu" if suite == "pipeline" else "cpu", hardware(cpu),
                         os_label(system), architecture(arch), "rust" + slug(version[1] if version else rust)])


def scene_name(identity):
    adapter = identity["adapter"]
    name = adapter.get("name", "")
    backend = slug(adapter["backend"]).replace("browserwebgpu", "webgpu")
    if identity["kind"] == "native":
        parts = [hardware(name), backend, os_label(identity.get("os") or "unknown-os"),
                 architecture(identity.get("architecture") or "unknown-arch")]
        cpu = identity.get("cpu")
        if cpu and hardware(cpu) != hardware(name):
            parts.insert(1, hardware(cpu))
        for key in ("driver", "driver_info"):
            if adapter.get(key):
                parts.append(slug(adapter[key]))
        return checked_name(parts)
    if identity.get("physical_device"):
        target = identity["physical_device"]
        return checked_name(["mobile", hardware(target["model"]), os_label(target["os"]),
                             backend, "chrome" + slug(identity["browser"])])
    # Inventory is a label only, never an attestation of the active WebGPU GPU.
    if not name:
        inventory = adapter.get("browser_device", "")
        match = re.search(r"ANGLE Metal Renderer: ([^,]+)", inventory)
        match = match or re.search(r"ANGLE \([^,]+, ([^,]+),", inventory)
        if not match:
            raise ValueError("Cannot name browser GPU: active identity and browser inventory are unavailable")
        name = match[1]
    host = identity.get("hardware", "").split("/")[-1]
    system = re.match(r"(macOS|Windows|Linux)-([0-9.]+)", host, re.I)
    system = "-".join(system.groups()) if system else identity.get("platform", "unknown-os")
    parts = [hardware(name), backend, "chrome" + slug(identity["browser"]),
             os_label(system), architecture(identity["arch"])]
    parts.extend(slug(adapter[key]) for key in ("driver", "driver_info") if adapter.get(key))
    return checked_name(parts)


def ensure_name(project, identifier, name, rename_existing=False):
    """Preserve an existing testbed UUID and its thresholds while naming it."""
    args = ["--attempts", "3"]
    result = subprocess.run(["bencher", "testbed", "view", project, identifier, *args],
                            text=True, capture_output=True)
    if result.returncode:
        if not re.search(r"\b404\b", result.stderr):
            raise ValueError("Cannot inspect Bencher testbed")
        command = ["bencher", "testbed", "create", project, "--slug", identifier, "--name", name, *args]
    else:
        current = json.loads(result.stdout)
        if current["name"] == name:
            return
        if not rename_existing:
            # CI project keys can create named testbeds but cannot rename them.
            return
        command = ["bencher", "testbed", "update", project, current["uuid"], "--name", name, *args]
    updated = subprocess.run(command, text=True, capture_output=True)
    if updated.returncode:
        if "Project keys cannot rename testbeds" in updated.stderr:
            raise ValueError("Renaming existing testbeds requires a Bencher user API key; project keys only allow result uploads")
        raise ValueError("Cannot set readable Bencher testbed name")
    if json.loads(updated.stdout).get("name") != name:
        raise ValueError("Cannot set readable Bencher testbed name")
    if not result.returncode and json.loads(updated.stdout).get("uuid") != current["uuid"]:
        raise ValueError("Bencher rename changed testbed identity")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", default="dimensify")
    parser.add_argument("--plan", type=Path, required=True, help="Reviewed UUID/name rename plan")
    parser.add_argument("--apply", action="store_true", help="Rename with a user API key; preserve UUIDs/slugs/history")
    args = parser.parse_args()
    rows = json.loads(args.plan.read_text())
    seen = set()
    for row in rows:
        uuid.UUID(row["uuid"])
        name = row["name"]
        if row["uuid"] in seen or not name.strip() or len(name) > 64 or not name.isascii() or not name.isprintable():
            raise ValueError("Invalid or duplicate testbed rename")
        seen.add(row["uuid"])
    for row in rows:
        if args.apply:
            ensure_name(args.project, row["uuid"], row["name"], rename_existing=True)
        print(("Renamed: " if args.apply else "Planned: ") + row["name"], flush=True)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        print(f"Testbed naming failed: {error}", file=sys.stderr)
        sys.exit(1)
