import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import { descriptor } from '../dist/world-store.mjs';
import init, { StreamGenerator, Game, inspect_world, inspect_map, inspect_routes, inspect_landscapes } from '../dist/pkg/fantasy_land.js';
await init({ module_or_path: await fs.readFile(new URL('../dist/pkg/fantasy_land_bg.wasm', import.meta.url)) });
for(const seed of [0,4294967295]) {
  const generator=new StreamGenerator(seed);
  assert.deepEqual(generator.world_identity(),descriptor(seed),'Rust/browser identity agrees across the seed range');
  const before=generator.map_layer_data(0,0,384000,32,1);
  generator.map_features(10000,20000,100000);
  assert.deepEqual(generator.map_layer_data(0,0,384000,32,1),before,'Atlas pixels do not depend on query order');
  generator.free();
}
for(const name of ['world_identity','save_snapshot','restore_snapshot'])assert.equal(typeof Game.prototype[name],'function');
const a = inspect_world(1337, 0, 0);
const b = inspect_world(1337, 0, 0);
assert.deepEqual(a, b, 'the compiled WASM reproduces its seed');
assert.ok(Number.isFinite(a.height));
assert.equal(typeof a.suitability.dryGround, 'boolean');
for (const field of ['buildable', 'freshWater', 'shelter', 'fertility', 'crossing', 'harbor', 'habitation']) {
  assert.ok(Number.isFinite(a.suitability[field]) && a.suitability[field] >= 0 && a.suitability[field] <= 1, `${field} suitability is bounded in compiled WASM`);
}
for (const name of ['set_weather_mode', 'set_weather_speed', 'set_weather_paused', 'set_reflections', 'set_enclosure', 'visit_interior']) {
  assert.equal(typeof Game.prototype[name], 'function', `${name} is exposed in the actual WASM bindings`);
}
assert.equal(typeof Game.prototype.walking_journeys, 'function');
assert.equal(typeof Game.prototype.natural_destinations, 'function');
assert.ok(a.hydrology.confluences > 100, 'the compiled drainage network joins tributaries');
assert.ok(a.hydrology.outlets > 0 && a.hydrology.outlets < a.hydrology.headwaters, 'catchments merge toward outlets');
assert.ok(a.vegetation_density >= 0 && a.vegetation_density <= 1, 'ecology provides bounded tree occupancy');
assert.notEqual(a.height, inspect_world(42, 0, 0).height, 'seeds change the world');
assert.ok(a.sites.length > 0);
const spawn = inspect_world(1337, ...a.spawn);
assert.ok(spawn.road > .8, 'the player starts on a road');
assert.ok(spawn.height > spawn.water_height, 'the starting point is on dry ground');
for (const span of [6000, 384000, 768000]) {
  const start = performance.now();
  const map = inspect_map(1337, 0, 0, span, 128);
  assert.equal(map.length, 128 * 128 * 4);
  assert.ok(map.every((v, i) => i % 4 !== 3 || v === 255));
  console.log(`${span / 1000} km map from compiled WASM: ${(performance.now() - start).toFixed(1)} ms`);
}
console.log('WASM checks passed: deterministic drainage, seed changes, ecology, dry road spawn, atlas scales.');

const routes = inspect_routes(1337, -16546, -12304, 16000);
assert.ok(Array.isArray(routes) && routes.length > 0, 'classified atlas routes serialize into JS');
for (const route of routes) {
  assert.ok(Number.isSafeInteger(route.id), 'route IDs remain safe JS numbers');
  assert.ok(['main', 'lane', 'trail'].includes(route.kind));
  assert.ok(route.points.length >= 2 && route.points.every(p => p.length === 2 && p.every(Number.isFinite)));
}
const regional = inspect_routes(1337, -16546, -12304, 60000);
assert.ok(regional.length > 0 && regional.every(r => r.kind === 'main'), 'regional atlas shows only main roads');
console.log('WASM road checks passed: classified route serialization, finite geometry, zoom hierarchy.');

assert.equal(a.world_size, 384000, 'expanded world includes the surrounding ocean');
const ocean = inspect_world(1337, 185000, 185000);
assert.ok(ocean.ocean && ocean.height < -100 && ocean.water_height === 0, 'deep ocean has a sea-level surface');
assert.equal(ocean.landmass, undefined, 'open sea is not a landmass');
assert.equal(ocean.vegetation_density, 0);
assert.equal(ocean.sites.length, 0, 'no offshore settlements');
assert.equal(inspect_routes(1337, 185000, 185000, 6000).length, 0, 'no roads across open ocean');
console.log('WASM coastal checks passed: expanded map, deep ocean, sea level, no offshore trees, roads or settlements.');

assert.equal(typeof Game.prototype.set_ground_cover_density, "function", "density control is present in the actual WASM bindings");

assert.equal(typeof Game.prototype.set_shadows, "function", "sun shadows are exposed in the actual WASM bindings");
assert.ok(['ancient_woodland', 'granite_highlands', 'windswept_coast', 'wet_lowlands', 'sandstone_country', 'meadowlands', 'alpine'].includes(a.landscape.kind));
for (const field of ['exposure', 'soil', 'wetness', 'rockiness', 'ancient', 'pale']) assert.ok(a.landscape[field] >= 0 && a.landscape[field] <= 1, `${field} remains bounded`);
assert.ok(a.lakes.length > 0 && a.lakes.length === a.hydrology.retained_lakes, 'retained basins serialize with the drainage network');
const lake = a.lakes[0];
const water = inspect_world(1337, ...lake.center);
assert.ok(water.height < lake.surface && Math.abs(water.water_height - lake.surface) < .01, 'retained lake has a flat surface above its basin floor');
assert.ok(lake.outlet.every(Number.isFinite) && lake.area_km2 > 0);
console.log('WASM regional checks passed: landscape fields, retained lake surface, outlet metadata, shadow control.');

const destinations = inspect_landscapes(1337);
const landscapeFamilies = ['Ancient woodland', 'Granite highlands', 'Windswept coast', 'Wet lowlands', 'Sandstone country', 'Meadowlands', 'Alpine heights'];
const destinationNames = new Set(destinations.map(d => d.name));
for (const family of landscapeFamilies) {
  assert.ok(destinationNames.has(family), `landscape tour includes ${family} alongside biome and forest destinations`);
}
assert.equal(destinationNames.size, destinations.length, 'tour destinations have distinct names');
assert.ok(destinations.every(d => Number.isFinite(d.x) && Number.isFinite(d.z) && Number.isFinite(d.yaw) && Number.isFinite(d.pitch)));
assert.equal(typeof Game.prototype.face, 'function');
console.log(`WASM landscape tour passed: all seven landscape families within ${destinations.length} distinct finite destinations.`);
