"""Verify native benchmark captures against saved opaque RGBA8 evidence."""

import hashlib
from pathlib import Path
import re


def validate_capture(report, case, result_path):
    if result_path is None:
        raise ValueError("Native capture verification requires the saved result path")
    capture = case["capture"]
    directory = Path(result_path).resolve().parent
    filename = capture["file"]
    if (
        not isinstance(filename, str)
        or not filename
        or Path(filename).name != filename
        or filename in (".", "..")
    ):
        raise ValueError("Unsafe native capture path")
    path = directory / filename
    if path.is_symlink():
        raise ValueError("Unsafe native capture symlink")
    try:
        data = path.read_bytes()
    except OSError as error:
        raise ValueError("Missing or unreadable native capture") from error
    header = re.match(rb"P6\n([0-9]+) ([0-9]+)\n255\n", data)
    if header is None:
        raise ValueError("Malformed native capture PPM")
    width, height = (int(part) for part in header.groups())
    rgb = data[header.end() :]
    if (
        width <= 0
        or height <= 0
        or [width, height] != report["render"]["physical_resolution"]
        or len(rgb) != width * height * 3
    ):
        raise ValueError("Native capture dimensions or byte count mismatch")
    rgba = bytearray(width * height * 4)
    rgba[0::4], rgba[1::4], rgba[2::4], rgba[3::4] = (
        rgb[0::3],
        rgb[1::3],
        rgb[2::3],
        b"\xff" * (width * height),
    )
    if hashlib.sha256(rgba).hexdigest() != capture["rgba_sha256"]:
        raise ValueError("Native capture RGBA hash mismatch")
    count = sum(max(rgb[index : index + 3]) > 2 for index in range(0, len(rgb), 3))
    if count == 0 or count != capture["non_background_pixels"]:
        raise ValueError("Native capture is blank or non-background count mismatch")
