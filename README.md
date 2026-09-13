# FantasyLand — The Wilderness

A first-person procedural fantasy exploration slice, built with Rust, WebAssembly, and wgpu. All terrain, trees, rocks, crossings and landmark geometry are generated in code. The browser shell handles input, maps and a retro HUD; it does not render the 3D world.

## Settlements and citizens

Settings → World tools → Settlements visits generated examples of all six sizes. Aim and press **E** to talk to a resident or animate a door. Conversations pause the world; **Escape** resumes it. Homes, jobs, daily routes, names, backstories and verbal directions share the same generated records. Interiors have framed leaded glass, aperture-based light shafts, fireplaces, torches and furnished bedrooms. Buildings vary between brick, timber, stone and plaster. The compact DOS dialogue supports mouse, arrows, numbered topics and a scrollable history. Commerce and guild mechanics remain future work. [Implementation, scope and validation](docs/settlements-and-citizens.md).

## Procedural pixel material style

The renderer now generates a shared 20-layer, 128 × 128 texture library at startup: soil, turf/litter, stone, bark, planks, leaves, needles, grass, ferns, flowers, sand, canvas, metal, snow, water detail, moss, flame, masonry, plaster and roof shingles. Albedo, alpha, normal XY, roughness and emission are generated in Rust. Geometry remains procedural. There are no downloaded models or authored texture files.

Near trees use layered, wind-animated cutout cards, with rounded oak crowns, upright broadleaf variants, tall birches, conifers and hanging willows. The same masks cut directional and shelter shadows. Cutout samplers clamp at plant edges; tiling surface samplers repeat. Wind phases stay fixed in world space while weather changes the displacement direction. Ground communities use instanced textured cards. Plant mipmaps preserve alpha coverage; nearest magnification retains deliberate pixels, while filtered mip minification stabilizes the distance. Opaque surfaces use world-projected material coordinates and per-surface roughness/normal response. Wetness darkens exposed surfaces and changes specular reflections; foliage has thin-leaf transmission.

A shadow-raymarched atmosphere integrates 12/16/20 samples at half width and half height, then reconstructs with depth-aware weights before HDR bloom. Thin pixels without reliable low-resolution depth support receive a matching full-resolution integration, avoiding dark holes around moving leaves. It uses real foliage shadows and stops at visible water depth. Sunlight and moonlight, cloud cover and humidity affect its intensity. This is bounded near-field scattering, not unlimited volumetric global illumination. Generated campfires use their own emissive texture and eight bounded local light slots. Nearby building hearths and torches use analytic room/window occlusion; furniture does not yet cast local fire shadows. The sun and primary moon cast shadows.

GPU upload packing retains full-precision positions/water data, compacts normals/UVs and merges identical vertices. Terrain and prop streaming run in separate phases; cover respects the remaining frame budget. Visible world meshes draw from front to back; the sky draws after opaque geometry to avoid shading covered pixels. Directional shadow texels use a nearby persistent anchor with phase-preserving rebasing, preventing large-world coordinate jitter as the sun moves. Cached ambient enclosure uses a stable canopy envelope. The texture library occupies about 3.83 MiB with all mipmaps, plus a 2 MiB procedural human sprite atlas. Existing instancing, frustum culling, terrain/vegetation LOD, cached water reflections and the local fixed-step water solver remain active.

Settings includes explicit 420p, 450p, 540p and 720p targets, five lighting studies in seed 1337, and an actual-browser frame-pacing comparison. The view comparison warms streaming, then samples 15 seconds of camera movement at each resolution. The walk comparison includes 30 seconds of normal player movement and streaming per resolution. Results show actual pixel size, mean FPS, p95/p99 frame interval, CPU submission time and frames over 33 ms. Tests stop if the tab becomes hidden. Hidden tabs submit no engine frames. Native capture timings must not be presented as browser FPS. F4 hides/restores the HUD for photos.

## Play

- WASD / arrow keys: walk. Mouse: look. Shift: sprint. Space: jump.
- Click the world to focus and capture the mouse. Escape releases it. When a browser blocks capture, click-focused mouse look still works within the window; touch uses drag look.
- M or Tab: open the unified atlas. M or Escape closes it; Tab navigates controls inside menus.
- Drag the atlas to pan, scroll to zoom from the local landscape to the whole continent, select a location to set a waypoint or fast travel. Zoom and position persist between openings.
- I: bag. C: character. K: skills. O: settings (resolution and mouse sensitivity; advanced graphics and world tools are collapsed). F3: diagnostics.
- Progress and preferences are stored on this device. Add `?seed=42` to explore another deterministic seed.
- Settings → World tools & experiments → Natural wonders visits ten families of generated rock formations. Walking journeys offers three continuous, roughly 6–7 minute routes in the default world, with turns marked in the atlas. These are walking corridors through wilderness; small trees may need a short detour. Discovery is computed and cached the first time each selector is opened.
- Dawn, Day, Dusk and Moons buttons change the lighting immediately; the time slider explores the complete cycle.
- Weather starts in Automatic. Settings offers Clear, Cloudy, Overcast, Rain, Storm, Tempest, Snow and Blizzard overrides, a 0.25–20× weather clock and pause. Overrides blend in gradually. Higher clock speeds make snow buildup and changing fronts easier to inspect; particle and river motion retain real animation speed.

Bloom is the default screen filter, with soft highlights across three blur scales while the original pixel detail stays sharp. CRT adds stable scanlines, an RGB phosphor mask, slight curvature, subtle color separation and a smaller glow. Clean preserves the tone-mapped image without a screen effect. Select the filter and set its strength from 0–150% in Settings → Advanced graphics; the choice persists with the existing save. Filters affect the 3D view, keeping HUD and atlas text crisp. The complete scene renders into linear HDR. Bloom extraction and blur use small floating-point targets; a shared filmic tone map and subtle final palette reduction preserve highlight energy until presentation. Clean skips the Bloom passes.

Resolution offers Native (default), 720p, 540p, 420p and Adaptive, with actual dimensions shown. Native matches the game canvas in CSS pixels, without multiplying by Retina device-pixel ratio. Adaptive adjusts between 420–720p; choosing a fixed resolution turns it off. Existing custom resolutions remain supported and visible when selected. Explicit resolutions stay fixed when world quality changes. Saved player position and atlas view are preserved.

Ground cover density ranges from Off to 400% in Settings → Advanced graphics, independently of world quality. It controls bent grass tufts, flowers, serrated ferns, heather, dry seedheads, reeds, low shrubs and leaf/branch litter; 400% is the default. Eight community families each have eight variants sharing a bounded 24-triangle template. Changing it adjusts the number of submitted instances immediately, keeps existing plants in place, and preserves the saved position, atlas and other settings. F3 reports density, submitted cover instances and mesh buffer memory.

Ground cover extends 420 m, thinning distant instance counts while preserving dense nearby plants. Its entire visible range retains the exact 6 m terrain surface for anchored roots. Ground cover streams in 48 m tiles and shares procedural plant templates. A compact placement record replaces duplicated plant vertices. Each tile includes a 6 m terrain-height grid; the vertex shader uses the same alternating triangles as the visible ground to anchor roots on slopes. Whole tiles outside the visible range or camera frustum are rejected before vertex processing. Terrain, water and tree meshes also use full frustum culling. Generation uses small cover jobs and a shared time budget; individual terrain/prop jobs remain non-preemptible.

## World

The world is 384 × 384 km (147,456 km², including ocean), with signed coordinates centered on zero. Terrain streams in 192 m chunks with several levels of detail. Coarse terrain patches extend the real horizon to roughly 20 km at the default setting; distant mountains remain part of the same walkable world. The default seed is 1337.

A connected main continent and five substantial offshore islands sit inside an ocean margin. Warped landmass contours form bays, headlands and straits. Low shores descend through sandy beaches; high coastal relief forms rock cliffs. Shallow turquoise shelves deepen into the open sea, with depth-driven surf and animated water extending into the horizon. Rivers drain to sea-level outlets; offshore cells do not generate rain-fed river channels.

Three connected mainland mountain ranges and five island ranges provide the large-scale skeleton. Branching spurs, lowered passes, sheltered basins and cirques shape the terrain beneath local geological variations. Windward slopes and rain shadows influence regional moisture. Walk-scale hollows and grooves add relief between the large forms. A separate suitability field describes dry buildable ground, fresh water, shelter, fertility, crossings and potential harbors for future settlement placement; the settlement catalogue now uses dry ground, slope and coastal constraints to place populated communities.

Blended geological provinces roughly 24 km across generate rolling woodland, granite ridges, sandstone ledges, chalk slopes and basalt plateaus. Relief and prevailing wind exposure influence rainfall, shelter and soil. Ancient woodland, granite highlands, windswept coast, wet lowlands, sandstone country, meadowlands and alpine heights recur across the continent, with smoothly varying surface colors and local plant communities. Climate biomes remain a separate input to species selection. Settings → Explore landscape selects real generated locations for each family; no showcase scenery is planted or authored. Settlement sites have irregular positions around approximately 2.4 km spacing. A sparse network links major towns, branches toward selected villages, and leaves many rural sites isolated. Smaller points of interest occupy eligible 640 m cells with jitter, exclusions and occasional empty areas. Settlement sites now generate camps, hamlets, forts, villages, towns and rare regional capitals, with households, local streets, enterable buildings and sparse road travellers.

Walking is 5.5 m/s; sprinting reaches 9 m/s and consumes stamina. Settlement spacing targets roughly 5–10 minutes between neighboring sites on foot, excluding stops; mountain detours can take longer. Map estimates show straight-line walking time; terrain and routes can make the actual journey longer.

A shared ecological cover field creates open wildland, sparse woodland, and dense overlapping forest stands within the climate regions. Warped regional fields blend 1.1 km woodland areas, 380 m stands, and 190 m clearings. The same cover drives tree occupancy, terrain and atlas colors, grass height, ferns, and flower patches. Procedural tree families have forked trunks, irregular crowns, bent coastal growth, ancient broadleaf forms and pale wetland groves, mixed with reeds, stones, stumps and fallen logs. Short wooden crossings meet river banks with ramps. Clouds, sunlight, night stars and flowing water are shader-generated, with no imported models or texture assets.

Structures sample their own footprints on the rendered terrain: tents and hearths have level retaining foundations, while tower legs, signs and bridge piers extend into the ground. Vegetation anchors use the matching terrain detail level, distant trees retain trunks, and grass/reed roots stay fixed in the wind. Road surfaces are clipped to terrain triangles to follow slopes and dips without suspended strips. Main roads have a 6.6 m traveled surface, country lanes 4.2 m, and wilderness trails 1.7 m, with softer verges and matching bridge widths. The atlas draws the same routes as thin vector strokes: solid main roads and lanes, dashed trails visible at closer zoom.

Road topology uses a separate terrain-weighted town spanning tree on each landmass with a few useful loops, selected village branches, and occasional hamlet trails. Terrain-cost routing favors gentler grades and dry ground, smooths the chosen corridors, and aligns river crossings across the channel. There are no guaranteed horizontal or vertical road chains. Coastal corridors route around bays and stay on dry land; there are no road links across the ocean. Detailed routes are generated on demand; the continent view uses the same coastal corridors as coarse main-route summaries.

Rivers come from catchments: Priority-Flood conditions a coarse elevation grid, downhill receivers route rainfall, and accumulated runoff determines channel formation and width. Tributaries share junctions with their downstream river. Smoothed channel paths carve the detailed terrain, and their directions drive the animated water. Selected natural depressions retain lakes with level water surfaces and outlets tied to the receiver graph. Broad lakes are obstacles for roads; narrow river crossings remain bridge candidates. Valley profiles and alluvial ground vary with substrate and river flow. The atlas uses the same drainage network. This adapts the drainage-conditioning approach described by [Barnes, Lehman and Mulla](https://rbarnes.org/sci/2014_depressions.pdf); it is a terrain generator, not a fluid simulation.

Channel incision now follows contributing river flow rather than tiny disconnected upstream pockets. Floodplains, terraces and wider deep-valley transitions reduce abrupt trenches. Retained lakes include upland basins; the default world has 65 lakes, including one at about 962 m elevation. A bounded two-dimensional contour search repairs selected steep road approaches and generates dry walking journeys between geographic features. It can reverse direction for switchbacks; difficult road approaches still use the previous valid corridor when the bounded search fails.

## Rendering style

All landscape models and surface pigments are procedural. Nearby trees use branched trunks and several asymmetric crown masses; simpler distant shapes share the same root positions and broad envelopes. Rocks use granite tors, layered sandstone/chalk outcrops, basalt and rounded alluvial forms with buried bases and slope-aligned debris. Ground has fern banks, flower colonies, seedheads and exposed stone, with quieter leaf-litter areas under dense canopy.

Surface shaders create small bark fibers, mineral flecks, strata, moss, lichen and soil/litter patterns. Distance and derivatives fade fine pigment patterns to control shimmer. Terrain colors and normals vary smoothly on gentle slopes while rocks and crowns retain low-poly facets.

Rare larger rock formations follow the local geological axis, with ledges, fractured tors and nearby scree. Their footprints clear roads and water; per-piece collision allows walking between separated stones. Broader oak forks support several flattened crown layers, birches grow in connected multi-stem clusters, and exposed pines form crooked branch shelves. These silhouettes retain matching simplified distant forms.

Ten additional natural-monument recipes generate wind-carved arches, stone windows, pinnacles, granite crowns, stone amphitheatres, fractured escarpments, basalt organs, split monoliths, coastal stacks and hoodoos. Seeded topology, dimensions, asymmetry, strata, orientation and local terrain create repeated families with different forms. Common, rare and monumental candidate tiers have approximately 86/12/2% weights before terrain exclusions; acceptance changes the final proportions. Arches can have multiple connected spans and collapsed secondary roofs, spires vary in number and profile, and basalt organs form irregular clusters. Attached strike-aligned fins, shelves and scree extend the formations into surrounding ground. Structural radius is bounded at 112 m; measured candidate maxima were 2,252 near and 820 far triangles. Attached talus and irregular broken tops ground their silhouettes. Near and far meshes preserve every structural opening, and the same individual solids drive collision: the empty space under an arch remains walkable. Ground cover avoids the actual rock masses. These are landscape features, not the later quest/POI system.

Atmosphere follows local ecology and substrate: woodland shade is cooler, sandstone sunlight warmer, and damp low ground carries restrained height-integrated mist. The reference height follows visible valley/water surfaces, not camera height or submerged lake beds. Regional light blends gradually while walking and resets on fast travel. Local weather wind and gusts move crowns and plant tips, with identical shadow motion and fixed roots. Signed water-depth data gives freshwater and ocean surfaces distinct shallow sediment/deep-water colors, including distant lakes.

Beyond the detailed area, simplified trees and rocks continue through the full terrain-chunk range. Separately streamed forest stands follow the same ecological fields out to 5/8.5/11 km on Low/Balanced/High, rooted on the distant terrain surface. Broad hillside pigments, colonies and broken strata stay visible after individual plants become too small to draw. Fine terrain reaches farther around the player so extended ground cover stays anchored.

A 2048 px near shadow map casts tree, rock and terrain shadows onto the ground and cover, with soft comparisons and a gradual distance fade. It follows the player and uses snapped coordinates to reduce crawling. The primary light transitions from Solenne during the day to Aster at night, fading shadows during the handoff. Shadows can be disabled in Settings independently of the Bloom/CRT filters. This is directional shadowing with colored ambient light, not screen-space ambient occlusion.

The fantasy sky shares its celestial directions with illumination and shadows. Solenne supplies warm daylight and amber twilight; pale Aster provides cool directional moonlight and copper Vey follows a different nightly arc. Procedural crater patterns and sun-driven phases shade both moons, which occlude stars before cloud layers pass over them. The Ashen River is a star-and-dust band, and the Keeper's Crown is a seven-star constellation. Two cloud layers receive sun and moon illumination. A restrained blue ambient floor and reduced night pigment saturation keep paths readable. This is an invented repeating 24-hour sky, without orbital seasons or a lunar calendar; there is one active shadow-casting light at a time.

A separate cached 512 px overhead depth map measures nearby enclosure from actual trees, terrain and rock. Ambient light is reduced beneath canopy and overhangs, while the same physical shelter prevents precipitation and surface deposition below them. The Enclosure setting only changes ambient shading. This is approximate overhead sky visibility, not full global illumination or screen-space ambient occlusion. The map refreshes after movement or about four times per second.

Lakes and the sea reflect surrounding terrain and trees through a mirrored camera, at half the scene dimensions capped at 640 px. A separate opaque color/depth snapshot now provides actual shallow-water refraction and depth-dependent absorption, with foreground rejection at shores. The near surface combines small waves, simulated gradients, rain rings and roughness-aware sun/moon glints; ocean swells taper at the clipped coast. River flow uses generated downstream velocities with bounded dual-phase advection. Fixed wave directions/frequencies and integrated wind offsets prevent changing weather from multiplying total animation time into sudden motion.

A 96 × 96 GPU grid with 1 m cells exchanges surface displacement and horizontal discharge with neighbouring cells at a fixed 60 Hz. Generated bathymetry supplies wet/dry walls; rain, wind and player movement disturb the surface, and foam follows turbulent motion. Its 96 m local patch follows the player in 16 m steps while retaining overlapping cells. A maximum of six steps per frame bounds catch-up. Outside this patch, analytic waves remain the cheaper representation. This is a depth-limited small-wave surface solver around the generated water levels; it does not simulate continent-wide floods, overturning waves or change permanent river/lake volumes.

## Dynamic weather

Seeded traveling pressure fronts and smaller convection fields evolve continuously across space and time. Regional climate, terrain altitude, moisture and time of day affect temperature and the rain/snow phase. Manual overrides smoothly approach their selected condition; Automatic resumes local fronts. Clear spells, broken cloud, overcast, rain, storms, tempests, snow and blizzards share the same simulation. A compact conditions line and Settings show temperature, wind, precipitation, wetness and snow cover.

The visible cloud field also projects cloud shadows onto land and water. Precipitation changes height-integrated visibility; gravity-led rain streaks and snowflakes use constant fall speeds with integrated wind drift; they are depth-tested and sheltered by actual nearby geometry. Storms drive waves, gusts, cloud illumination and brief seeded lightning events, including world-anchored forked bolts. Snow accumulates on exposed upward surfaces; rain wets them, warm conditions melt snow and fair weather dries the ground. Surface history uses a bounded 192-cell cache on a 1.6 km grid, interpolated during travel. Revisited cells replay at most 15 minutes of weather; this is local visual history, not persistent continent-wide climate simulation. Weather pause freezes fronts and accumulation, while manual transitions can finish and ordinary visual animation continues. Sound and wildlife remain for the next stage.

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
cargo run --release --bin verify output/fluid 1337 fluid
cargo run --release --bin verify output/grounding 1337 grounding
cargo run --release --bin verify output/roads 1337 roads
cargo run --release --bin verify output/coasts 1337 coasts
cargo run --release --bin verify output/cover 1337 cover
cargo run --release --bin verify output/regions 1337 regions
cargo run --release --bin verify output/portraits 1337 portraits
cargo run --release --bin verify output/epic 1337 epic
cargo run --release --bin verify output/journeys 1337 journeys
cargo run --release --bin verify output/weather 1337 weather
cargo run --release --bin verify output/climate 1337 climate
cargo run --release --bin verify output/lightning 1337 lightning
node scripts/verify-wasm.mjs
node scripts/verify-ui.cjs
```

The native verifier runs the same wgpu shaders on a real GPU, checks the rendered output, and writes terrain/map images under `output/verification`. The WASM check loads the actual compiled module and verifies drainage, ecology and atlas generation independently of the graphics backend. The UI harness checks input and atlas behavior with DOM/pointer-lock mocks, including rejected capture and late responses after Escape; it is not a browser end-to-end test.

The `cover` verifier compares Off, 100%, 200% and 400% from one fixed camera, checks that density changes preserve all resident buffers, then measures four 48 m streaming transitions. It writes `cover-report.json` with CPU submission, serialized GPU-completed frame timings and buffer payloads. These native measurements are not browser FPS. The `regions` verifier captures all seven landscape families at walking height, checks their repetition across the continent, compares sunlight shadows on/off, and records representative native GPU timings and buffer payloads. Landscape complexity affects both numbers; these are not browser FPS.

## Current limits

- This is an exploration prototype: no combat, quests or economy yet. Generated NPC routines and furnished single-storey interiors are playable. Health and mana are visible resources reserved for those systems; stamina is active. Bag and skills panels state which systems are still unavailable.
- All map destinations are available for fast travel while testing.
- Hydrology uses a 500 m drainage grid with refined channel curves and carved valleys. It does not yet simulate long-term erosion, seasonal floods, or sediment transport over time. Lakes are static retained basins. Rare highland spillways can still carve overly deep gorges in the coarse drainage model.
- Road approach repair is bounded and does not guarantee a maximum grade everywhere. Some settlements intentionally have no road; fast travel and cross-country walking remain available.
- Trunks, boulders and natural-monument solids block movement. Natural arches have usable openings, but their elevated roofs are not climbable landing surfaces. Earlier survey structures still have limited structural collision.
- Weather does not yet alter lake levels, freeze lakes into walkable ice, drive seasonal floods or persist its clock across reloads. Cloud layers and overhead enclosure are bounded rendering approximations; water uses one planar landscape reflection at a time; refraction is limited to the opaque scene visible to the camera.
- Terrain and props generate incrementally on the browser's main thread. Ground cover is instanced and generated in small jobs; worker scheduling remains a future optimization.

## Code

`world.rs` defines world identity, climate, terrain, features and maps. `coast.rs` defines landmass contours, shore profiles and the ocean shelf. `geography.rs` generates connected mountain ranges and basins; `regions.rs` supplies geology and landscape identity. `hydrology.rs` builds drainage and channel profiles. `roads.rs` builds the sparse transport graph; `traversal.rs` supplies bounded contour routing and `journeys.rs` selects geographic walks. `habitat.rs` describes future habitation suitability. `natural.rs` owns natural-monument geometry, collision and viewpoints; `exploration.rs` finds regional destinations. `ecology.rs` supplies shared forest and cover fields. `plants.rs`, `cover.rs` and `cover.wgsl` generate and instance ground cover. `geometry.rs` builds other mesh recipes and physical crossing floors. `player.rs` implements movement. `renderer.rs` manages wgpu and streaming; `horizon.rs` generates distant terrain. `celestial.rs` supplies the shared sky and light state; `weather.rs` provides moving fronts and local surface history, `environment.wgsl` handles shelter, reflections and precipitation, and `tonemap.wgsl` supplies final HDR presentation; `world.wgsl`, `shadow.rs`, `shadow.wgsl` and `lighting.wgsl` draw the sky, surfaces and shadows. `dist/app.js` supplies browser controls and atlas interactions. Generated browser bindings and WASM are checked into `dist/pkg` for static hosting.

### Smooth streaming and rendering

The browser uses one module worker (`dist/world-worker.js`) with its own seeded
Rust world. Terrain, props, horizon patches, canopy and cover generation, vertex
packing and deduplication run there. Versioned binary packets transfer their
buffers; the main thread admits at most two outstanding jobs and uploads ready
packets within a small frame budget. Teleports and quality changes invalidate job
tickets. The bounded synchronous path remains available during worker startup
or after a worker failure. Water bathymetry, player collision and atmosphere
sampling still run on the main thread.

Ground cover keeps the 400% foreground setting through 80m, tapers to zero at
300m, and uses 72/36/24-vertex template banks with hysteresis. Middle-distance trees keep a sparse subset of the original leaf cards and crown
positions, shared between the main view and reflections. Chunk-bound hysteresis
reduces detail beyond 244m and restores it below 204m, preventing repeated LOD
switching at the boundary. This is not a per-tree crossfade; outer-distance trees
still use solid silhouettes. Detailed leaf shadows remain in the sun pass so
canopy godrays retain their gaps. Horizon
land and water have separate buffers; invisible water skips refraction copies
and reflection updates. The 17 procedural materials remain unchanged (2.83 MiB).

Only one visible game view per origin submits frames. A BroadcastChannel lease
handles embedded browsers that report multiple previews as visible; input takes
over immediately. Paused views also stop streaming work and progress saves.

F3 includes a sampled GPU draw span when timestamps are supported. Individual
pass intervals can overlap on Apple tile GPUs and must **not** be added together
or interpreted as exclusive shading costs. The span excludes the preceding
water simulation and cloud-cache refresh. The browser performance check waits for streaming to drain,
freezes the sun clock, hides diagnostics during measurement, and reports frame
intervals separately from CPU submission time. Walking checks include streaming.

Additional verification: `node scripts/verify-streaming.mjs` checks actual WASM
worker packets; `node scripts/verify-ui.cjs` covers input, saved settings, worker
fallback/resume and single-view rendering.

### Cached lighting and foliage continuity

A world-anchored 512 × 512 R8 texture caches low-cloud density across 32.8km
(256 KiB), refreshing up to eight times per weather second. Surface elevation and
light direction still project each shadow onto that cloud plane. Camera movement
does not drag the pattern; large travel recenters on the same 64m sample lattice.
The cache fades to the analytic density at its edges, and the visible sky remains
procedural. Fast weather clocks refresh more often.

Foliage retains full material highlights nearby, blending to a cheaper wetness-aware
response from 24–64m. Diffuse light, leaf transmission, enclosure, directional
shadows and godrays remain enabled. The ray reconstruction also skips light
texture reads whose blend weight is exactly zero, retaining its exact-depth
fallback and original integration sample count. Snow noise and empty fire-light calculations
are skipped when they cannot contribute. Texture resolution and 400% foreground
cover are unchanged. Middle-distance alpha cards trade a little extra memory and
overdraw for better silhouette continuity, so net performance is measured using
browser frame intervals rather than inferred from triangle counts.

See [the measured comparison](docs/performance-lighting-cache.md) for conditions,
raw results and limitations of the current lighting-cache build.


## Anti-aliasing and motion stability (local, September 12)

Settings → Advanced graphics has independent Off / FXAA / SMAA edge smoothing, with FXAA as the default.
SMAA uses the three-pass High implementation and canonical area/search tables. Both modes
run after bloom/tone mapping and before the pixel palette and CRT display; HUD text remains crisp.
The dense meadow filters unresolved pigment detail, gently widens thin distant ribbons and
scales wind with projected size and fade. Its near coverage and terrain anchoring are retained.

Optional adaptive resolution targets 60 FPS within 420–720p without changing world quality,
400% grass density or godrays. It uses sustained frame timing and cautious upward probes;
the demanding forest can still fall below 60 FPS at the lower bound. Performance check can
compare all AA modes at fixed daylight, or record an eight-second walk separately from measurement.

See [implementation and validation](docs/antialiasing.md). The checkpoint before this change
is `checkpoint/pre-antialiasing-20260912` (`c8fb458`). This experiment has not been deployed.

## Recommended visual defaults

The baseline is Native resolution, Balanced world detail, FXAA, Bloom at 100%,
400% ground cover with meadow carpet, sun/moon shadows, water reflections and
ambient enclosure shading. Godrays retain their current renderer settings.
Adaptive resolution is opt-in. Existing saved preferences are respected.

The main settings panel contains resolution and mouse sensitivity. Advanced
graphics and world tools/experiments are collapsed by default. Restore recommended
defaults applies the complete visual baseline without resetting position, seed,
atlas, waypoint, sensitivity, time or weather. No engine or shader changes were
needed for this settings simplification.

## Forest foliage refinement

Foliage uses fewer overlapping broadleaf cards, volumetric crown normals and
tapered conifer sprays. Distant leaves use one atlas lookup and a lightweight
wet sheen; directional shadows, wind and godrays remain. See
[forest foliage notes](docs/forest-foliage.md) for exact changes, Safari scene
observations and validation limits.
# Forest communities

Five recurring forest communities now mix tall canopy trees, sapling patches,
shrubs and ferns. Use **Settings → World tools → Explore landscape** to visit
them. See [forest communities](docs/forest-communities.md) for the procedural art
direction, rendering budgets and verification details.
