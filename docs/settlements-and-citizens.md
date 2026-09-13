# Settlements and citizens

This is the first playable settlement simulation. It uses generated geometry and pixel sprites; it adds no authored models, downloaded textures, AI service, commerce or guild mechanics.

## Try it locally

Run `sh scripts/build.sh`, then `python3 scripts/serve.py`. Open http://127.0.0.1:4173/ in Safari. Settings → World tools → Settlements offers two examples of each size. Visit a place, walk its streets and aim at a person or door; press **E**. **Escape** leaves a conversation. The atlas shows building footprints and streets at close zoom.

## Generation contracts

- The seed produces one authoritative settlement catalogue, followed by the regional road network, then lazily generated detailed layouts. Terrain and hydrology are sampled without settlements during site/parcel selection, avoiding recursive generation.
- Camps have 3–5 structures, hamlets 4–8, forts 12–18, villages 14–25, towns 46–75 and capital targets 170–230. Unsuitable lots are rejected. Capitals are rare, chosen within 48 km regions on broad dry ground. A location can remain isolated from regional roads.
- Parcels, street graph, door portals, room anchors and names are shared by rendering, collision, directions and NPC routines. Short local lanes are checked for building intersections, water and excessive grade; disconnected parcels are removed.
- Foundations and graded lots use actual terrain samples. Fort/city walls have gate gaps at local and regional access routes. Detailed layouts use a bounded cache; names and identities regenerate deterministically.

## Buildings and illumination

Homes, tents, inns, smithies, markets, guilds, arcane halls, barracks, temples, civic halls and stables have procedural shells. Enclosed rooms contain furniture, fireplaces and, in larger public buildings, wall torches. Windows are actual holes, not painted rectangles. Interiors are seamless, single-storey rooms; tall halls/watch houses do not yet have upper floors.

The mesh and collision use the same dimensions. E animates a hinged leaf. Public doors are accessible; homes are accessible when a household member has returned, and exits remain usable. Passing residents open doors automatically. Sleeping residents currently remain simple standing sprites inside homes; beds are scenery.

Twelve nearby room probes attenuate outdoor ambient/sunlight and admit direct light through the actual apertures. Eight nearby fire sources provide bounded, flickering illumination. Source and target room checks prevent light crossing opaque exterior walls; this is an analytic approximation, not full indoor global illumination or furniture shadows. Door daylight follows the rotated leaf. Room/source selection is cached while stationary, while door angles remain live. Only the people-light update owns the light array, including outdoor campfires.

## People and simulation

Humans have stable names, households, home/work bindings, age, birthplace, personality and a backstory. Sixteen roles include ordinary residents, guards, hunters, rangers, fighters, merchants, innkeepers, smiths, scholars, healers, mages and arcanists. A code-generated 128×256×16 RGBA atlas uses directional walking poses and material slots for skin, hair, clothes, leather, metal and accessories. Per-person palettes and proportions vary without allocating a texture per person; the sprite atlas is 2 MiB.

A day lasts 48 real minutes. Homes, work, neighbourhood walks and evening visits form deterministic daily routines along validated street/portal routes. Nearby communities retain their full household records, while at most 320 nearby people are drawn. A 75 ms pose update and bounded visual extrapolation reduce per-frame simulation cost. Static settlement meshes stream with world chunks; dynamic sprites and doors reuse GPU buffers. Human sprites currently do not cast individual shadows.

Sparse merchant companies and escorts travel selected main-road polylines, with directions referring to actual end settlements. These are pedestrian caravans for now: no pack animals or wheeled wagons. Distant movement is derived from the clock and route rather than ticking every person across the continent. There is no simulated trade economy yet.

## Dialogue and persistence

E opens structured dialogue with factual topics: current activity, life, home, local area, weather, destinations and directions to named local buildings. Directions use relative compass bearings and approximate paces; they do not add map/compass markers. Responses come from the same generated records and current simulation state, not invented service locations. All service buildings remain non-functional as requested.

Conversation pauses movement, life, day/night and weather. It restores input on Escape. Save v4 retains the world clock and existing player/visual/map preferences. NPC identities derive from the seed; this pass does not persist inventories, relationships or door states across reloads. Those should be saved as deltas keyed by stable person/building IDs when their systems are introduced.

## Verification

- `cargo test --release --lib -- --test-threads=2`: engine regressions including six settlement sizes, graph reachability, dry routes, door collision, population capacity, stable identities, schedule boundaries, facts and pause.
- `node scripts/verify-ui.cjs`: existing UI coverage plus E dialogue, topic responses, focus, Escape and in-world door interactions.
- `target/release/verify output/settlements 1337 settlement-data`: reproducible catalogue counts, full sample layouts and connectivity/grounding reports.
- `target/release/verify output/settlements 1337 settlement-scenes`: actual wgpu capital, daylight interior and night interior captures. Native captures are visual checks, not Safari FPS measurements.

The seed-1337 catalogue contains 32 capitals, 307 towns, 950 villages, 228 forts, 1,155 hamlets and 1,061 camps. The inspected capital Mistfield has 230 buildings and 918 generated residents. In a stationary native CPU-only check with 320 nearby actors, simulation plus dynamic mesh construction averaged 0.19–0.23 ms per iteration (p95 0.42–0.64 ms); this excludes GPU drawing, WASM and streaming, so it is not a browser frame-rate claim. Results are in `docs/benchmarks/settlements-native-cpu-20260913.json`.

The pre-change git tag is `checkpoint/pre-settlements-20260913`. Development remains local; no deployment is automatic.
