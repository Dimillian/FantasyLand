# FantasyLand — The Wilderness

A first-person procedural fantasy exploration slice, built with Rust, WebAssembly, and wgpu. All terrain, trees, rocks, crossings and landmark geometry are generated in code. The browser shell handles input, maps and a retro HUD; it does not render the 3D world.

## Play

- WASD / arrow keys: walk. Mouse: look. Shift: sprint. Space: jump.
- Click to capture the mouse. If pointer lock is unavailable, drag the view to look.
- M: local map. Tab: world map. Escape: close a map or release the mouse.
- Drag maps to pan, scroll to zoom, select a location to set a waypoint or fast travel.
- Settings: seed, quality, mouse sensitivity, daylight and return to spawn. F3: diagnostics.
- Progress and preferences are stored on this device. Add `?seed=42` to explore another deterministic seed.

## World

The world is 256 × 256 km (65,536 km²), with signed coordinates centered on zero. Terrain streams in 192 m chunks with several levels of detail. Coarse terrain patches extend the real horizon to roughly 20 km at the default setting; distant mountains remain part of the same walkable world. The default seed is 1337.

Rolling foothills, larger mountain ridges, continuous river corridors and a climate model produce grassland, temperate forest, pine forest, moor, alpine, dryland and wetland biomes. Shared road curves connect approximately 2.4 km settlement cells. Smaller points of interest occupy eligible 640 m cells with jitter, exclusions and occasional empty areas. Future settlement sites currently contain survey structures, banners and camps, not populated towns.

Walking is 5.5 m/s; sprinting reaches 9 m/s and consumes stamina. Settlement spacing targets roughly 5–10 minutes between neighboring sites on foot, excluding stops; mountain detours can take longer. Map estimates show straight-line walking time; terrain and routes can make the actual journey longer.

Six procedural tree silhouettes mix with biome-specific grass, ferns, flowers, heather, reeds, stones, stumps and fallen logs. Short wooden crossings meet river banks with ramps. Clouds, sunlight, night stars and flowing water are shader-generated, with no imported models or texture assets.

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
node scripts/verify-wasm.mjs
```

The native verifier runs the same wgpu shaders on a real GPU, checks the rendered output, and writes terrain/map images under `output/verification`. The Node check loads the actual compiled WASM and verifies world/map generation independently of the graphics backend.

## Current limits

- This is an exploration prototype: no combat, quests, living NPCs, town interiors or economy yet.
- All map destinations are available for fast travel while testing.
- Hydrology and geography are analytic approximations, not erosion simulation. River corridors mostly share a northward drainage direction.
- Roads connect sites and follow/level local terrain; distant mountain roads can be too steep and still need pass/switchback routing.
- Trunks and boulders block movement. Landmark structures are currently primarily visual, with limited structural collision.
- Terrain and props generate incrementally on the browser's main thread. Worker scheduling and instancing are future optimizations.
- The low-poly renderer is the first art target. ASCII++ is not implemented in this slice.

## Code

`world.rs` defines world identity, climate, terrain, routes, features and maps. `geometry.rs` builds mesh recipes and physical crossing floors. `player.rs` implements movement. `renderer.rs` manages wgpu, chunk streaming and presentation; `horizon.rs` generates distant terrain. `dist/app.js` supplies browser controls and atlas interactions. Generated browser bindings and WASM are checked into `dist/pkg` for static hosting.
