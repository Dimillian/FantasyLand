# Tree architecture and fuller foliage

2026-10-02. Checkpoint: `checkpoint/pre-tree-diversity-20261002`.

The existing ecology families now resolve into twelve named growth recipes:
pine, spruce, fir, cedar, oak, beech, alder, birch, willow, acacia, emergent jungle
tree and palm. Dead snags remain separate. These are procedural growth recipes,
not imported botanical models. Biome selects the appropriate tropical/wetland
forms, and seeded companion variation selects spruce/fir, pine/cedar and oak/beech.

Spruce has a low overlapping skirt, fir has drooping boughs, cedar spreads into
broad flat tiers, and pine retains a higher open crown. Young conifers have four
low tiers plus a leader, rather than three scaled-down adult tiers. Young
broadleaves have four main branches and enlarged overlapping crown lobes. Alder
is shorter with rounded upright crowns; beech is taller. Birch crowns are fuller.
Regeneration patches may also contain additional shrubs on otherwise empty prop
candidates. Large-tree candidate spacing and population density are unchanged.

Five additional procedural atlas layers provide dense spruce/fir needles,
flattened cedar fans, small birch leaves, pendant willow sprays and broad
beech/alder leaves. These remain 128×128, drawn on the existing 64×64 pixel grid.
The two material arrays now total approximately 5.5 MiB including mipmaps,
about 0.83 MiB more than before. Alpha-coverage mipmaps and trimmed needle cards
are retained. Color, reflection, shadow, shelter, ambient-occlusion depth and
leaf-transmission paths recognize all new cutouts. No new render passes.

Near trees remain bounded at 48 foliage cards; middle trees retain the same
card planes as a subset, with at most 33 cards. Distant trees remain opaque
merged crown geometry. Their envelopes come from the actual card recipes;
cedar keeps broad rounded tiers. Individual average mesh and pixel costs can
increase: an unchanged maximum is not a claim of zero performance cost.

## Validation

Tests cover all twelve recipes across temperate, wetland and tropical biomes,
finite geometry, sapling branch tiers, card budgets, trimmed-mask containment,
coverage-preserving mipmaps, wind-fixed roots and stable near/middle cards.
The existing forest capture mode records actual generated forests at 720p:

```sh
cargo run --release --bin verify -- output/tree-diversity/review 1337 forests
```

Before and after images are under `output/tree-diversity`. Native capture timing
includes synchronous GPU completion and is not browser FPS. Captures made while
tests or browser work are active must not be used as controlled timing results.

The full release regression suite passed (163 tests). Final mask refinements
are additionally checked with the material and foliage/LOD tests. UI contracts
and deterministic WASM worker packets are checked against the local build.

Final validation: four material tests and eight tree/grounding asset tests passed
after the last art refinement; all five final WASM packet checks passed. Seven
matched-camera captures in `output/tree-diversity/review` use 1.11–2.76% more
resident mesh bytes than the checkpoint. Ground-cover draw counts are unchanged.
The local browser loaded the final revision without console errors, including
an active nighttime blizzard. Browser FPS has not been benchmarked in a controlled
before/after run for this change.
