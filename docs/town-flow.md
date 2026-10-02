# Town streets, inhabitants and building clearance

2026-10-02. Before-change checkpoint: `checkpoint/pre-town-flow-20261002`.

Settlement wards share a seeded deformation of their lanes and plots. Radial
approaches turn as they cross the wards, and verified graph edges gain sampled
bends. Dryness, grade and building clearance still reject invalid routes. NPCs
and the road mesh consume this same graph; shortest paths use distance rather
than the number of graph edges. Central streets, outer lanes and doorstep paths
have distinct widths. This improves the existing ward-based generator; it is
not an entirely new street-growth simulation.

Town and country roads now share a coverage field in the terrain mesh. Only
cells near roads receive extra tessellation, preserving their original triangle
planes. Junctions combine their coverage on one surface, eliminating overlapping
strips and z-fighting. Shoulders blend back into the underlying ground colour and
normal, with broad width variation. There are no new textures or render passes,
but terrain cells near roads contain more triangles (bounded subdivision per
cell, reduced density at distant LODs).

Residents use cached right-side walking routes, narrowed at doorways. Stationary
outdoor activities have individual positions beside the street. Household
members can work in public buildings; outdoor trades use the settlement edge.
Workday wandering has longer, staggered rest periods. Room activity positions
have finite capacity, with overflow visits assigned outdoor yards. Nearby actor
separation uses spatial buckets and cached obstacle bounds, and retains offsets
between updates. Rendering interpolates resolved positions instead of predicting
forward through turns. This is local crowd avoidance, not a full agent navigation
or social simulation. The existing 320 nearby-actor limit remains.

House plots reserve space between roofs. Left windows occupy the front living
bay, away from the hearth and flue; frames, glass, internal wall lining and
analytic window lighting share that placement. Small-home partitions have a
minimum setback from the other window. Roof chimneys and caps remain at every
building LOD, while fine interior furniture can still simplify with distance.

## Verification

- Navigation/plot checks across all six settlement sizes, including dry routes,
  connected doors, plot separation and distinct street widths.
- Actual cached NPC paths checked against walls and solid furniture in a village
  and capital; crowd snapshots at four times of day check body separation and
  static geometry clearance, including 320 nearby actors in the capital.
- Door, dialogue, schedule-boundary and pause checks remain in place.
- Window clearance across all building uses and material variants, plus rotated
  roof/UV and shared collision checks.
- Rural and town road surfaces checked against actual sloping terrain across
  all five detail levels; summed projected triangle area checks no duplicate
  surfaces at junctions.

Validation completed: 162 Rust tests passed, followed by both crowd tests after
the final floor-grounding adjustment. UI checks and all five deterministic WASM
worker packet checks passed. The release WASM build was opened in the local
browser and checked in Oakbury with no console errors.

Reproducible local captures:

```sh
cargo run --release --bin verify -- output/town-flow/after 1337 settlement-data
cargo run --release --bin verify -- output/town-flow/final 1337 town-flow
```

The capture harness reports native CPU simulation/mesh timing separately; that
is not a browser FPS benchmark. Layout changes regenerate existing settlements
for the same seed. World geography and the save format are unchanged.
