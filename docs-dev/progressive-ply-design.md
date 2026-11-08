# Progressive network PLY loading

Approved scope: opt-in progressive binary little-endian Gaussian PLY loading,
with the existing LoadSplat and whole-file PLY/SOG paths preserved.

Transport emits bounded decoded batches. A Bevy plugin manages one generation,
cancellation, partial geometry, completion and rollback. The renderer allocates
one full declared-capacity GPU cloud and appends only new rows; zero-initialized
unreceived rows remain invisible. The existing GPU publication/readiness boundary
orders uploads before draws. Sorting uses declared capacity throughout loading.
No downstream-specific policy, dependency or authorization is introduced.

The standalone viewer exposes --progressive-ply on native and progressive=1 in
browser launch links. It applies to remote PLY only. ASCII/big-endian PLY falls
back to the existing whole-file decoder. Preserve platform allocation and GPU capacity limits, without a fixed input-size cutoff.
Keep previous successful content until the first render-ready batch; retain it
for rollback if a later batch fails. New selections retire old queues and GPU
ownership. Completed stream buffers remain drawable without retaining input.

Validation: compare all decoded attributes and final bounds against whole-file
loading, split headers/records at arbitrary boundaries, reject malformed and
truncated streams, test bounded queues and cancellation, and exercise real HTTP
partial delivery and GPU append/readback. Affected visual fixtures: anisotropic,
overlap/reversed depth and SH color; compare existing native analytic captures.
Performance scenarios: full-file PLY/SOG import, throttled progressive first
batch, append upload byte counts and fixed-camera full-capacity sorting. Retain
raw outputs; unavailable Pixel Eagle or matched timing evidence stays explicit.
