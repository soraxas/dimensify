#!/usr/bin/env python3
"""Run a reviewed inventory of real GPU correctness cases, never an empty filter."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = "streaming::runtime::raster_handoff_tests"
RASTER_CASES = {
    ROOT + "::" + name
    for name in (
        "encoded_compositing_changes_real_streamed_pixels_and_returns_to_linear",
        "encoded_compositing_keeps_unsupported_ldr_camera_on_linear_path",
        "encoded_compositing_preserves_msaa_oit_and_resolution_override_fallback_pixels",
        "encoded_compositing_survives_resize_and_scene_unload",
        "encoded_compositing_preserves_transparent_mesh_order_and_opaque_depth",
        "encoded_compositing_isolates_cameras_sharing_a_target",
        "default_screen_space_zoom_out_and_return_restore_rendered_fine_detail",
        "decoded_leaf_footprints_survive_coarse_to_fine_render_handoff",
        "packed_handoff_tests::packed_gpu_residency_survives_cpu_cache_eviction",
        "packed_handoff_tests::persistent_scratch_capacity_shrinks_and_explicit_retirement_releases_it",
        "packed_handoff_tests::failed_projection_compilation_falls_back_and_can_recover",
        "packed_handoff_tests::projection_pipeline_is_only_compiled_for_opted_in_supported_work",
        "packed_handoff_tests::projected_footprint_preserves_edges_near_plane_and_projection_changes",
        "packed_handoff_tests::packed_pending_scene_removal_releases_unpublished_work",
        "packed_handoff_tests::packed_same_file_ids_are_isolated_across_scene_lifetimes",
        "packed_handoff_tests::packed_stream_refines_without_expanded_assets_and_retires_sources",
        "packed_handoff_tests::projected_stream_refines_reuses_slots_and_retires_cache",
        "packed_handoff_tests::packed_sources_bake_sort_and_draw_without_expanded_assets",
        "stochastic_tests::stochastic_coverage_is_binary_repeatable_and_switches_back_to_sorted",
        "stochastic_tests::stochastic_overlap_uses_depth_with_partial_coverage",
        "stochastic_tests::cpu_sort_after_compaction_restores_full_draw_count_and_returns_to_radix",
        "stochastic_tests::stochastic_projected_packed_lod_refines_and_returns_to_sorted",
        "turn_coverage_tests::packed_turn_has_coarse_pixels_before_selection_or_download_updates",
        "turn_coverage_tests::paged_turn_has_coarse_pixels_before_selection_or_download_updates",
        "turn_coverage_tests::compacted_turn_has_coarse_pixels_before_selection_or_download_updates",
    )
}
BUFFER_CASES = {
    "renderer::gpu::tests::resident_gpu_copies_reuse_sources_slots_and_retire_scene"
}


def test_args(group):
    args = ["--ignored", "--test-threads=1", "--nocapture"]
    if group == "raster":
        args += ["--skip", "producer_bounds::"]
    return args


def validate_inventory(output, expected):
    names = re.findall(r"^(.+): test$", output, re.MULTILINE)
    if len(names) != len(set(names)) or set(names) != expected:
        raise ValueError(
            f"GPU case inventory differs: missing={sorted(expected - set(names))}, extra={sorted(set(names) - expected)}, listed={len(names)}"
        )


def validate_summary(output, count):
    summaries = re.findall(
        r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored;", output
    )
    if summaries != [("ok", str(count), "0", "0")]:
        raise ValueError(
            f"Expected {count} executed GPU tests, zero failed/ignored; got {summaries}"
        )


def invoke(command, log):
    with subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    ) as child:
        lines = []
        for line in child.stdout:
            print(line, end="", flush=True)
            log.write(line)
            log.flush()
            lines.append(line)
        code = child.wait()
    if code:
        raise RuntimeError(f"GPU command failed with exit {code}")
    return "".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="dist/visual-gpu-tests")
    args = parser.parse_args()
    output = Path(args.out)
    output.mkdir(parents=True, exist_ok=False)
    report = {
        "schema": "dimensify-gpu-correctness/1",
        "status": "failed",
        "backendRequested": os.getenv("WGPU_BACKEND"),
        "groups": [],
    }
    try:
        for group, expected, selection in [
            ("raster", RASTER_CASES, ROOT),
            ("buffer", BUFFER_CASES, next(iter(BUFFER_CASES))),
        ]:
            base = [
                "cargo",
                "test",
                "--locked",
                "--profile",
                "ci",
                "-p",
                "dimensify_splat",
                "--features",
                "test-support",
                "--lib",
                selection,
                "--",
            ]
            with (output / f"{group}.log").open("w") as log:
                inventory = invoke(base + test_args(group) + ["--list"], log)
                validate_inventory(inventory, expected)
                result = invoke(base + test_args(group), log)
                validate_summary(result, len(expected))
            report["groups"].append(
                {"name": group, "passed": len(expected), "cases": sorted(expected)}
            )
        report["status"] = "passed"
    finally:
        (output / "result.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError) as error:
        print(f"GPU correctness failed: {error}", file=sys.stderr)
        sys.exit(1)
