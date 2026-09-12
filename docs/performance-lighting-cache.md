# Lighting-cache performance check — 2026-09-12

Apple M4 MacBook Air, 32GB, on AC power with Low Power Mode off. Local release
WASM, Apple Metal WebGPU adapter, browser panel visibly presented, 1280 × 720
viewport. Balanced world quality, seed 1337, 400% ground cover, Bloom 100%,
shadows/reflections/enclosure enabled, unchanged godrays. Clear weather, 1× clock.
No builds or tests ran during measurement; only one game page rendered at a time.

| Visible-browser scene | 420p FPS | 720p FPS | 420p p95 ms | 720p p95 ms |
|---|---:|---:|---:|---:|
| Checkpoint forest | 41.3 | 30.4 | 33.7 | 34.3 |
| Updated forest | 58.0 | 45.0 | 17.5 | 33.5 |
| Updated walking + streaming | 56.5 | 47.3 | 33.2 | 33.4 |
| Updated lakeside | 60.0 | 60.0 | 17.5 | 17.4 |

420p uses 747 × 420 scene pixels; 720p uses 1280 × 720. Static comparisons include
15 seconds of a slow camera sweep at each resolution after streaming drains;
walking includes 30 seconds and roughly 237m per resolution. Raw samples are in
[lighting-cache-2026-09-12.json](benchmarks/lighting-cache-2026-09-12.json).

The checkpoint is `fec15527ee9e412438855c2ad3f61810dfcf87be`, tagged
`checkpoint/pre-lighting-cache-20260912`. The source and compiled WASM were
unchanged when it was served separately for the control. The new build is identified
by its WASM SHA-256 in the raw results. This is one foreground comparison, not a
universal speedup or guaranteed 60 FPS. Weather history, clock phase and machine
load can still vary. The earlier post-reboot run was on battery; do not attribute
its difference to code alone. Earlier exploratory tests this session accidentally
had the browser panel hidden despite the page reporting visible; those results
are excluded here.

The updated forest uses 337.5 MiB of mesh buffers vs 332.7 MiB at the checkpoint.
Sparse middle-distance alpha cards add modest mesh memory and overdraw but retain
original crown shapes. Cloud-shadow density adds a 256 KiB texture, sampled from a
64m world lattice. Highlight response simplifies only with distance; full material
maps, foliage transmission and directional shadows remain. Godray integration
resolution and sample count are unchanged; unused reconstruction fetches are skipped.

GPU draw-span telemetry excludes water simulation and cloud-cache refresh.
Individual Apple GPU pass intervals overlap and must not be added together or
interpreted as exclusive costs. The table therefore uses browser frame intervals.

Verification: focused cache/LOD/geometry Rust tests, release WASM build, actual WASM
worker-packet validation, UI/worker lifecycle checks, WebGPU startup validation,
and fully streamed captures in three locations. Still images do not establish
absence of all temporal shimmer or LOD popping. The LOD transition uses hysteresis,
not crossfade.

Local before/after images and an interactive comparison are retained under
`output/lighting-cache-performance/`. Each capture waits for all pending chunks,
uses matching camera/lighting presets and 720p, and leaves animated wind and water
running. No deployment was performed.
