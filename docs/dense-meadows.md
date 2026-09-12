# Continuous meadow experiment

The release before this experiment is Git checkpoint `checkpoint/pre-dense-meadows-20260912`, commit `302588cc855117acd56ca0ef119c2245287708d5`, published as Sites version 17. The new grass remains a local experiment.

## What changed

The original system chose at most one plant per 1.5 m square. Flowers, ferns and grass competed for that spot, so raising the density slider could not create a continuous sward. The carpet now has its own ecological cells, independent of those accent plants.

Each suitable cell supports an 8×8 jittered set of short tufts. Two opaque, tapered ribbons form each tuft. At maximum density this is about 28 tufts / 57 blades per square metre in the immediate foreground. The grass is short enough to leave flower heads above it. Woodland has thinner, shorter cover mixed with the existing litter and fern communities. Desert, alpine rock, shores, roads, underwater ground and steep terrain keep their existing sparse accents.

## GPU work

Workers transfer 16-byte ecological cells alongside the existing 48 m cover tiles. Shared boundary samples protect roads and water; the blade vertex shader uses the exact triangulated 6 m terrain surface, including alternating diagonals and chunk seams.

A WebGPU compute pass rejects distant and offscreen candidates and compacts the survivors into indirect draw buffers. Full density tapers in stages: most blades retire between 12–26 m, a quarter survives farther, and a sixteenth supplies the midground until it shrinks into the ground between 38–52 m. Stable seed ranks and geometric shrinking avoid screen-door noise. Density changes do not regenerate the world.

Expanded buffers exist only around the player, with a short reuse period during turns. Each buffer has capacity for all its source candidates, so append counts cannot exceed the indirect draw capacity. There is no CPU readback or per-blade CPU animation.

The opaque carpet draws before the landscape and accent vegetation. It can therefore reject hidden expensive material pixels through depth testing. It receives celestial and cloud shadows, enclosure shading, weather, firelight and godrays without submitting thousands of grass shadow casters. Fine procedural pigment, shared wind and a local player bend give it detail without alpha-card padding or per-blade normal maps.

## Trying it

Use the local preview at http://127.0.0.1:4173/. Settings → Meadow carpet switches the layer on/off immediately; Ground cover density still defaults to 400%. Lighting studies include Windmere wildflowers, dawn and dusk views. Existing world progress is retained.

## Validation

- Seeded meadow placement is repeatable and independent of sparse accent occupancy.
- Actual generated meadow, forest and riverbank cells were checked for dry terrain and road exclusions.
- Worker packet round trips, bounds validation and actual built WASM packets pass.
- UI initialization, comparison toggle persistence, existing settings, navigation and worker lifecycle checks pass.
- Browser captures and timed 420p / 720p comparisons accompany this experiment. Performance results are recorded below.

## Measured browser comparisons

M4 MacBook Air, 32 GB, AC power, low power mode off; one visibly presented game tab at a 1280×720 viewport. Balanced world quality, 400% cover, Bloom, celestial shadows, reflections and enclosure enabled. Build and test processes were finished before sampling. Each result is a 15-second slow camera sweep after streaming drained and a warmup.

| View | Internal resolution | Carpet off | Carpet on |
|---|---|---:|---:|
| Windmere meadow | 420p | 53.4 FPS | 52.1 FPS |
| Windmere meadow | 720p | 44.9 FPS | 41.8 FPS |
| Amberwood woodland | 420p | 46.9 FPS | 45.9 FPS |
| Amberwood woodland | 720p | 36.6 FPS | 33.0 FPS |

The meadow added approximately 12.6 MiB of expanded GPU buffers; woodland added approximately 10.5 MiB. These are adjacent paired runs, not perfectly synchronized simulation replays. The woodland run remains demanding and showed occasional longer frames with the carpet enabled; this does not establish a steady 60 FPS. Absolute results vary from earlier sessions even with the carpet off. Do not sum individual Apple GPU timestamp intervals: they overlap and do not isolate pass costs.

The 30-second starting-road walking test averaged 57.5 FPS at 420p (p95 17.6 ms) and 46.1 FPS at 720p (p95 33.5 ms), traversing about 237 m and two terrain chunks per run. Both ended with no pending jobs.

Actual images and the interactive before/after comparison are in `output/dense-meadows/`; raw benchmark data is preserved in `docs/benchmarks/dense-meadows-2026-09-12.json`.
