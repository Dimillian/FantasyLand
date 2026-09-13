# Continental geography and climate — 2026-09-13

Pre-change checkpoint: `checkpoint/pre-geography-rework-20260913` at `b0ab5e5`.
Local preview: http://127.0.0.1:4173/. No deployment or automatic publication.

## Geography

The 384 × 384 km seeded world retains its main continent, offshore islands and
ocean boundary. Explicit, localized mountain ranges now dominate the large
relief. Generic granite hills are lower; individual summits, saddles, ribs and
cirques break up the ranges. Lowlands remain extensive. This is a procedural
fantasy landscape, not an imported real-country elevation dataset.

The perennial channel threshold increases from 24 to 120 drainage cells.
Rivers still follow the shared priority-flood / D8 drainage network, confluences,
lake spillways and coastline. Low-flow runoff is not rendered as a river.
Channels are narrower, with fewer retained lowland lakes and greater spacing.
Small high-altitude pockets can retain tarns and feed connected outlet streams.
Lake planes constrain both upstream and downstream water levels.

Steep upland reaches can form short bedrock cascades with a shelf below them.
Terrain carving, water level, flow speed and foam use the same monotonic profile.
These are terrain-following cascades, not free-falling waterfall curtains.

## Climate and vegetation

Continuous latitude, elevation, prevailing-wind uplift, rain shadow and regional
moisture fields choose the climate. Biome labels do not switch weather presets.
Moving weather fronts remain continuous, and humid places still have fair breaks.
High, cold fronts can produce snow and blizzards without requiring thunderstorms.

The world now has eleven climate biomes: grassland, temperate forest, pine forest,
moor, alpine, desert/badlands, river wetlands, lowland swamp, savanna, jungle and
tropical coast. They recur where their temperature, moisture and terrain permit.
Swamps favor poorly drained low terrain, muddy gaps, reeds and sparse flowers.
Savanna has open golden grass and umbrella crowns; jungle has taller emergent
trees and fern/shrub cover; tropical shores have palm groves behind the beach.
The existing five temperate forest communities remain available.

Palms have bent trunks and radial fronds, including matching distant silhouettes.
Acacia-like crowns are flatter; jungle branching starts higher. All assets remain
procedural and reuse the existing 17-layer 128 × 128 texture atlas and renderer.
No additional foliage pass or external model/texture assets were introduced.

Climatic snow belongs to ground location rather than camera position. Retained
cold high tarns have frost/crack shading and a walkable ice plane. Collision uses
the same clipped 6 m water triangles as rendering, excluding sloping outlets;
the fluid simulation excludes frozen cells and player wakes there.

## Exploration

Mountain footpaths use bounded terrain routing, contour turns and a maximum
36% grade, with up to three linked ascent legs. A failed search leaves the
mountain unroaded rather than drawing an unsafe straight connection. Routes
are cached and enter the existing local road map/clearing system. The seed 1337
audit found nine routes, including summit routes; the largest ascent gains
about 1,911 m over 16 km. Routes are not guaranteed to reach every summit or tarn.

Settings → World tools → Explore landscape includes biome destinations,
forest communities, a frozen tarn shore and three Mountain ascent trailheads.
The existing Walking journeys menu also contains a checked high-tarn shore walk.
Footpaths appear when the atlas is zoomed to a span of 18 km or less.

## Validation and cost

The 512 × 512 world audits for seeds 1337, 42 and 2026 sampled roughly 1.0–1.3% inland
open water and peaks of 4.7–5.4 km. Only about 1.2–1.3% of sampled land exceeds
2,000 m, so high ranges remain isolated. All eleven biomes occur in each audit.
Seed1337 has 367 headwaters, versus 2,274 at the previous checkpoint, and about
11.1 MB of retained hydrology data. These are sampled measures, not exact area
integrals or a physical climate simulation.

Acceptance checks cover multiple seeds, sparse water, biome diversity, localized
peaks, gradual weather, fair breaks and blizzards; geographic rain shadows across
actual paired mountain slopes; downhill drainage and lake continuity; snowy
summit access, dry trail grades, grounded/clear tour arrivals; and exact ice
render/collision agreement with player movement. Existing UI and WASM worker
packet checks remain applicable.

`verify output/geography-rework 1337 geography` captures actual native wgpu views.
`atlas` writes the climate raster and statistics; `mountains` writes actual route
points and ascent metrics. Native completed-frame timings are not browser FPS;
a clean Safari performance comparison for this world revision is unmeasured.

The design follows drainage-basin and orographic principles described by
[USGS](https://www.usgs.gov/water-science-school/science/watersheds-and-drainage-basins)
and [NOAA](https://prod-01-asg-www-climate.woc.noaa.gov/news-features/featured-images/rain-shadows-summits-hawaii),
using deliberately simplified fields suitable for deterministic runtime generation.

Final checks: 132/132 Rust tests pass. The four geography integration checks and
tour-arrival test also pass after adding trailhead destinations. UI contract
checks and deterministic WASM worker packets pass. The release WASM is served
locally; the latest native images are under `output/geography-rework/`.
