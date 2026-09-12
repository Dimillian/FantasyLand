# Anti-aliasing and meadow stability

The pre-change checkpoint is `checkpoint/pre-antialiasing-20260912` (`c8fb458`). This work is local only; the hosted release was not changed.

## Graphics settings

- Anti-aliasing: Off / FXAA / SMAA. FXAA is the default for saves without the preference.
- Bloom, CRT and Clean remain independent. The HTML HUD is never filtered.
- Adaptive resolution is opt-in. It adjusts only the internal scene between 420, 480, 540, 600, 660 and 720 pixels high. Manual resolution, world quality, 400% density and godrays remain separate preferences.
- Compare all AA modes runs all six mode/resolution combinations at a single fixed time of day and restores the selected AA and resolution afterward.
- The local Performance check can also record an eight-second walk with the selected AA mode. Video encoding is deliberately separate from FPS benchmarks.

## Render integration

Off preserves the existing direct presentation path. Enabled AA uses:

HDR world / water / godrays → bloom composition → filmic display color → spatial AA → pixel palette / CRT optics → output. UI text is composited by the browser.

FXAA uses directional edge search and a restrained 0.4 subpixel blend cap. SMAA is the canonical three-pass 1x High algorithm adapted from Bevy v0.17.0: luma edges, diagonal/corner-aware blend weights, neighborhood resolve. The 176 KiB area/search tables and licenses are included in `src/aa/`. There is no temporal history, jitter, MSAA attachment or depth resolve, so water and godray depth contracts are unchanged.

The shared tone-mapped input avoids repeating bloom and filmic conversion at every AA tap. Pipelines are retained across resolution changes. SMAA resources are created on first selection and then reused. Its edge and weight images are cleared on every frame. Approximate added full-resolution targets at 1280×720: 7.0 MiB for the two display-color images; a further 7.0 MiB plus 176 KiB of LUTs after SMAA is initialized. Existing terrain/prop material textures remain 128×128 with coverage-preserving mipmaps.

## Grass stability

Near meadow coverage remains unchanged. The GPU now uses projected blade size to gently widen unresolved midground ribbons, with a strict 1.45× cap. Subpixel-height remnants shrink out continuously using unfaded height to avoid feedback. Wind displacement follows the same combined fade and quiets on small projected blades. Procedural 4×16 pigment cells filter toward their average when unresolved; broad root-to-tip shading stays intact. Terrain anchoring, road/water exclusions, seeded subsets and indirect-buffer capacity are unchanged.

## Adaptive frame pacing

A 1.8-second timing window detects sustained missed frames; brief hitches do not force a downshift. Target allocations have a 2.5-second settling period. Upshifts require roughly 18 seconds of steady cadence and are treated as probes, since refresh-limited RAF does not prove GPU headroom. Failed probes suppress another increase for a minute.

The governor freezes during menus, loading, benchmarks, recording, hidden tabs and another active preview. It resets after resume and large position jumps. Resolution changes reuse the existing scene-target rebuild; they do not clear world chunks or regeneration jobs. At the 420p floor it preserves detail instead of silently reducing density or lighting. It targets, but cannot guarantee, 60 FPS.

## Validation and comparisons

Native GPU tests run the actual WGSL pipelines and confirm constant-color preservation, diagonal smoothing, cleared SMAA edge/weight buffers, resolution changes and mode switches. UI tests cover initialization, preference migration/persistence, manual resolution restoration and hidden-preview freezing. A pure timing test covers 60/120Hz cadence, isolated hitches, sustained overload, the 420p floor and failed-probe cooldown.

Actual browser captures, eight-second walks and a draggable comparison are in `output/antialiasing/`. The comparison is available locally at `http://127.0.0.1:4176/antialiasing/comparison.html` while the retained gallery server is running. All image comparisons use this build; Off versus FXAA/SMAA isolates AA, with the grass stability changes present in every view. Wind and daylight continue between shots.

Measured results are recorded in `docs/benchmarks/antialiasing-2026-09-12.json`. No build/test or video encoding runs concurrently with those samples. Apple GPU per-pass intervals overlap and are not treated as additive pass costs.


## Fixed-light browser results

| Scene | Internal resolution | Off | FXAA | SMAA |
|---|---:|---:|---:|---:|
| Meadow | 420p | 57.1 FPS | 57.1 FPS | 56.9 FPS |
| Meadow | 720p | 45.1 FPS | 44.5 FPS | 44.0 FPS |
| Forest | 420p | 50.7 FPS | 50.8 FPS | 49.9 FPS |
| Forest | 720p | 39.4 FPS | 39.4 FPS | 38.6 FPS |

These are measured frame-cadence averages on the M4 Air, not isolated GPU pass costs. FXAA and SMAA were within roughly 0–1 FPS of Off. Both remain below a steady 60 FPS in the demanding forest. Small apparent improvements are measurement variation, not an AA speedup. Initial separate runs (retained under output) allowed daylight to advance between modes; this final table uses the automatic fixed-light comparisons instead.

Browser QA also observed adaptive resolution moving 540→420p in the woodland with 1194 chunks still loaded and no pending jobs. Browser QA checked Bloom with all three AA modes, and SMAA with CRT and Clean. These share the single-sample depth path; the CRT mask remained after AA.
