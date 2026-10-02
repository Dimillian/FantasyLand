# Regional world generation

Local checkpoint before this pass: `checkpoint/pre-regional-worldgen-20261002` (`6e1ebd8`). This changes geography for existing seeds; coordinates still restore, but coastlines and some terrain/settlement locations can change. No saved character progression is reset.

## Geography and ecology

The mainland lobes and bays now vary in size, offset and orientation. The five offshore island envelopes also change position and shape recipe, and their mountain ranges follow those same island descriptors. Deep ocean remains around the world boundary. This is deterministic analytic geography, not simulated plate tectonics.

Mountain passes feed curved U-shaped valleys descending both sides of the range. These join the existing spurs, sheltered basins, cirques, tarns, drainage and ascent routes. Chalk downs and basalt uplands now have distinct regional classifications, alongside the existing woodland, granite, sandstone, lowland, coast and alpine families. Shared geology continues to control terrain, stone and soil colors.

Forest carrying capacity blends across moisture, temperature and broad woodland fields. Elevation fades trees below the snow line; conifer composition responds to temperature and elevation rather than switching at a biome label. Ground pigments blend between temperate, dry, tropical and alpine climates.

## Coasts and flowing water

Shore recipes distinguish beach, dunes, shingle, sea cliffs, saltmarsh and estuary. Sediment supply, rock resistance and upwind shelter select them. Sheltered back-barrier pockets can become shallow saltwater lagoons. Low river mouths broaden into estuaries, with rare deposited bars and two water gaps around them. These are analytic depositional shapes; there is no evolving sediment or tidal simulation.

Actual steep river reaches emit curved falling sheets between their existing lip and downstream surface. Geometry is trimmed against the rendered terrain, and a small scour depression deepens the channel below the drop. Animation moves down the vertical sheet, rather than sliding a flat-water texture up a cliff. Falling sheets share the existing water pass, have at most two descriptors per chunk, and disappear beyond detailed LOD2. They are not a 3D fluid simulation.

## Settlement countryside

An allocation-free settlement bucket lookup limits hinterlands to nearby eligible settlements. Flat, fertile, dry ground receives warped irregular parcels of pasture, grain, orchard or coppice. Camps remain uncultivated. Water, steep slopes, high mountains and settlement centers are excluded. Some parcels and outer margins stay wild.

Terrain colors, planted cover, shrubs along boundaries, bare access strips, smaller orchard trees and the distant terrain/forest layer use the same parcel field. Cover remains streamed through the existing instancing system; no new draw pass or world-sized farm mesh is allocated. Farming gameplay, ownership and crop growth are not implemented by this pass.

## Atlas and inspecting the result

The atlas has Landscape, Elevation and Geology surveys, plus a geography-name toggle. Named ranges, valleys, regional country, lakes and river catchments retain their names when panning or zooming. Geographic names get first choice of label space; settlements and roads remain visible underneath. Town buildings and streets are also retained from the engine feature payload.

Settings → Landscape travel includes Chalk downs, Basalt uplands, Cultivated countryside, Dune coast, Tidal saltmarsh, Shingle shore, Sea cliffs and River gorge waterfall when the selected seed supplies a safe generated arrival. These are generated places, not separately authored showcases.

## Verification

- Full Rust release library suite covers drainage conservation, coastal mouths, five separate sizable islands, dry connected roads and mountain trails, render/collision grounding, settlement navigation and GPU pipeline creation.
- Added checks cover atlas name stability and layer differences, field locality/suitability/repeatability, coastal recipe diversity and actual submerged lagoon pockets.
- Updated water geometry checks distinguish supported falling sheets from whitewater fans and enforce the respective geometry budgets.
- `scripts/verify-ui.cjs` exercises existing mouse/keyboard menus, atlas roads and settings behavior.
- `scripts/verify-wasm.mjs` exercises the compiled browser engine, seed determinism, hydrology, spawn safety and generated travel destinations.
- `verify output/regional-worldgen 1337 regional-studies` captures the real renderer at 1280×720 and exports all three atlas layers. These captures are not a browser FPS benchmark.

Development remains local at http://127.0.0.1:4173/.
