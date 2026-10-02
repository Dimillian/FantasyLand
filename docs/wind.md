# Weather-driven vegetation wind

Added 2026-10-02. Checkpoint: `checkpoint/pre-wind-20261002`.

`wind.rs` smooths the existing weather velocity and gust energy over 1.8 seconds,
then integrates three spatial wave phases and a flutter phase. Fixed spatial
bases avoid phase jumps when wind turns far from the world origin. Phases wrap
at their actual periods, including the slower tree-flex harmonic. Dialogue
pauses wind with the simulation; pausing weather fronts holds the weather
conditions while foliage continues moving. No new graphics setting is required.

`wind.wgsl` evaluates this shared field in the scene, AO depth, reflections and
celestial shadow passes. Broad gusts travel through neighboring plants; tree
stems flex slowly and leaves flutter faster. Birch and willow are more flexible
than conifers and broadleaf trees; dead trees barely bend. Height-dependent
weights keep trunk roots fixed and move branches with the crown through all
three tree detail tiers. Shrubs retain their existing anchored root weights.
Ground cover and flowers bend from their stems. Dense meadow blades scale
movement with their actual height and retain the existing player-parting effect.
Grass normals follow the bend for changing highlights.

Forest-floor exposure is baked from the continuous ecology tree-density field,
so clearings receive more wind than dense understory. This is an art-directed
wind model, not fluid dynamics or an exact aerodynamic simulation of individual
obstacles. Foliage collision and the simplified indirect-light proxy remain
static; the real-time depth and sun shadows use animated geometry. Existing
reflection and light-shaft caching still applies.

## Cost and verification

There are no new render passes, draw calls, per-plant CPU ticks, or wind textures.
The field takes 32 uniform bytes. The CPU vertex holds one extra float during
generation, but the uploaded vertex remains 40 bytes: tree response uses the
previously unused packed normal byte. Cover exposure uses the unused high byte
of existing RGB instance data. Generation/streaming packet shapes stay unchanged.
Bounds include a three-metre motion margin for storm-bent crowns.

The GPU does additional vertex arithmetic, particularly on previously rigid
stems. Unchanged geometry and buffers do not imply zero GPU cost. This pass does
not establish a new browser FPS guarantee or replace the lighting benchmarks.

Wind tests cover deterministic replay, pause, bounded energy, weather turns,
GPU root anchoring, shelter attenuation, storm strength and temporal change.
Tree geometry tests check fixed roots and flexible stems/crowns in every LOD.

Fixed-camera native motion captures (real wgpu, 720p, FXAA, full lighting,
400% cover, eight seconds at 12 captured frames/second):

```sh
cargo run --release --bin verify -- output/wind/forest 1337 wind-motion forest 2
cargo run --release --bin verify -- output/wind/meadow 1337 wind-motion meadow 4
```

Each records PNG frames and a manifest. Capture readback is separate from the
reported native rendering span; neither is browser FPS. To try it interactively,
open the local game, use O → World tools & experiments → Lighting studies to
visit Amberwood or Windmere, then compare Clear, Cloudy and Storm weather.
Weather changes remain gradual; Automatic restores the world's weather system.

Validation: all 160 existing library tests passed, followed by the added real-GPU
wind deformation regression (161 total). UI preference checks and five WASM
worker packet fixtures passed. Full indirect lighting is now the engine and
recommended browser default; saved explicit modes remain available.
