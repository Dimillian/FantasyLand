# Forest communities and foliage — 2026-09-12

Previous working version: `b543c05` (foliage optimization checkpoint).

## Direction from the supplied references

The references use several scales together: tall clear trunks, a broken middle
canopy, young trees, waist-high shrubs, arching ferns, and quiet patches of moss
or litter. Conifer branches form shelves; broadleaf masses have connected twigs
and deliberate light openings. These observations informed the procedural
recipes. No reference textures, models, or external assets were imported.

## World generation

- Five recurring forest communities: oak/beech, silver birch, tall pine/heath,
  cathedral fir, and willow/alder wetwood. Climate, wetness, geology and a warped
  780m field choose a continuous mixture, with stronger dominance in interiors.
- Each community retains companion species. Existing climate biomes, tree
  candidate positions, roads, coastline and world scale remain in place.
- Regeneration varies over 43m patches. Saplings are 30–46% of adult height;
  mature conifers have clear boles and taller stands. Wetwood broadleaves use
  slimmer upright forms. Ancient trees are occasional canopy accents.
- Dense interiors can now contain shrub thickets and fern colonies. Shrub
  eligibility no longer disappears when the canopy closes. Small woodland
  flower patches persist, while the opaque grass carpet yields to undergrowth.
- The HUD names the local forest. The existing lazy landscape tour includes all
  five communities, with dry arrivals and headings chosen between nearby trunks.

## Rendering

- Asymmetric connected broadleaf clusters and irregular conifer boughs replace
  rosettes and miniature tree patterns in the procedural atlas.
- Fir/pine crowns use radial inclined shelves, with an upright top leader.
  Near/middle LOD retain identical cards; far bounds are derived from the same
  card recipe without allocating temporary meshes.
- Three fern forms and taller multi-lobed shrubs reuse the existing plant
  templates. Large prop shrubs now have cutout foliage rather than solid lumps.
- Ground anchoring preserves tree crown normals. Previously it silently replaced
  them with flat triangle normals. Shrub roots now remain fixed in wind.
- Analytic humid air adds gentle depth between distant trunks. It uses the
  existing surface pass; shadowed cutout godrays remain enabled.

## Cost and validation

The atlas remains 17 shared 128×128 layers, procedurally drawn at 64×64, with
coverage-preserving mipmaps. No larger textures or extra foliage render passes.
Tree candidate spacing, streaming distances and ground-cover instance layouts
are unchanged. Tested maximum near foliage remains 48 cards; ferns use 20
triangles and shrubs 18, within the 24-triangle template budget. A mature fir
can now use more cards than before, so an unchanged maximum is not a claim that
every individual tree became cheaper. Interior accent counts increase.

The cover generator samples only the thicket field it needs, and tree placement
reuses its existing region sample. All plant variants fit current culling bounds.

Checks cover deterministic community mixtures and smooth boundaries, water and
shore exclusions, grounded/wind-fixed roots, preserved crown normals, near/mid
card consistency, far envelopes, texture coverage, template size, dry tour
arrivals, worker packet determinism and browser UI/persistence contracts.
An old culling test expected a 390m tile to survive the existing 300m range;
it now tests a box straddling the actual distance boundary. Runtime culling was
not changed.

`verify output/forest-communities/release 1337 forests` captures the real engine
at five automatically found forests plus the original Orin view. Gallery views
use Balanced/720p/FXAA/Bloom100%/cover400%; Orin uses High with the previous fixed
camera and noon lighting. Native timings are not Safari FPS measurements.

Local WASM SHA-256:
`3c7477f882a77d33decfd3e2f0a57696b71ad91ea2d999ed7d690448690b506b`.

No deployment or push. Final Safari visual validation passed on the local build:
the HUD identified Silver Birch Grove near Dawnreach, and the storm scene
rendered the new tree/undergrowth recipes. The actual Safari capture is
`output/forest-communities/safari-final.png`. A clean Safari before/after
performance comparison is still unmeasured.
