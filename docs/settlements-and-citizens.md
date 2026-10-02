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

Homes, tents, inns, smithies, markets, guilds, arcane halls, barracks, temples, civic halls and stables have procedural shells. Enclosed rooms contain furniture, fireplaces and, in larger public buildings, wall torches. Windows have recessed timber frames, mullions, transoms and rippled leaded glass. A separate nearby transparent batch uses depth testing without writing depth or filling window holes in the sun-shadow map. Interiors remain seamless and single-storey; homes and larger lodgings have a partitioned sleeping room with an open central doorway. Beds, pillows, blankets, nightstands, tables, chairs, cupboards, books and crockery share a geometry recipe with collision. Tents use low bedrolls and a small exterior fire. Tall halls do not yet have upper floors.

Five wall variants use brick, clapboard, stone and two plaster palettes, with three roof palettes. Building-local metric UVs keep courses aligned through rotation. The packed vertex path encodes signed metric UVs in the existing four UV bytes, retaining repeats without increasing vertex bandwidth. One roof pitch defines the gables, roof planes and continuous bargeboards. The procedural atlas has 28 layers, each 128×128 with mipmaps, using approximately 4.67 MiB for albedo and surface data combined.

The mesh and collision use the same dimensions. E animates a hinged leaf. Public doors are accessible; homes are accessible when a household member has returned, and exits remain usable. Passing residents open doors automatically. Sleeping residents currently remain simple standing sprites inside homes; beds are scenery.

Twelve nearby room probes attenuate outdoor ambient/sunlight and admit direct light through the actual apertures. Eight nearby fire and candle sources provide bounded, flickering illumination. Source and target room checks prevent light crossing opaque exterior walls; this is an analytic approximation, not full indoor global illumination or furniture shadows. Door daylight follows the rotated leaf. Bedroom partitions block direct sun and cross-room firelight. Window masks include the timber and leadwork. A room dust volume reuses the existing half-resolution godray pass; the containing or immediately adjacent room is selected so shafts remain visible while approaching the doorway. Glass highlights follow the existing sun shadows and weather, with derivative filtering for distant leadwork. Room/source selection is cached while stationary, while door angles remain live. Only the people-light update owns the light array, including outdoor campfires.

## People and simulation

Humans have stable names, households, home/work bindings, age, birthplace, personality and a backstory. Sixteen roles include ordinary residents, guards, hunters, rangers, fighters, merchants, innkeepers, smiths, scholars, healers, mages and arcanists. A code-generated 192×384×64 RG8 atlas contains 48×96 frames, four directions, four walking poses and four costume/body silhouettes for each of sixteen roles. Two channels store shade and material ID; material zero is transparent. Material slots represent skin, hair, clothes, leather, metal and accessories. Per-person palettes and proportions vary without allocating a texture per person; the shared sprite atlas occupies 9 MiB. Cloaks, split-hem tunics, tall boots, tabards, pauldrons, embroidered robes, pouches, lace collars and caps give the silhouettes a high-fantasy appearance.

A day lasts 48 real minutes. Homes, work, neighbourhood walks and evening visits form deterministic daily routines along validated street/portal routes. Nearby communities retain their full household records, while at most 320 nearby people are drawn. A 75 ms pose update and bounded visual extrapolation reduce per-frame simulation cost. Static settlement meshes stream with world chunks; dynamic sprites and doors reuse GPU buffers. Human sprites currently do not cast individual shadows.

Sparse merchant companies and escorts travel selected main-road polylines, with directions referring to actual end settlements. These are pedestrian caravans for now: no pack animals or wheeled wagons. Distant movement is derived from the clock and route rather than ticking every person across the continent. There is no simulated trade economy yet.

## Dialogue and persistence

E opens structured dialogue with factual topics: current activity, life, home, local area, weather, destinations and directions to named local buildings. Directions use relative compass bearings and approximate paces; they do not add map/compass markers. Responses come from the same generated records and current simulation state, not invented service locations. All service buildings remain non-functional as requested.

The interface uses an original bundled 5×7 pixel font and a field-codex treatment: oxblood binding, brass details, parchment pages and dark ink. A 64×80 procedural miniature is drawn once when a conversation opens; its skin, hair and garment palette come from the same NPC appearance record as the world sprite. No extra world render pass or downloaded portrait is needed. Dialogue keeps the last 12 exchanges. Short topic labels retain full questions in the transcript and accessible names. Mouse, arrows, Home/End, Enter and 1–9 / A–C work together; Tab cycles visible enabled controls in every modal. The transcript keeps native keyboard scrolling when focused. Browser modifier shortcuts are preserved. The atlas adds Enter centre selection, bracket cycling through places, Home to the player and End to the whole world.

Conversation pauses movement, life, day/night and weather. It restores input on Escape. Save v4 retains the world clock and existing player/visual/map preferences. NPC identities derive from the seed; this pass does not persist inventories, relationships or door states across reloads. Those should be saved as deltas keyed by stable person/building IDs when their systems are introduced.

## Verification

- `cargo test --release --lib -- --test-threads=2`: engine regressions including six settlement sizes, graph reachability, dry routes, door collision, population capacity, stable identities, schedule boundaries, facts and pause.
- `node scripts/verify-ui.cjs`: existing UI coverage plus E dialogue, topic responses, focus, Escape and in-world door interactions.
- `target/release/verify output/settlements 1337 settlement-data`: reproducible catalogue counts, full sample layouts and connectivity/grounding reports.
- `target/release/verify output/settlement-art 1337 settlement-art`: additional brick/timber/stone exteriors, a furnished bedroom and morning window light.
- `target/release/verify output/settlements 1337 settlement-scenes`: actual wgpu capital, daylight interior and night interior captures. Native captures are visual checks, not Safari FPS measurements.

The seed-1337 catalogue contains 32 capitals, 307 towns, 950 villages, 228 forts, 1,155 hamlets and 1,061 camps. The inspected capital Mistfield has 230 buildings and 918 generated residents. In a stationary native CPU-only check with 320 nearby actors, simulation plus dynamic mesh construction averaged 0.19–0.23 ms per iteration (p95 0.42–0.64 ms); this excludes GPU drawing, WASM and streaming, so it is not a browser frame-rate claim. Results are in `docs/benchmarks/settlements-native-cpu-20260913.json`.

The original settlement tag is `checkpoint/pre-settlements-20260913`; the art/UI checkpoint is `checkpoint/pre-settlement-art-20260913`. Development remains local; no deployment is automatic.

The compact dialogue was measured in a real browser DOM at 1280×720 and 960×540. All twelve topics fit without scrolling at both sizes; long conversation history scrolls independently. The browser UI check uses an isolated fixture over a native engine capture, without starting a second GPU renderer.

Art validation: the full 139-test engine suite passed; the subsequent signed-UV packing regression and citizen portrait metadata checks also passed. Eight native wgpu art captures were regenerated after the packing fix. UI checks cover modal focus, transcripts, all twelve topic commands, atlas selection, saved settings and renderer startup. Final local WebGPU boot was checked; these checks are not a new Safari FPS benchmark.


## October 2026 interior and citizen redraw

Checkpoint before this pass: `checkpoint/pre-interior-people-rethink-20261002`.

The art study used the [original Daggerfall screenshot catalogue](https://www.mobygames.com/game/778/the-elder-scrolls-chapter-ii-daggerfall/screenshots/) and this [Daggerfall Unity tavern reference](https://items.gog.com/daggerfall_unity/3Bb.png). The latter is a modern Unity/modded reference, not a claim about the original DOS renderer. The useful visual principles were strong timber framing, discrete pools of warm light, readable profession-specific silhouettes and recognizable furnishings. All new art remains original and generated in code; no reference sprites or textures were imported.

`interior_design.rs` now owns semantic room recipes and a small vocabulary of furniture: dining settings, chairs with separate rails and legs, cupboards/open shelving, bound books, handled tankards, glazed jugs, bottles, hooped barrels, iron-bound chests, bedsteads and curtained canopy beds. Rotational vessel profiles give pottery and barrels actual round silhouettes. Interiors have flagstones or timber floors, recessed wall panels, plaster, joists, cornices, woven runners and complete heraldic wall hangings. Inns have a long serving counter and bottle shelves; arcane/guild rooms have books and desk objects; smithies have a workbench, tools and an anvil; temples/halls have side altars and benches; markets/stables remain open structures. Major furniture shares its bounds with player collision. The center passage, bedroom doorway and window apertures stay clear. Small non-solid ornaments are dropped at the next detail tier; interiors are still omitted at distant LODs.

Warmth comes from the actual hearth, a hanging four-candle fixture and bedroom candles, with the original eight-light limit retained. Containing-room lights take priority over lights behind neighbors' walls. Flame meshes and light locations agree; markets and stables do not spawn unsupported indoor fires. This remains bounded analytic room lighting, not full indirect illumination or per-object point-light shadows.

Citizen plates were redrawn with articulated sleeves and boots, face planes, ears, beards, plaits, layered robes, mail/plate accents, guild tabards, merchant chains, hunting equipment, books, courier scrolls and innkeeper trays. Four silhouettes vary build, age presentation and garments rather than merely scaling one plate. Residents reserve distributed indoor activity positions; their approach spurs are checked against furniture bounds and preserve the original door/room anchors. The elder appearance follows generated age. All views and walking poses use the same palette slot contract, preserving runtime recoloring. The original one-quad-per-person render path and actor limit remain; the feet use the visible sprite baseline rather than the bottom of its transparent border. Wrapped character lighting avoids black paper silhouettes when a billboard rotates, while retaining world shadow and room-light visibility.

For a direct local tour, open **O → World tools & experiments → Lighting studies** and choose one of the six **Interior** entries. These locate real generated rooms in the current seed; landscape lighting studies retain their original seed requirement. Furniture is decorative and residents still use the existing routines; this art pass does not introduce sitting/sleeping animations, new services or multi-storey navigation.

Verification captures use `target/release/verify output/interior-rethink/after 1337 settlement-art`. In addition to matched before/after views, this produces tavern bar/table views, a guest chamber, an arcanist room, smithy, temple and home at dusk, plus a color reference sheet of all 64 front-facing costume variants. Captures use the real wgpu renderer at 540p internal resolution presented at 1280×720. Native capture timings are not browser FPS measurements.

Validation for this pass: all 141 engine tests passed, followed by the citizen schedule regression after the indoor position change; browser-shell checks include interior visits on a non-default seed. Compiled WASM and native GPU captures are checked separately.
