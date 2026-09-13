# Map and outcrop cleanup — 2026-09-13

Previous checkpoint: `31b76f3`, tagged `checkpoint/pre-outcrop-cleanup-20260913`.

The pale parallel stripes were generated geological scarps. All geological
families used the same globally stretched (3:1) noise field, while the exposure
mask never fully removed it. Point-sampled soil color and hillshade amplified
those features on the atlas.

Outcrops now use an isotropic 520 m field, warped independently in X and Z by
two kilometre-scale fields. A smooth exposure mask creates genuinely quiet
ground. The complete centered relief profiles and their metadata use the same
mask. Formation orientation follows the actual warped fold contour. Analytic
derivatives include both warp cross terms and mask product-rule terms.

The atlas blends toward biome/snow colors and macro relief between 3 and 32
metres per pixel. Macro relief is captured before walk-scale formations and
microgrooves; coastal shaping is reapplied. Water coverage and road/river strokes
remain exact and crisp. Inland water is shaded at its surface rather than bed
height. This changes map presentation without removing actual major terrain.

The local relief change can move tributaries, lake footprints and route choices
for a seed. Main continental shapes, explicit range descriptors and continuous
climate formulas are unchanged. There are two additional procedural noise
samples per regional terrain evaluation; no additional GPU passes or textures.

Verification compares the same area at centre (-14974.938, 82502.55), at 16 km,
42 km and whole-world zoom, using actual `World::map_background_rgba` output.
Captures are in `output/map-geology/before` and `output/map-geology/after`.
The native diagnostic can be repeated with the `map-relief` verification mode.

New regression checks cover quiet terrain, retained visible relief, bounded
amplitudes, absence of one dominant continental direction, and agreement between
analytical slopes and finite-difference geometry. Existing watershed, coast,
ice, mountain access, grounding and streaming checks cover downstream effects.

Validation: the broader run passed 132 checks and found two obsolete fixed-location
fixtures. Those now discover actual lake outlets and open grassland, retaining
the original water/root clearance and dense-coverage requirements; both targeted
reruns pass, covering all 134 checks. UI contracts and deterministic WASM worker
packets pass. Release WASM rebuilt and the same-area maps inspected at all three
zoom levels. No deployment or browser FPS claim.
