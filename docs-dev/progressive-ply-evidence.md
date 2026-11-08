# Progressive PLY validation

The remote PLY path is opt-in through native `--progressive-ply` or browser
`progressive=1`. The existing whole-file decoder and LoadSplat API remain intact.

## Checks run (2026-11-07)

- Native splat library: 313 passed; 53 GPU/platform tests explicitly ignored.
- Explicit progressive GPU tests: three passed on Apple M5 Max / Metal. Four
  planes preserve earlier rows, unread/padded rows stay zero, encoded upload
  bytes grow linearly, and canceled publications are retired. Real offscreen
  captures match ordinary-cloud pixels for first/final batches with anisotropy,
  reversed depth and higher SH; render assets and scratch retire after teardown.
- Six progressive lifecycle tests passed using real throttled HTTP and native
  GPU allocation, with explicit readiness acknowledgments. They cover partial
  output before EOF, completion, truncation rollback, newer selection/commit
  authority and restoration in the current coordinate basis. These lifecycle
  acknowledgments alone are not draw evidence; the separate GPU test draws.
- Decoder checks cover arbitrary header/record splits, CRLF, SH0-3 whole-file
  attribute parity, bounded batches, schema/count/storage limits and malformed
  input. Header integer overflow is guarded before the upstream parser.
- Native standalone viewer builds; `--progressive-ply --help` accepts the flag.
- Native all-target and WASM library Clippy pass for dimensify_splat with
  `--no-deps -D warnings`. Standard `mise exec -- just ci-format` passes.

Raw logs and source hashes: `dist/progressive-ply-20261107/` (ignored artifacts).
The provenance records the base revision plus hashes of the uncommitted candidate.
The synthetic fixtures and validation elapsed times are not performance samples.

## Remaining evidence

Pixel Eagle reference comparisons, browser runtime/cancellation, representative
large-scene loading/frame-time comparisons, and physical mobile performance were
not run. No rendered PNG references or generated binaries were committed and no
baselines/thresholds changed. The opt-in path allocates/sorts the full declared
GPU capacity and decodes bounded chunks on the browser thread. Native outstanding
reads may remain until input arrives or the 120-second request timeout, while
receiver-owned input and queued data are released on cancellation.

See [performance coverage](splat-performance-todo.md#progressive-ply-delivery).
