# Native SOG expansion experiment

This is the source and dependency lock for the CPU-only probe used in the
[repeatability investigation](../../../../docs-dev/sog-repeatability-evidence.md).
It is an experimental reproducer, separate from the active Cargo workspace and
Bencher workloads. It initializes no renderer or GPU.

Stage the same harness against each codec revision in a new ignored directory.
The `codec` dependency is deliberately supplied from Git rather than copied into
this source directory. Revisions before the codec moved to `crates/` use
`vendor/sog_codec` as the archive path; later ones use `crates/sog_codec`.
From the repository root, for the historical baseline:

```sh
probe_rev=1c1cbe83443bc2da6adf164943012eb4730ba31b
probe_dir=dist/sog-probe-baseline
test ! -e "$probe_dir"
mkdir -p "$probe_dir/codec"
cp tools/perf/probes/sog-expansion/Cargo.toml tools/perf/probes/sog-expansion/Cargo.lock "$probe_dir/"
cp -R tools/perf/probes/sog-expansion/src "$probe_dir/"
git archive "$probe_rev:vendor/sog_codec" | tar -x -C "$probe_dir/codec"
mise exec -- cargo build --locked --release --manifest-path "$probe_dir/Cargo.toml"
```

Repeat with `probe_rev=f1f11489df23d636ee6b7716fe5b1a5294c10a2c` and a different
`probe_dir` for the measured candidate. Build both before timing; retain the
full revisions, compiler version, source/lock/executable hashes and run order.
The release profile fixes opt-level 3, thin LTO and one codegen unit. The lock
file preserves the experiment's dependency selection; do not regenerate it to
compare these historical revisions.

Use a locally prepared, hash-verified bundled SOG from the external
[fixture preparation workflow](../../../../docs/performance.md#real-scene-fixture-preparation).
Retain its `fixture.json` and recheck it before/after measurement. The probe
does not download data or independently validate the fixture catalog identity.
For example:

```sh
dist/sog-probe-baseline/target/release/sog_decode_probe /path/to/scene.sog > dist/sog-probe-baseline/round-1.json
```

Run three fresh processes per revision and scene, alternating revision order
per round and reversing the starting revision for the other scene. Each process
warms twice and records 15 calls to the shared production `decode_expanded` at
SH3. Each call includes WebP decoding and output allocation; reading/extracting
the ZIP, parsing metadata, hashing outputs and dropping outputs are outside the
timed interval. No engine validation/bounds, upload, first draw, browser worker
or display timing is measured.

Check all output hashes agree across processes and revisions. Compute a median
within each process, then the median of the three process medians; retain every
raw sample and process median. The 15 calls within a process are not independent
trials. Lower warm expansion medians do not establish cold loading or FPS gains.
This probe emits diagnostic JSON and has no Bencher uploader or regression gate.

The source is retained as used in the experiment, including its fixed iteration
counts and compact formatting. Captures, model payloads, codec snapshots and
generated binaries remain outside this directory.
