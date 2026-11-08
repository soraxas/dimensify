# Running the PLY → Potree converter

The converter lives on the **`feat/converter`** branch (not `main`) behind the
`convert` cargo feature.

## Build

```sh
cd potree-rs
git checkout feat/converter
cargo build --release --features convert --bin ply_to_potree
```

Binary lands at `target/release/ply_to_potree`.

## Run

```sh
./target/release/ply_to_potree <input.ply> <output_dir> [flags]
```

Writes `metadata.json`, `hierarchy.bin`, and octree node data (layout below)
into `<output_dir>` (created if missing). Prints a one-line quality summary
(node count, depth, points per node, spacing) when done.

| Flag | Default | Meaning |
| --- | --- | --- |
| `--name <s>` | input file stem | pointcloud name in metadata |
| `--projection <s>` | `""` | projection string in metadata |
| `--scale <f>` | `0.001` | quantization grid (meters); 1 mm |
| `--max-points-per-node <n>` | `10000` | split threshold, matches C++ PotreeConverter |
| `--max-depth <n>` | `20` | octree depth cap |
| `--encoding <s>` | `BROTLI` | `BROTLI` (SoA + brotli) or `DEFAULT` (raw AoS, Morton-sorted) |
| `--layout <s>` | `multi-file` | Octree node storage layout — see below |

### Octree node storage layout (`--layout`)

| Layout | Writes | Use when |
| --- | --- | --- |
| `multi-file` (default) | `octree/<name>.bin` per node, no `octree.bin` | Reading through this crate's own (patched) reader, in a browser. Avoids Chrome's HTTP cache being unreliable for concurrent Range requests sharing one cache key — each node is a plain GET returning 200, cached independently. No storage overhead. |
| `single-file` | Only `octree.bin`, addressed via HTTP Range requests | Feeding stock Potree tooling (`potree.js`, the C++ `PotreeConverter` viewer) that has never heard of `octreeLayout` — the original Potree v2 layout. |
| `dual` | Both `octree.bin` and `octree/<name>.bin` for every node | You need the same output to work in both stock Potree tooling and this crate's reader. Costs ~2x point-data storage to keep both readable. |

`hierarchy.bin` and `metadata.json` are identical in content regardless of
layout (aside from `metadata.json`'s `octreeLayout` key, set to
`"multi-file"` for `multi-file`/`dual`, absent for `single-file`) — layout
only changes which octree node-data files get written.

There is no `--seed`: sampling is fully deterministic (grid-based Poisson).

Library API: `potree::convert::streaming::convert_ply_streaming(input, output,
&ConvertPlyOptions { .. })` — same knobs, `ConvertPlyOptions::default()`
matches the table above except `encoding: "DEFAULT"`.

### Repack an existing stock Potree v2 dataset

The repacker creates the multi-file layout by copying each hierarchy node's
exact `octree.bin` byte range. It does not decode/re-encode points and does not
modify the source directory:

```sh
cargo run --features convert --bin potree_repack -- \
  path/to/stock-pcd path/to/new-multi-file-pcd
```

The output contains `metadata.json`, `hierarchy.bin`, and
`octree/<node-name>.bin`; its metadata is marked with
`"octreeLayout":"multi-file"`. The output path must not already exist so a
failed run cannot mix old and new node files.

### Input support

- PLY formats: ASCII, binary little-endian, binary big-endian.
- `x/y/z` (any scalar type) → quantized int32 positions.
- `red/green/blue` (or `r/g/b`) → uint16 `rgb` attribute.
- Every other property passes through as a typed extra attribute:
  - native scalar type preserved (uchar→uint8, ushort→uint16, float, …),
  - name normalized: CloudCompare `scalar_X` prefix stripped, LAS-style names
    canonicalized (`scalar_Intensity`→`intensity`, `return_number`→`return
    number`); unknown names keep the stripped form, collisions fall back to
    the original,
  - metadata `min`/`max` are the observed data range (scanned in pass 1).

### Output conventions (aligned with C++ PotreeConverter 2.x)

- `boundingBox` is **cubed** to the largest extent; the position attribute
  keeps the tight range snapped to the quantization grid.
- offset = data min; full f64 precision in metadata.
- Hierarchy chunked every 4 levels (`stepSize: 4`), 22-byte node records.
- Streaming: two passes over the PLY + temp bucket files in `$TMPDIR`, so
  memory stays flat (tested on a 27M-point / 595 MB PLY).

## Inspecting output

`examples/dump_potree.rs` decodes a Potree directory through this crate's own
reader and prints every node and point (positions + rgb) as text:

```sh
cargo build --release --features fs,tokio_dev --example dump_potree
./target/release/examples/dump_potree <potree_dir>   # 'node …' lines, then 'point x y z [r g b]'
```

Works on this converter's output and on C++ PotreeConverter output
(`DEFAULT`/`UNCOMPRESSED`/`BROTLI` encodings).

## Tests

```sh
cargo test --features convert,tokio_dev            # whole crate incl. converter suites
cargo test --features convert,tokio_dev --test streaming_roundtrip
```

`streaming_roundtrip` converts corner points and reloads them through the
reader — the end-to-end safety net after touching either side.

## Reference C++ PotreeConverter (for comparison)

Checkout at `../PotreeConverter`. Input is **LAS/LAZ only** (convert PLY→LAS
first, e.g. via numpy: read PLY dtype, `round(xyz/0.001)` → int32 LAS 1.2
records). Build (the checkout carries local macOS patches: `-fexperimental-
library` instead of TBB, an `__APPLE__` branch in unsuck, execution policies
bound by reference):

```sh
cmake -S ../PotreeConverter -B build -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build build -j 8
./build/PotreeConverter input.las -o outdir -m poisson --encoding UNCOMPRESSED
```

Comparison methodology that was used to validate parity (rust vs C++ on the
same points): run both, `dump_potree` both outputs, then match decoded
positions against the source at ±1 quantization-unit tolerance. Expected
result: both lossless (every input point recovered exactly once); rust matches
the source grid exactly for ~50–77 % of points, the C++ output systematically
drifts ~1 unit (its re-quantization bias) — that difference is cosmetic.
