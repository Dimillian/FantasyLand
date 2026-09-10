import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import init, { inspect_world, inspect_map } from '../dist/pkg/fantasy_land.js';
await init({ module_or_path: await fs.readFile(new URL('../dist/pkg/fantasy_land_bg.wasm', import.meta.url)) });
const a = inspect_world(1337, 0, 0);
const b = inspect_world(1337, 0, 0);
assert.deepEqual(a, b, 'the compiled WASM reproduces its seed');
assert.ok(Number.isFinite(a.height));
assert.ok(a.hydrology.confluences > 100, 'the compiled drainage network joins tributaries');
assert.ok(a.hydrology.outlets > 0 && a.hydrology.outlets < a.hydrology.headwaters, 'catchments merge toward outlets');
assert.ok(a.vegetation_density >= 0 && a.vegetation_density <= 1, 'ecology provides bounded tree occupancy');
assert.notEqual(a.height, inspect_world(42, 0, 0).height, 'seeds change the world');
assert.ok(a.sites.length > 0);
const spawn = inspect_world(1337, ...a.spawn);
assert.ok(spawn.road > .8, 'the player starts on a road');
assert.ok(spawn.height > spawn.water_height, 'the starting point is on dry ground');
for (const span of [6000, 256000, 512000]) {
  const start = performance.now();
  const map = inspect_map(1337, 0, 0, span, 128);
  assert.equal(map.length, 128 * 128 * 4);
  assert.ok(map.every((v, i) => i % 4 !== 3 || v === 255));
  console.log(`${span / 1000} km map from compiled WASM: ${(performance.now() - start).toFixed(1)} ms`);
}
console.log('WASM checks passed: deterministic drainage, seed changes, ecology, dry road spawn, atlas scales.');
