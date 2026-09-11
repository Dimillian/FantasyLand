# FantasyLand — The Wilderness

A first-person procedural fantasy exploration slice, built with Rust, WebAssembly, and wgpu. All terrain, trees, rocks, crossings and landmark geometry are generated in code. The browser shell handles input, maps and a retro HUD; it does not render the 3D world.

## Play

- WASD / arrow keys: walk. Mouse: look. Shift: sprint. Space: jump.
- Click the world to focus and capture the mouse. Escape releases it. When a browser blocks capture, click-focused mouse look still works within the window; touch uses drag look.
- M or Tab: open the unified atlas. M or Escape closes it; Tab navigates controls inside menus.
- Drag the atlas to pan, scroll to zoom from the local landscape to the whole continent, select a location to set a waypoint or fast travel. Zoom and position persist between openings.
- I: bag. C: character. K: skills. O: settings (landscape travel, sun shadows, ground cover density, screen filter, seed, quality, sensitivity, daylight, return to spawn). F3: diagnostics.
- Progress and preferences are stored on this device. Add `?seed=42` to explore another deterministic seed.

Bloom is the default screen filter, with soft highlights across three blur scales while the original pixel detail stays sharp. CRT adds stable scanlines, an RGB phosphor mask, slight curvature, subtle color separation and a smaller glow. Clean preserves the unfiltered image. Select the filter and set its strength from 0–150% in Settings; the choice persists with the existing save. Filters affect the 3D view, keeping HUD and atlas text crisp. Bloom extraction and blur run in linear light on small floating-point GPU targets, rebuilt when resolution or quality changes; Clean skips those passes.

ASCII renders the view entirely as printable character cells. A GPU pass encodes one glyph and foreground color per cell from the scene's brightness and edge direction; a second draws original 5×7 bitmap glyphs against a terminal background. No original scene pixels show through. Choose Small, Medium or Large text and scene colors, amber or green phosphor. ASCII uses its own text-size controls rather than the effect-strength slider.

Internal resolution is separately selectable from 120p to 1080p, preserving the viewport aspect ratio, with actual dimensions shown. Auto retains the quality preset's 270/450/720p cap; Native matches the game canvas. Explicit resolutions stay fixed when world quality changes and can supersample a smaller viewport. ASCII text size controls its character grid independently of the underlying scene resolution. These preferences preserve the existing player position and atlas view.

Ground cover density ranges from Off to 400% in Settings, independently of world quality. It controls grass, flowers, ferns, heather and dry seedheads; 400% is the default. Changing it adjusts the number of submitted instances immediately, keeps existing plants in place, and preserves the saved position, atlas and other settings. F3 reports density, submitted cover instances and mesh buffer memory.

Ground cover extends 420 m, thinning distant instance counts while preserving dense nearby plants. Its entire visible range retains the exact 6 m terrain surface for anchored roots. Ground cover streams in 48 m tiles and shares procedural plant templates. A compact placement record replaces duplicated plant vertices. Each tile includes a 6 m terrain-height grid; the vertex shader uses the same alternating triangles as the visible ground to anchor roots on slopes. Whole tiles outside the visible range or camera frustum are rejected before vertex processing. Terrain, water and tree meshes also use full frustum culling. Generation uses small cover jobs and a shared time budget; individual terrain/prop jobs remain non-preemptible.

## World

The world is 384 × 384 km (147,456 km², including ocean), with signed coordinates centered on zero. Terrain streams in 192 m chunks with several levels of detail. Coarse terrain patches extend the real horizon to roughly 20 km at the default setting; distant mountains remain part of the same walkable world. The default seed is 1337.

A connected main continent and five substantial offshore islands sit inside an ocean margin. Warped landmass contours form bays, headlands and straits. Low shores descend through sandy beaches; high coastal relief forms rock cliffs. Shallow turquoise shelves deepen into the open sea, with depth-driven surf and animated water extending into the horizon. Rivers drain to sea-level outlets; offshore cells do not generate rain-fed river channels.

Blended geological provinces roughly 24 km across generate rolling woodland, granite ridges, sandstone ledges, chalk slopes and basalt plateaus. Relief and prevailing wind exposure influence rainfall, shelter and soil. Ancient woodland, granite highlands, windswept coast, wet lowlands, sandstone country, meadowlands and alpine heights recur across the continent, with smoothly varying surface colors and local plant communities. Climate biomes remain a separate input to species selection. Settings → Explore landscape selects real generated locations for each family; no showcase scenery is planted or authored. Settlement sites have irregular positions around approximately 2.4 km spacing. A sparse network links major towns, branches toward selected villages, and leaves many rural sites isolated. Smaller points of interest occupy eligible 640 m cells with jitter, exclusions and occasional empty areas. Future settlement sites currently contain survey structures, banners and camps, not populated towns.

Walking is 5.5 m/s; sprinting reaches 9 m/s and consumes stamina. Settlement spacing targets roughly 5–10 minutes between neighboring sites on foot, excluding stops; mountain detours can take longer. Map estimates show straight-line walking time; terrain and routes can make the actual journey longer.

A shared ecological cover field creates open wildland, sparse woodland, and dense overlapping forest stands within the climate regions. Warped regional fields blend 1.1 km woodland areas, 380 m stands, and 190 m clearings. The same cover drives tree occupancy, terrain and atlas colors, grass height, ferns, and flower patches. Procedural tree families have forked trunks, irregular crowns, bent coastal growth, ancient broadleaf forms and pale wetland groves, mixed with reeds, stones, stumps and fallen logs. Short wooden crossings meet river banks with ramps. Clouds, sunlight, night stars and flowing water are shader-generated, with no imported models or texture assets.

Structures sample their own footprints on the rendered terrain: tents and hearths have level retaining foundations, while tower legs, signs and bridge piers extend into the ground. Vegetation anchors use the matching terrain detail level, distant trees retain trunks, and grass/reed roots stay fixed in the wind. Road surfaces are clipped to terrain triangles to follow slopes and dips without suspended strips. Main roads have a 6.6 m traveled surface, country lanes 4.2 m, and wilderness trails 1.7 m, with softer verges and matching bridge widths. The atlas draws the same routes as thin vector strokes: solid main roads and lanes, dashed trails visible at closer zoom.

Road topology uses a separate terrain-weighted town spanning tree on each landmass with a few useful loops, selected village branches, and occasional hamlet trails. Terrain-cost routing favors gentler grades and dry ground, smooths the chosen corridors, and aligns river crossings across the channel. There are no guaranteed horizontal or vertical road chains. Coastal corridors route around bays and stay on dry land; there are no road links across the ocean. Detailed routes are generated on demand; the continent view uses the same coastal corridors as coarse main-route summaries.

Rivers come from catchments: Priority-Flood conditions a coarse elevation grid, downhill receivers route rainfall, and accumulated runoff determines channel formation and width. Tributaries share junctions with their downstream river. Smoothed channel paths carve the detailed terrain, and their directions drive the animated water. Selected natural depressions retain lakes with level water surfaces and outlets tied to the receiver graph. Broad lakes are obstacles for roads; narrow river crossings remain bridge candidates. Valley profiles and alluvial ground vary with substrate and river flow. The atlas uses the same drainage network. This adapts the drainage-conditioning approach described by [Barnes, Lehman and Mulla](https://rbarnes.org/sci/2014_depressions.pdf); it is a terrain generator, not a fluid simulation.

## Rendering style

All landscape models and surface pigments are procedural. Nearby trees use branched trunks and several asymmetric crown masses; simpler distant shapes share the same root positions and broad envelopes. Rocks use granite tors, layered sandstone/chalk outcrops, basalt and rounded alluvial forms with buried bases and slope-aligned debris. Ground has fern banks, flower colonies, seedheads and exposed stone, with quieter leaf-litter areas under dense canopy.

Surface shaders create small bark fibers, mineral flecks, strata, moss, lichen and soil/litter patterns. Distance and derivatives fade fine pigment patterns to control shimmer. Terrain colors and normals vary smoothly on gentle slopes while rocks and crowns retain low-poly facets.

Beyond the detailed area, simplified trees and rocks continue through the full terrain-chunk range. Separately streamed forest stands follow the same ecological fields out to 5/8.5/11 km on Low/Balanced/High, rooted on the distant terrain surface. Broad hillside pigments, colonies and broken strata stay visible after individual plants become too small to draw. Fine terrain reaches farther around the player so extended ground cover stays anchored.

A 2048 px near sun-shadow map casts tree, rock and terrain shadows onto the ground and cover, with soft comparisons and a gradual distance fade. It follows the player and uses snapped coordinates to reduce crawling. Shadows switch off below the horizon and can be disabled in Settings independently of the Bloom/CRT/ASCII filters. This is direct sunlight shadowing with colored ambient light, not screen-space ambient occlusion.

## Build locally

Install a stable Rust toolchain, the `wasm32-unknown-unknown` target, and `wasm-bindgen-cli` **0.2.128**. Then:

```sh
sh scripts/build.sh
python3 scripts/serve.py
```

The build script also recognizes the isolated toolchain installed under `.tools/` during development. No global shell configuration is changed. Serve over localhost or HTTPS in a browser with WebGPU support. There is currently no WebGL fallback.

## Checks

```sh
cargo test --release --lib
cargo run --release --bin verify
cargo run --release --bin verify output/filters 1337 filters
cargo run --release --bin verify output/ascii-verification 1337 ascii
cargo run --release --bin verify output/grounding 1337 grounding
cargo run --release --bin verify output/roads 1337 roads
cargo run --release --bin verify output/coasts 1337 coasts
cargo run --release --bin verify output/cover 1337 cover
cargo run --release --bin verify output/regions 1337 regions
node scripts/verify-wasm.mjs
node scripts/verify-ui.cjs
```

The native verifier runs the same wgpu shaders on a real GPU, checks the rendered output, and writes terrain/map images under `output/verification`. The WASM check loads the actual compiled module and verifies drainage, ecology and atlas generation independently of the graphics backend. The UI harness checks input and atlas behavior with DOM/pointer-lock mocks, including rejected capture and late responses after Escape; it is not a browser end-to-end test.

The `cover` verifier compares Off, 100%, 200% and 400% from one fixed camera, checks that density changes preserve all resident buffers, then measures four 48 m streaming transitions. It writes `cover-report.json` with CPU submission, serialized GPU-completed frame timings and buffer payloads. These native measurements are not browser FPS. The `regions` verifier captures all seven landscape families at walking height, checks their repetition across the continent, compares sunlight shadows on/off, and records representative native GPU timings and buffer payloads. Landscape complexity affects both numbers; these are not browser FPS.

## Current limits

- This is an exploration prototype: no combat, quests, living NPCs, town interiors or economy yet. Health and mana are visible resources reserved for those systems; stamina is active. Bag and skills panels state which systems are still unavailable.
- All map destinations are available for fast travel while testing.
- Hydrology uses a 500 m drainage grid with refined channel curves and carved valleys. It does not yet simulate long-term erosion, seasonal floods, or sediment transport over time. Lakes are static retained lowland basins, not a fluid simulation. Rare highland spillways can still carve overly deep gorges in the coarse drainage model.
- Road corridors penalize steep slopes but do not guarantee a maximum grade or solve full mountain switchbacks. Some settlements intentionally have no road; fast travel and cross-country walking remain available.
- Trunks and boulders block movement. Landmark structures are currently primarily visual, with limited structural collision.
- Terrain and props generate incrementally on the browser's main thread. Ground cover is instanced and generated in small jobs; worker scheduling remains a future optimization.
- ASCII currently converts the rasterized 3D view into character cells; it is not a separate terminal application or a text representation of world geometry.

## Code

`world.rs` defines world identity, climate, terrain, features and maps. `coast.rs` defines the landmass contours, shore profiles and ocean shelf. `roads.rs` builds the sparse transport graph and terrain-aware routes. `hydrology.rs` builds the drainage graph and channel profiles. `regions.rs` supplies blended geology, landforms, climate exposure and landscape identity. `exploration.rs` finds real regional destinations for the optional tour. `ecology.rs` supplies shared forest and ground-cover fields. `cover.rs` and `cover.wgsl` stream and instance nearby ground cover. `geometry.rs` builds mesh recipes and physical crossing floors. `player.rs` implements movement. `renderer.rs` manages wgpu, chunk streaming and presentation; `horizon.rs` generates distant terrain. `shadow.rs`, `shadow.wgsl` and `lighting.wgsl` handle nearby sunlight shadows. `dist/app.js` supplies browser controls and atlas interactions. Generated browser bindings and WASM are checked into `dist/pkg` for static hosting.
