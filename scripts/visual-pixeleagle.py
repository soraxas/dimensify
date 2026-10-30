#!/usr/bin/env python3
"""Optional reporting to fixed Pixel Eagle runs; never approves a golden record.

API schema: https://pixel-eagle.com/api/openapi.json (checked 2026-10-29).
Local image acceptance remains a separate, credential-free check.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import struct
import sys
import time
import urllib.error
import urllib.request
import uuid
import zlib


CASES = ("anisotropic", "overlap-front", "overlap-reverse", "sh-front", "sh-reverse")
IDENTITY = (
    "lane",
    "target",
    "backend",
    "adapter",
    "driver",
    "browser",
    "viewport",
    "sceneHash",
    "settingsHash",
)
ORIGIN = "https://pixel-eagle.com"


class ReportingError(Exception):
    """A bounded, safe-to-log failure with no server response or token included."""

    def __init__(self, message, publication=None):
        super().__init__(message)
        self.publication = publication


def require(condition, message):
    if not condition:
        raise ReportingError(message)


def integer(value):
    return type(value) is int and value >= 0


def nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def identifier(value):
    require(integer(value), "Invalid run identifier")
    return value


def flatten_metadata(metadata):
    require(isinstance(metadata, dict), "Missing capture metadata identity")
    result = {}
    for key in IDENTITY:
        value = metadata.get(key)
        if key == "viewport":
            require(
                value == {"width": 256, "height": 256},
                "Invalid capture viewport identity",
            )
            result[key] = json.dumps(value, sort_keys=True, separators=(",", ":"))
        else:
            require(nonempty(value), "Missing capture metadata identity: " + key)
            if key in ("sceneHash", "settingsHash"):
                require(
                    re.fullmatch("[0-9a-f]{64}", value),
                    "Invalid capture hash identity: " + key,
                )
            result[key] = value
    return result


def validate_source(value):
    keys = {"repository", "runId", "commit", "artifact", "conclusion"}
    require(
        isinstance(value, dict) and set(value) == keys,
        "Malformed trusted source provenance",
    )
    repository = value["repository"]
    require(
        isinstance(repository, str)
        and len(repository) <= 200
        and re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository),
        "Invalid trusted source repository",
    )
    require(
        type(value["runId"]) is int and 0 < value["runId"] <= 9007199254740991,
        "Invalid trusted source run identifier",
    )
    require(
        isinstance(value["commit"], str)
        and re.fullmatch("[0-9a-f]{40}", value["commit"]),
        "Invalid trusted source commit",
    )
    require(
        isinstance(value["artifact"], str)
        and value["artifact"]
        in (
            "visual-native-linux",
            "visual-browser-linux",
            "visual-native-metal",
            "visual-native-dx12",
        ),
        "Invalid trusted source artifact",
    )
    require(
        isinstance(value["conclusion"], str)
        and value["conclusion"]
        in (
            "success",
            "failure",
            "cancelled",
            "timed_out",
            "action_required",
            "neutral",
            "skipped",
            "stale",
            "startup_failure",
        ),
        "Invalid trusted source conclusion",
    )
    return dict(value)


def load_source(file):
    file = Path(file)
    require(
        not file.is_symlink() and file.is_file() and file.stat().st_size <= 4096,
        "Missing/invalid trusted source sidecar",
    )
    try:
        value = json.loads(file.read_text())
    except (ValueError, UnicodeError):
        raise ReportingError("Malformed trusted source sidecar") from None
    return validate_source(value)


def validate_png(data):
    """Validate bounded RGBA8 captures, CRCs and complete decompressed scanlines."""
    require(
        len(data) <= 4 * 1024 * 1024 and data[:8] == b"\x89PNG\r\n\x1a\n",
        "Invalid PNG signature/size",
    )
    offset = 8
    kinds = []
    compressed = bytearray()
    while offset < len(data):
        require(offset + 12 <= len(data), "Truncated PNG chunk")
        length = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4 : offset + 8]
        require(offset + 12 + length <= len(data), "Truncated PNG payload")
        value = data[offset + 8 : offset + 8 + length]
        checksum = struct.unpack_from(">I", data, offset + 8 + length)[0]
        require(zlib.crc32(kind + value) == checksum, "PNG CRC mismatch")
        if not kinds:
            require(kind == b"IHDR", "Missing first PNG IHDR")
        if kind == b"IHDR":
            require(
                not kinds and value == struct.pack(">IIBBBBB", 256, 256, 8, 6, 0, 0, 0),
                "Invalid PNG dimensions/format; require 256x256 RGBA8",
            )
        elif kind == b"IDAT":
            require(
                not kinds or kinds[-1] in (b"IHDR", b"IDAT") or b"IDAT" not in kinds,
                "Noncontiguous PNG image data",
            )
            compressed.extend(value)
        elif kind == b"IEND":
            require(
                length == 0 and compressed and offset + 12 == len(data),
                "Invalid PNG end",
            )
        else:
            require(kind and 97 <= kind[0] <= 122, "Unsupported critical PNG chunk")
        kinds.append(kind)
        offset += 12 + length
    require(kinds and kinds[-1] == b"IEND", "Missing PNG end")
    expected = (256 * 4 + 1) * 256
    try:
        decoder = zlib.decompressobj()
        raw = decoder.decompress(bytes(compressed), expected + 1)
        require(
            decoder.eof
            and not decoder.unused_data
            and not decoder.unconsumed_tail
            and len(raw) == expected,
            "Invalid PNG decompressed length",
        )
        require(
            all(raw[row * 1025] <= 4 for row in range(256)), "Invalid PNG row filter"
        )
    except zlib.error:
        raise ReportingError("Invalid compressed PNG data") from None


def load_captures(directory):
    directory = Path(directory)
    require(
        not directory.is_symlink() and directory.is_dir(), "Invalid capture directory"
    )
    manifest_path = directory / "captures.json"
    require(
        not manifest_path.is_symlink()
        and manifest_path.is_file()
        and manifest_path.stat().st_size <= 1024 * 1024,
        "Missing/invalid captures.json",
    )
    try:
        manifest = json.loads(manifest_path.read_text())
    except (ValueError, UnicodeError):
        raise ReportingError("Malformed captures.json") from None
    require(
        isinstance(manifest, dict)
        and manifest.get("schema") == "dimensify-visual-captures/1",
        "Unknown capture manifest schema",
    )
    metadata = flatten_metadata(manifest.get("metadata"))
    captures = manifest.get("captures")
    require(
        isinstance(captures, list) and len(captures) == len(CASES),
        "Missing capture inventory",
    )
    names, files, images = set(), set(), []
    for entry in captures:
        require(isinstance(entry, dict), "Malformed capture entry")
        name, file = entry.get("name"), entry.get("file")
        require(
            isinstance(name, str) and name in CASES and name not in names,
            "Unknown/duplicate capture name",
        )
        require(
            isinstance(file, str)
            and re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]*\.png", file)
            and file not in files,
            "Unsafe/duplicate capture PNG path",
        )
        require(
            type(entry.get("width")) is int
            and type(entry.get("height")) is int
            and entry["width"] == 256
            and entry["height"] == 256,
            "Invalid declared capture dimensions",
        )
        source = directory / file
        require(
            not source.is_symlink()
            and source.is_file()
            and source.stat().st_size <= 4 * 1024 * 1024,
            "Missing/unsafe capture PNG",
        )
        data = source.read_bytes()
        validate_png(data)
        names.add(name)
        files.add(file)
        images.append(
            {
                "name": name,
                "file": file,
                "data": data,
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
    require(names == set(CASES), "Capture inventory mismatch")
    disk_files = set()
    for file in directory.iterdir():
        require(
            not file.is_symlink() and not file.is_dir(),
            "Unsafe capture directory entry",
        )
        if file.suffix.lower() == ".png":
            disk_files.add(file.name)
    require(files == disk_files, "PNG inventory mismatch")
    return metadata, images


def screenshot(value):
    require(
        isinstance(value, dict)
        and nonempty(value.get("name"))
        and nonempty(value.get("hash")),
        "Malformed service screenshot",
    )
    return value["name"]


def validate_comparison(value, expected_names, run_id, baseline_run, project=None):
    require(isinstance(value, dict), "Malformed comparison response")
    require(
        integer(value.get("from"))
        and integer(value.get("to"))
        and value["from"] == run_id
        and value["to"] == baseline_run,
        "Comparison run identity mismatch",
    )
    require(integer(value.get("comparison_id")), "Missing comparison identifier")
    try:
        uuid.UUID(value.get("project_id", ""))
    except (ValueError, AttributeError, TypeError):
        raise ReportingError("Invalid comparison project identity") from None
    require(
        project is None or value["project_id"] == project,
        "Comparison project identity mismatch",
    )
    for key in ("missing", "new", "diff", "unchanged"):
        require(
            isinstance(value.get(key), list),
            "Missing/malformed comparison field: " + key,
        )
    require(
        not value["missing"] and not value["new"], "Comparison has missing/new captures"
    )
    names = [screenshot(entry) for key in ("unchanged", "diff") for entry in value[key]]
    require(
        len(names) == len(expected_names) and set(names) == set(expected_names),
        "Comparison inventory missing/extra/duplicate",
    )
    pending = False
    for entry in value["diff"]:
        require(
            nonempty(entry.get("previous_hash")), "Missing previous screenshot identity"
        )
        difference = entry.get("diff")
        if difference in ("Unknown", "Processing"):
            pending = True
        else:
            require(
                isinstance(difference, dict) and set(difference) == {"Done"},
                "Unknown comparison completion status",
            )
            error = difference["Done"]
            require(
                type(error) in (int, float) and math.isfinite(error) and error >= 0,
                "Invalid numeric comparison difference",
            )
            require(error == 0, "Comparison has visual differences")
    return "pending" if pending else "passed"


class NoRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ReportingError("Service redirect rejected")


class Transport:
    def __init__(self, token, timeout=15):
        require(
            nonempty(token)
            and token.isascii()
            and all(33 <= ord(char) <= 126 for char in token),
            "Missing/invalid PIXEL_EAGLE_TOKEN",
        )
        self.token = token
        self.timeout = timeout
        self.opener = urllib.request.build_opener(NoRedirects())

    def __call__(self, method, route, body, content_type):
        headers = {
            "Authorization": "Bearer " + self.token,
            "Accept": "application/json",
        }
        if content_type:
            headers["Content-Type"] = content_type
        request = urllib.request.Request(
            ORIGIN + route, data=body, headers=headers, method=method
        )
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                require(
                    response.headers.get_content_type() == "application/json",
                    "Service returned non-JSON response",
                )
                data = response.read(2 * 1024 * 1024 + 1)
                require(len(data) <= 2 * 1024 * 1024, "Service response too large")
                try:
                    return response.status, json.loads(data)
                except (ValueError, UnicodeError):
                    raise ReportingError("Service returned malformed JSON") from None
        except urllib.error.HTTPError as error:
            raise ReportingError(f"Service HTTP status {error.code}") from None
        except (urllib.error.URLError, TimeoutError, OSError):
            raise ReportingError("Service connection failed or timed out") from None


def multipart(image):
    boundary = "dimensify-" + uuid.uuid4().hex
    prefix = (
        f'--{boundary}\r\nContent-Disposition: form-data; name="screenshot"\r\n\r\n{image["name"]}\r\n'
        f'--{boundary}\r\nContent-Disposition: form-data; name="data"; filename="{image["file"]}"\r\nContent-Type: image/png\r\n\r\n'
    ).encode()
    return prefix + image[
        "data"
    ] + f"\r\n--{boundary}--\r\n".encode(), "multipart/form-data; boundary=" + boundary


def publish(
    captures,
    *,
    baseline_run=None,
    project=None,
    upload_only=False,
    source,
    transport,
    poll_interval=1,
    max_polls=20,
):
    require(
        type(upload_only) is bool
        and (
            (upload_only and baseline_run is None)
            or (not upload_only and integer(baseline_run))
        ),
        "Choose an explicit baseline run or --upload-only bootstrap",
    )
    require(
        type(max_polls) is int
        and 1 <= max_polls <= 60
        and type(poll_interval) in (int, float)
        and math.isfinite(poll_interval)
        and 0 <= poll_interval <= 10,
        "Invalid bounded polling settings",
    )
    source = validate_source(source)
    metadata, images = load_captures(captures)

    def request(method, route, body=None, content_type=None, expected=200):
        status, value = transport(method, route, body, content_type)
        require(
            type(status) is int and status == expected,
            f"Unexpected service HTTP status {status if type(status) is int else 'invalid'}",
        )
        return value

    if not upload_only:
        baseline = request("GET", f"/runs/{baseline_run}")
        require(
            isinstance(baseline, dict)
            and integer(baseline.get("id"))
            and baseline["id"] == baseline_run
            and integer(baseline.get("timestamp")),
            "Malformed baseline run identity",
        )
        previous = baseline.get("metadata")
        require(
            isinstance(previous, dict)
            and all(previous.get(key) == metadata[key] for key in IDENTITY),
            "Baseline metadata identity mismatch",
        )
        require(
            isinstance(baseline.get("screenshots"), list),
            "Missing baseline screenshots",
        )
        names = [screenshot(entry) for entry in baseline["screenshots"]]
        require(
            len(names) == len(CASES) and set(names) == set(CASES),
            "Baseline screenshot inventory mismatch",
        )
    # Trusted workflow identity is separate from compatibility metadata. In
    # particular, a source commit must not select or approve a reference image.
    provenance = {
        "source" + key[0].upper() + key[1:]: str(value) for key, value in source.items()
    }
    run_metadata = {**metadata, **provenance, "compare_with_golden": False}
    run = request(
        "POST",
        "/runs",
        json.dumps(run_metadata).encode(),
        "application/json",
        expected=201,
    )
    require(isinstance(run, dict), "Malformed created run response")
    run_id = identifier(run.get("id"))
    require(
        upload_only or run_id != baseline_run, "Created run matches baseline identifier"
    )
    result = {
        "schema": "dimensify-visual-publication/1",
        "state": "uploaded-no-comparison",
        "passed": False,
        "runId": run_id,
        "baselineRunId": baseline_run,
        "metadata": metadata,
        "source": source,
        "captures": [
            {"name": image["name"], "sha256": image["sha256"]} for image in images
        ],
    }
    if project:
        result["url"] = f"{ORIGIN}/project/{project}/run/{run_id}" + (
            "" if upload_only else f"/compare/{baseline_run}"
        )
    try:
        for image in images:
            body, content_type = multipart(image)
            uploaded = request("POST", f"/runs/{run_id}", body, content_type)
            require(
                screenshot(uploaded) == image["name"],
                "Uploaded screenshot identity mismatch",
            )
        if upload_only:
            return result
        route = f"/runs/{run_id}/compare/{baseline_run}"
        value = request("POST", route)
        for attempt in range(max_polls + 1):
            state = validate_comparison(value, CASES, run_id, baseline_run, project)
            if state == "passed":
                result.update(state="compared", passed=True, comparison=value)
                return result
            if attempt == max_polls:
                break
            time.sleep(poll_interval)
            value = request("GET", route)
        raise ReportingError("Comparison incomplete after bounded polling")
    except ReportingError as error:
        result.update(state="failed", passed=False, error=str(error))
        error.publication = result
        raise


def write_summary(result, file):
    state = result["state"]
    verdict = str(result["passed"]).lower()
    # Every interpolated URL is built locally from validated UUIDs/integers and
    # a bounded repository name. No API response text is interpreted as Markdown.
    line = f"Pixel Eagle publication: {state}; comparison passed: {verdict}."
    if result.get("url"):
        label = (
            "Pixel Eagle run"
            if state == "uploaded-no-comparison"
            else "Pixel Eagle comparison"
        )
        line += f" [{label}]({result['url']})."
    if result.get("source"):
        source = result["source"]
        line += f" [Source workflow](https://github.com/{source['repository']}/actions/runs/{source['runId']})."
    with Path(file).open("a", encoding="utf8") as summary:
        summary.write(line + "\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--captures", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--baseline-run", type=int)
    mode.add_argument("--upload-only", action="store_true")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--source",
        type=Path,
        required=True,
        help="Trusted source-run sidecar written outside the downloaded artifact",
    )
    parser.add_argument(
        "--project", help="Project UUID used to validate responses and link the report"
    )
    parser.add_argument("--request-timeout", type=int, default=15)
    parser.add_argument("--max-polls", type=int, default=20)
    parser.add_argument("--poll-interval", type=float, default=1)
    args = parser.parse_args(argv)
    try:
        require(1 <= args.request_timeout <= 30, "Invalid bounded request timeout")
        if args.project:
            try:
                uuid.UUID(args.project)
            except ValueError:
                raise ReportingError("Invalid project UUID") from None
        source, output = args.captures.resolve(), args.out.resolve()
        require(
            source != output
            and source not in output.parents
            and output not in source.parents,
            "Output overlaps captures",
        )
        args.out.mkdir()
    except (ReportingError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 1
    source = None
    try:
        source = load_source(args.source)
        result = publish(
            args.captures,
            baseline_run=args.baseline_run,
            project=args.project,
            upload_only=args.upload_only,
            source=source,
            transport=Transport(
                os.environ.get("PIXEL_EAGLE_TOKEN"), args.request_timeout
            ),
            poll_interval=args.poll_interval,
            max_polls=args.max_polls,
        )
    except (ReportingError, OSError) as error:
        # External text, response bodies, environment values and credentials are never logged.
        message = (
            str(error)
            if isinstance(error, ReportingError)
            else "Unable to read capture files"
        )
        result = getattr(error, "publication", None) or {
            "schema": "dimensify-visual-publication/1",
            "state": "failed",
            "passed": False,
            "error": message,
        }
        if source is not None:
            result["source"] = source
    (args.out / "publication.json").write_text(
        json.dumps(result, indent=2, allow_nan=False) + "\n"
    )
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        write_summary(result, os.environ["GITHUB_STEP_SUMMARY"])
    print(
        f"Pixel Eagle publication: {result['state']}; comparison passed: {str(result['passed']).lower()}"
    )
    if result.get("error"):
        print(result["error"], file=sys.stderr)
    return 0 if result["passed"] or result["state"] == "uploaded-no-comparison" else 1


if __name__ == "__main__":
    sys.exit(main())
