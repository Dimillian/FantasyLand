# Local rendering upgrade — 2026-10-02

The upgrade was implemented in an isolated `codex/rendering-upgrade` worktree
from main `76f77ea`, then brought into the main checkout with its local evidence.
The engine remains our Rust renderer built on wgpu/WebGPU/Metal, with procedural
world geometry and a forward HDR scene pass. The three upgrades extend that
renderer; they do not introduce an external game engine.

## Implementation

* GTAO integrates depth horizons at half resolution, filters by depth, and
  resolves visibility to the scene size. It changes indirect diffuse lighting.
  The opaque depth prepass is reused by the main color pass. Nearby grass is
  included; distant blades keep their existing geometry and root shading.
* Stable celestial shadow cascades use a 1536² near map over an 80m radius and
  a 1024² far map over 320m, blended at 56–72m. Near density triples relative to
  the original 2048²/320m map; total shadow storage decreases from 16 to 13MiB.
  Far tree casters use the existing silhouette-preserving reflection proxies.
* Local one-bounce diffuse GI traces against a worker-generated 0.5m geometry
  proxy in a 64×24×64m volume. 1536 directional probes retain irradiance and
  visibility moments. A 24-probe update budget scrolls overlapping history,
  fills invalid probes and refreshes when lighting or doors change. A depth-aware
  half-resolution screen cache replaces per-fragment probe gathering. Water
  reflections sample the probe field directly and refresh during convergence.

Sunlight, material specular response, emission, weather and post-processing
remain separate from indirect illumination. Unavailable probe coverage falls
back to existing ambient lighting. If worker generation fails, the browser
uses ambient fallback and reports it in diagnostics. Classic remains the
saved/default migration path while the upgraded look is reviewed.

Select Classic, Ambient depth, Detailed shadows or Full indirect in graphics
settings. `?lighting=7` enables all three for a local review session. Graphics
preferences keep their existing save format and preserve world progress.

## Reproducible visual evidence

`output/rendering-upgrade/baseline/source` preserves the main source/runtime.
Only capture/benchmark instrumentation is added to this control. Capture output
is intentionally local and ignored by Git.

```sh
# Prepare the preserved control from main 76f77ea; this does not build or run it.
python3 scripts/prepare-render-baseline.py
# Execute the printed native/WASM build commands before capturing the control.
# The printed subshell isolates its toolchain and CARGO_TARGET_DIR settings.
output/rendering-upgrade/baseline/target/release/verify output/rendering-upgrade/baseline/captures 1337 render-compare baseline all 0 720
```

The helper uses `git archive` and retains the original renderer, shaders and
locked dependencies. It adds the rendered `cameraPosition`, asynchronous
`begin_gpu_drain`/`gpu_drained` hooks, and the shared native fixture harness with
a no-op lighting callback. The control report omits the candidate-only
`lightingBytes` field. It links the repo's `.tools` directory when available
and prints native and WASM build commands using the separate
`output/rendering-upgrade/baseline/target` directory. The preserved build script
is not needed for these commands.

Rerunning preparation verifies and reuses an existing control. Its
`.render-baseline.json` records the resolved commit and source hashes.
`--dry-run` prepares and checks the transformations in a temporary directory;
`--ref` and `--output` select another compatible commit or source location.
Only an explicit `--force` replaces existing source, retaining a timestamped
sibling backup. Preparation never changes capture directories. Capture commands
write their requested PNGs and manifests, so use a fresh output directory when
keeping earlier runs.

Use the main checkout for candidate builds and comparisons:

```sh
# Build the candidate native verifier with the repo toolchain on PATH.
cargo build --release --bin verify --locked
# All seven scenes, fixed 720p scene size, 1280×720 output, frozen animation.
target/release/verify output/rendering-upgrade/candidate/full 1337 render-compare full all 0 720
# Other stages: ao, shadows, off. Optional measured native frames replace 0.
python3 scripts/compare-render-captures.py output/rendering-upgrade/baseline/captures output/rendering-upgrade/candidate/full --require-effect --out output/rendering-upgrade/candidate/full-differences.json
python3 scripts/serve-render-comparison.py
```

Open `http://127.0.0.1:4174/rendering.html` for the draggable comparison and
original PNG links. Captures use the same seed, camera, FOV, simulation time,
weather history, 400% cover, Balanced world quality, FXAA and Bloom.
The harness drains streaming and renders 240 frozen warmup frames. The seven
views cover forest, capital, day/night inn, meadow, stone arches and rain.
The baseline repeated capture is pixel-identical in all seven views.
The full candidate has matching manifests and visible differences in all seven.
Image differences establish identity/change, not an objective beauty score.

The final capture set in `candidate/final-full` also matches its independent
`candidate/final-repeat` run pixel for pixel in all seven views. These are
frozen-state captures; moving-camera and streaming performance are checked
separately. `candidate/final-repeat-check.json` records the strict A/A check,
and `candidate/final-differences.json` records the original-to-final comparison.
All scene manifests match, with no black-frame or alpha failures.
After the GI upload lifecycle fix, the rebuilt native verifier recaptured all
seven views in `candidate/lifecycle-check`. They remain pixel-identical to
`candidate/final-full`, with matching manifests and zero RGB differences;
`candidate/lifecycle-repeat-check.json` records this strict A/A validation.

| View | Changed pixels vs original | Mean absolute RGB difference (0–255) |
| --- | ---: | ---: |
| Capital | 50.506% | 4.0561 |
| Forest | 82.879% | 3.4116 |
| Inn, day | 95.162% | 9.1644 |
| Inn, night | 74.381% | 3.6618 |
| Meadow | 49.009% | 3.3044 |
| Stone arches | 12.288% | 0.1918 |
| Rain | 48.781% | 3.9678 |

The final GI coverage correction preserves the previous candidate's two inn
views and stone arches exactly. The other four views change at most 0.134% of
pixels, with mean absolute RGB differences below 0.001. This comparison is in
`candidate/bounds-differences.json`; it intentionally allows image changes at
the probe field's coverage border.

The final Classic/original comparison matches all seven scene manifests and
matches the forest, both inns and rain pixels exactly. Its strict pixel check
fails on four pixels across the remaining three views: two in the capital, one
in the meadow and one at the stone arches. The largest RGB channel difference
is 2/255; `candidate/final-off-parity.json` retains that failure. These values
are consistent with arithmetic rounding, though the image comparison alone
does not establish its cause. Exact original-to-Classic parity remains a
documented limitation.

## Performance protocol

The local review page runs fresh main/candidate games sequentially in ABBA
blocks at fixed resolution. Establish baseline A/A noise for each scene,
resolution and workload, then run the corresponding paired comparison.
Warmup advances weather in supported 50ms steps, waits for geometry/probes,
settles for 180 RAF frames and drains the GPU queue before measuring.
Raw frame intervals, CPU submission, scene manifests and asynchronous GPU
samples are saved locally as named JSON reports. At least two A/A blocks and
two paired blocks are required for the repeatability gate. Frozen view, camera
sweep, walking/streaming and cold teleports are available.

Cross-version GPU timestamp spans have different pass coverage. They are
reported as diagnostics; overlapping Apple GPU pass intervals cannot be added.
Native completed-frame timing is measured separately and is not browser FPS.
A frame-rate cap alone does not establish equal GPU headroom. Reports record
both WASM SHA-256 hashes, the fixture manifest hash and explicit adapter fields.
The runtime hash also keys module loading to avoid stale initialized WASM.

## Measured results

Measurements use an Apple M4 with 32GiB RAM and AC power. The browser reports
the Apple `metal-3` adapter and Chrome 154 user agent. All values below use the
forest fixture, fixed 1280×720 rendering and 400% ground cover.

The final native ABBA sequence measures 600 completed frames per run after
240 frozen warmup frames. Pooling the two main runs and two full-upgrade runs:

| Native completed-frame time | Main | All upgrades |
| --- | ---: | ---: |
| Median | 9.825ms | 10.009ms |
| Mean | 10.480ms | 10.716ms |
| 95th percentile | 12.783ms | 13.014ms |
| 99th percentile | 13.916ms | 14.395ms |

The median increase is 0.184ms (1.88%). CPU submission is also higher in the
native run. These are real costs; this result does not establish equal GPU
headroom. Raw values and native binary hashes are retained in
`native-final-abba/summary.json` under the local evidence directory.

The browser static comparison uses two ABBA blocks, 300 measured frames per
run, and the matching two-block baseline A/A control. Main and candidate both
have a 16.7ms median frame interval. Pooled CPU means are 1.626ms and 1.557ms;
frame 95th percentiles are 18.5ms and 18.6ms. The repeatability gate is
**provisional**, with matching scene states and no metric slower than the A/A
floor in both blocks. The gate includes mean frame interval as well as median,
95th/99th percentiles and CPU timing. The second candidate block has worse frame tails:
pooled 99th percentile is 33.4ms versus 18.7ms, and intervals above 33.4ms
number 5 versus 1. This variation is preserved in the report rather than
being hidden by the median. Static frame pacing is not a universal performance
guarantee.

`browser-forest-full-720-static-main-candidate.json` contains that comparison,
and `browser-forest-off-720-static-main-main.json` contains its baseline
control. The full 720p lighting targets and buffers occupy 26.09MiB, compared
with the original 16MiB sun-shadow map, excluding common scene resources.
Classic currently retains the new auxiliary allocations (29.09MiB lighting
resources) while disabling their rendering passes. Memory and startup costs
have not been profiled across devices. Classic stays the default; Full indirect
is available for explicit review.

The walking comparison uses the same five simulated seconds and 27.5m route
for each main/candidate run, with two ABBA blocks and its own two-block A/A
control. Initial scene state and final rendered-camera poses match. Median
frame interval is 16.7ms for both; mean intervals are 16.764ms and 16.667ms,
95th percentiles 17.2ms for both, and 99th percentiles 17.7ms and 17.6ms.
CPU means are 1.370ms and 1.453ms. No metric exceeds the walking control's
noise floor in both blocks, including mean frame interval. This gate is also
**provisional**, limited to the tested route, resolution and hardware.

The walking raw reports are `browser-forest-full-720-walk-main-candidate.json`
and `browser-forest-off-720-walk-main-main.json`. The final mean-aware analyses
are `browser-forest-full-720-static-analysis.json` and
`browser-forest-full-720-walk-analysis.json`. They retain SHA-256 references
to the untouched raw reports, control files, and actual browser gate source.
Reproduce the analysis without launching a renderer:

```sh
node scripts/reanalyze-rendering-report.cjs output/rendering-upgrade/browser-forest-full-720-walk-main-candidate.json output/rendering-upgrade/browser-forest-off-720-walk-main-main.json --out output/rendering-upgrade/walk-reanalysis.json
```

Average frame time participates in the gate so a 60FPS median cannot hide
repeatable lost frames. Older controls derive that metric from their saved
per-block frame means. Native completed frames, browser RAF intervals and
asynchronous GPU diagnostic spans are separate measurements.

## Verification

The 153-test Rust suite passed. Three added regressions passed in targeted runs:
point-light source-voxel exclusion on the GPU, queued GI upload handling on the
CPU, and repeated teleport handling on the GPU. This gives 156 passing Rust
tests across the suite and targeted invocations. GTAO contact, flat-surface
visibility and repeatability, GI colored bounce/occlusion, screen-cache
construction and cascade stability are exercised
on the actual GPU. WASM generation, worker packets, adaptive cadence, settings
migration/persistence and benchmark orchestration checks passed. Both release
runtimes build locally. No deployment or publication is part of this change.
