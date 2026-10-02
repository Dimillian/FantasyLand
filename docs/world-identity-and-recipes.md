# World identity, saves, recipes and atlas cache

Only the current generator, recipe and save format are supported. Seeds are unsigned 32-bit integers, including zero and 4294967295. Development changes are allowed to be destructive: no historical generator implementations, save migrations or automatic backup/recovery paths are required.

## Current-version policy

`worldgen::WorldDescriptor` identifies reproducible geography and keys reconstructible atlas data. Main-thread and worker engines must agree on the descriptor. Changes to terrain, hydrology, placement, topology or stable entity identities require a generator/recipe revision bump so previous coordinates and cached terrain cannot be applied to different geography. A missing current-version save starts fresh even when older versions of that seed were played. Invalid or incompatible data in the current local slot is discarded on load. Importing an incompatible file is rejected; it is never converted or applied to the current world.

Persistent location keys are namespaced exact logical coordinates (`settlement:i:j:slot`, `cultural:i:j:slot`, `natural:i:j:slot`). Random u32 hashes still drive appearance, not persistent location identity. Existing exact settlement/building runtime IDs remain usable for current door state. Scope all keys by WorldDescriptor when adding discoveries or location deltas.

## Mutable state and storage

`savegame::Snapshot` contains identity, schema, player position/look/vitals/distance, world clock and door states. Geography, house layouts and citizen schedules regenerate from the descriptor and clock. Movement resumes on the valid walking surface rather than restoring an in-flight jump. Citizen transient avoidance motion and weather particle positions are not saved.

`dist/world-store.mjs` stores the current snapshot per seed/version, separately from renderer preferences. Saving overwrites that slot directly, without retaining previous snapshots or active-version pointers. Preferences load only from the current preference key. Import/export supports the current JSON format; importing the active seed suppresses navigation autosave so it cannot overwrite the imported state. Validation occurs before writes and again in Rust before engine mutation. Browser storage failures do not prevent starting a world; save failures remain visible and export works without localStorage. Exported files are optional manual copies, with no promise of compatibility after an engine update.

Do not store a continent's rendered chunks in a save. Future gameplay systems should add versioned mutable deltas, keyed by stable location identity, rather than making generated meshes authoritative.

## Recipe boundaries

- `src/worldgen/mountains.rs`: immutable mountain envelope recipes, separate from analytic field evaluation in geography.rs.
- `src/natural/catalog.rs`: family placement requirements, including slope, support relief and walk-through clearance.
- `src/natural/recipes.rs`: seeded natural structure builders; common solids still drive rendering and collision.
- `src/geometry/structures.rs`: cultural markers, camps and their structural grounding helpers.
- `src/worldgen.rs`: world contract and shared natural placement configuration.

These remain statically compiled Rust recipes, with no runtime scripting/interpreter overhead. Broader settlement/tree systems remain their existing specialized modules. This pass does not introduce a new POI blueprint or placement reservation system.

## Atlas

A view key includes world seed, generator and recipe revision, atlas visual revision, layer, resolution, center and span. Cached records contain both raster pixels and static feature overlays. Player position and waypoint overlays remain live.

The cache uses a 16 MiB RAM LRU and a 32 MiB IndexedDB budget shared across atlas namespaces. Disk pruning reads small metadata records, not every raster. Saves and reconstructible atlas records use separate storage. A first visit generates on the existing world worker; returning to a view reuses memory, and revisiting after reload can use IndexedDB. Only the newest viewport request may update the map. If a worker is unavailable, generation falls back locally; browser storage being unavailable only disables the persistent cache.

## Mountain changes and validation

Six irregular continental mountain groups and five island groups replace the three long continental backbones. Wider, shorter envelopes, larger bends, variable shoulders, staggered summit groups and eight asymmetric spurs per group break up the stripe pattern. Existing passes, traversable valleys, cirques, snow and drainage still use the same analytic descriptors and derivatives. This is an analytic landform model, not a tectonic or erosion simulator.

Validation commands:

- `cargo test --release --lib --locked` with the bundled toolchain.
- `node scripts/verify-world-store.mjs` (identity/seed range, current saves, destructive reset, invalid imports, cache budgets).
- `node scripts/verify-ui.cjs` (real interface orchestration against engine boundary stubs, including import/autosave ordering).
- `node scripts/verify-wasm.mjs` (compiled generation, worker identity, query independence and geography checks).
- `/worldgen-check.html` (isolated actual browser localStorage and IndexedDB checks).
- `node scripts/benchmark-worldgen.mjs [module] [wasm]` (CPU generation timing, not an FPS benchmark).

The isolated WASM/Node comparison on this machine measured mean construction at 2.54 s before and 2.85 s after (four seeds), approximately 12% more initialization work for the richer mountain field. Continental elevation raster generation at 384² took 173–179 ms; static features took 2–4 ms. These are development measurements, not a universal timing guarantee. This pass adds no rendering passes. Browser cache latency is verified separately.

The live game atlas check measured 318.3 ms for an uncached local view and 0.3 ms to reopen that same view from RAM (including raster presentation). The isolated browser IndexedDB test read its small verification packet in 0.9 ms. Full-view disk latency will vary with the stored overlay size.
