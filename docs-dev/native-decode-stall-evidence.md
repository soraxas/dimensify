# Native streamed decode stalls

Native streamed SOG chunks were decoded synchronously in
`ChunkLoad::poll_measured` on the main thread. The browser already decodes in a
codec worker by default (`sog_decode` unset), so this affected native only.

## Diagnosis (2026-11-19)

Bevy `trace_chrome` traces of the optimized native viewer (Apple M5 Max /
Metal, `--frame`, hosted 3.9M-splat LOD scene) attribute every repeating main
thread stall to `update_sog_streams` → `poll_loads` → `decode_sync`:
42–65 ms per completed chunk, plus 177 ms for the first generation. The scene
keeps refining for more than 13 s, so stalls continue while a load effect
replays; replay itself added no frame over 25 ms.

The remaining long frames are pipeline compilation. Bevy 0.20 compiles
pipelines synchronously on macOS (`pipeline_cache.rs`, `create_pipeline_task`):
1.4 s and 2.6 s at a cold start and 448/267 ms when the first scene appears;
about 70–90 ms with a warm system shader cache.

## Change and checks

Native `ChunkLoad` decodes on the async compute pool (`Phase::Background`) and
collects the result on a later poll. Captured/pending test fixtures stay
synchronous so ordering tests remain deterministic; a dedicated test covers the
background path.

- Hosted scene, 16 s traces: `update_sog_streams` stalls 8 → 0; frames over
  25 ms 14 → 8 and 3 (two runs, all pipeline compilation in the first 3.6 s);
  frames completed 929 → 1039 and 1207.
- `native-lod`, playbot fixture, three rounds each, baseline `e148d35`
  (medians; read from per-round `raw.json`, the summarizer was not run):

| Case | Metric | Baseline | Candidate |
| --- | --- | --- | --- |
| static | first draw / completed-frame P99 | 119.0 / 5.3 ms | 120.7 / 5.2 ms |
| moving | completed-frame P99 | 5.3 ms | 5.3 ms |
| near-far-return | worst app update | 40.0 ms | 4.7 ms |
| near-far-return | app updates over 16 ms | 2 | 0 |
| near-far-return | completed-frame P99 | 8.5 ms | 6.7 ms |

Final displayed counts match in every case. Dirty local builds; diagnostic, not
publishable. Raw artifacts: `dist/decode-ab/` (ignored).

## Pipeline warm-up (2026-11-19)

The first visible frames of a streamed scene compiled the radix sort stages and
every Gaussian render variant the cloud used (sorted, stochastic sibling for
adaptive transparency, depth pick), stalling 267–448 ms in traces with a warm
system shader cache and longer cold. A `CloudPipelineWarmup` template (a
never-drawn child of the stream root, refined by the same settings systems as
real clouds) now specializes those keys through the shared
`cloud_pipeline_key` from the moment the stream entity exists.

- Pipeline-ready log on the hosted scene: all 9 splat pipelines (3 render
  variants, 6 radix stages) ready at 2.9 s, before the first cloud (~3.4 s);
  none compiled after it.
- 16 s traces, warm cache: no pipeline compilation at scene appearance; the
  only frames over 25 ms are at startup (≤ 67 ms). One run on a loaded machine
  (load average 20+) showed many 25–30 ms frames without any long system.
- GPU test `pipeline_warmup_compiles_every_variant_a_cloud_uses`: a drawn cloud
  with the warmed settings adds no splat pipeline.

On macOS this moves the compile into loading rather than removing it.

## Remaining evidence

- Browser long-task observations (worker handoff, upload, compilation).
- Bevy's own startup pipelines: 46 PBR/clustering/occlusion/atmosphere
  pipelines compile at launch (seconds on a cold shader cache); most are unused
  by the splat viewer.
- The warm-up can only start once the stream's metadata has arrived.
