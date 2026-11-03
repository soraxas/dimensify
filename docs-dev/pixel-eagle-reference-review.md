# Pixel Eagle reference identity review (2026-11-03)

Automatic reporting fails before image comparison because the fixed references
predate stochastic settings. Commit `11053e1` added `transparency`,
`stochastic_seed` and `stochastic_sample` to serialized `CloudSettings`.
The analytic scenes still use sorted rendering with seed/sample zero.

Reconstructing the canonical settings JSON reproduces both hashes exactly:

- Original: `1e4a637c9ecbadeb386c93ffac181f57fb94e45937e63e0ed90757a3f458279e`.
- Current: `bf848b8a12c7c123f5abb9df6a5f938df80ed0b979d5624545ed2d6d77616d22`.

The correct action is an explicit review of references with current metadata.
Removing fields from the hash or skipping identity validation would conceal
settings differences. The reporter now names mismatched identity fields without
printing arbitrary service values, and still refuses to upload on a mismatch.

## Reviewed evidence

Use the successful scheduled [visual run 41388704186](https://github.com/soraxas/dimensify/actions/runs/41388704186),
main revision `5d956e77b7e5e05ef64fd693227666f3acb6e937`. Download all four capture
artifacts. Verify the historical reference PNG hashes against the corresponding
Pixel Eagle screenshot inventory, then compare decoded RGBA against current main.
All 20 images are exactly identical: zero changed channels and maximum error zero.
All other compatibility identity fields match in every lane.

Upload those unmodified current captures with authoritative source provenance;
retain `compare_with_golden:false`. Explicit review-only comparisons against the
original runs pass in Pixel Eagle for all five cases in every lane:

| Artifact | Original run | Proposed fixed run | Review comparison |
| --- | --- | --- | --- |
| `visual-browser-linux` | 1 | 47 | [47 versus 1](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/47/compare/1) |
| `visual-native-linux` | 2 | 48 | [48 versus 2](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/48/compare/2) |
| `visual-native-metal` | 10 | 49 | [49 versus 10](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/49/compare/10) |
| `visual-native-dx12` | 11 | 50 | [50 versus 11](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/50/compare/11) |

These are review comparisons across the documented settings-schema change, not
passed production reporting or approval of a new reference. No golden approval
endpoint or tolerance override was used. The fixed-ID variable was initially
left unchanged. Original runs
and failed reports remain available. Previously observed one-level Vulkan
variation remains a separate issue; this exact match does not explain its cause
or prove that future captures cannot differ.

Contact sheets (original/current/amplified difference), per-image hashes and
channel statistics, source captures and API responses remain in ignored
`dist/pixel-eagle-review-20261103/`. No images or downloaded fixture data enter Git.

## Approved activation (2026-11-03)

The user approved this migration. Pull main `1654aab` (merged PR #23; no renderer
changes since the reviewed capture revision), update the fixed IDs and verify
normal identity-checked reporting against a completed visual run from main.
The variable is now configured as follows; the command uses the completed
visual run from current main:

```sh
gh variable set PIXEL_EAGLE_BASELINES --repo soraxas/dimensify \
  --body '{"visual-browser-linux":47,"visual-native-linux":48,"visual-native-metal":49,"visual-native-dx12":50}'
gh workflow run visual-report.yml --repo soraxas/dimensify \
  -f source_run=41550016687 -f upload_only=false
```

Normal reporting is restored. All four lanes pass in automatic reporting
[run 41552003909](https://github.com/soraxas/dimensify/actions/runs/41552003909),
using source visual [run 41550016687](https://github.com/soraxas/dimensify/actions/runs/41550016687)
on `1654aabf5b9ff4b8363abdf2933bf3a9329a0284`. Saved publications each state
`compared`, `passed:true` and the expected source revision and fixed reference.

| Lane | Verified comparison |
| --- | --- |
| Browser WebGPU | [57 versus 47](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/57/compare/47) |
| Native Vulkan | [55 versus 48](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/55/compare/48) |
| Native Metal | [56 versus 49](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/56/compare/49) |
| Native DX12 | [58 versus 50](https://pixel-eagle.com/project/6a4fd288-52c2-4985-a8e5-7463e5c1ef98/run/58/compare/50) |

The older reviewed source also passes all four normal reporting jobs in
[run 41551912287](https://github.com/soraxas/dimensify/actions/runs/41551912287).
No renderer behavior was changed for this repair; the current source visual run
also passes analytic assertions on every captured platform.
