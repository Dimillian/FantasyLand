import assert from 'node:assert/strict';
import {WorldStore,descriptor,validSeed,validateSave,worldKey} from '../dist/world-store.mjs';
import {AtlasCache} from '../dist/atlas-cache.mjs';
const data=new Map();const storage={getItem:k=>data.get(k),setItem:(k,v)=>data.set(k,v),removeItem:k=>data.delete(k)};
const store=new WorldStore(storage);
const save=seed=>({schema:1,snapshot:{schema:3,character:{strength:12,endurance:10,agility:10,intellect:10,willpower:10,sword_equipped:true,shield_equipped:true,inventory:[{id:"iron-sword",quantity:1},{id:"oak-shield",quantity:1}],blades:{rank:2,xp:17},blocking:{rank:1,xp:32}},corpses:[],defeated:["monster:landmark:2:3:0"],world:descriptor(seed),clock:2500,player:{x:10,z:20,yaw:1,pitch:.2,health:90,mana:80,stamina:70,walked:150},doors:[{id:45,angle:.5,target:1,hold:20}]},waypoint:{x:50,z:60,name:'Ruin'},atlas:{x:0,z:0,span:6000}});
for(const seed of [0,42,1337,4294967295]){store.write(save(seed));assert.deepEqual(store.load(descriptor(seed)),save(seed));}
assert.equal(validSeed('0'),0);assert.equal(validSeed('-1'),1337);assert.equal(validSeed('4294967295'),4294967295);
const changed=save(42);changed.snapshot.player.x=999;store.write(changed);
assert.equal(store.load(descriptor(42)).snapshot.player.x,999);assert.equal(store.load(descriptor(1337)).snapshot.player.x,10);
assert.equal([...data.keys()].filter(k=>k.endsWith('.backup')).length,0);
for(const mutate of [s=>s.snapshot.world.generator_version++,s=>s.snapshot.world.recipe_revision++,s=>s.snapshot.player.x=NaN,s=>s.snapshot.player.health=-1,s=>s.snapshot.doors.push(s.snapshot.doors[0]),s=>s.atlas.span=NaN,s=>s.snapshot.schema=1,s=>s.snapshot.character.blades.xp=99999,s=>s.snapshot.character.blocking.rank=0,s=>s.snapshot.defeated.push(s.snapshot.defeated[0]),s=>s.snapshot.character.sword_equipped="yes"]){
 const bad=save(42);mutate(bad);assert.throws(()=>store.write(bad));assert.equal(store.load(descriptor(42)).snapshot.player.x,999);
 data.set(store.key(descriptor(77)),JSON.stringify(bad));assert.equal(store.load(descriptor(77)),null);
 assert(!data.has(store.key(descriptor(77))),'Discard unusable local state');
}
const corrupted=store.key(descriptor(77));data.set(corrupted,'broken');assert.equal(store.load(descriptor(77)),null);
store.write(save(77));assert.equal(store.load(descriptor(77)).snapshot.player.x,10);
data.set(corrupted,'broken again');store.write(save(77));assert.equal(store.load(descriptor(77)).snapshot.player.x,10);
const exported=JSON.stringify(changed);assert.deepEqual(validateSave(JSON.parse(exported)),changed);
data.set('fantasyland.world.active.100','g99:r1:s100');data.set('fantasyland.world.g99:r1:s100','obsolete');
assert.equal(store.load(descriptor(100)),null,'An older played version must not block a fresh world');
store.write(save(100));assert.equal(store.load(descriptor(100)).snapshot.player.x,10);
const denied=new WorldStore({getItem(){throw Error('Storage denied');}});assert.throws(()=>denied.load(descriptor(42)));
const cache=new AtlasCache(worldKey(descriptor(42)),{memoryBytes:600});
const q={x:0,z:0,span:6000,layer:0,res:8},pixels=new Uint8Array(8*8*4),features={sites:[]};
assert.equal(await cache.get(q),null);cache.put(q,pixels,features);assert.deepEqual((await cache.get(q)).pixels,pixels);
assert.equal(await cache.get({...q,layer:1}),null);assert.equal(await cache.get({...q,span:7000}),null);
cache.put({...q,x:1},pixels,features);cache.put({...q,x:2},pixels,features);assert.equal(await cache.get(q),null);assert(cache.bytes<=600);
assert.notEqual(cache.key(q),new AtlasCache(worldKey(descriptor(43))).key(q));
console.log('PASS: full seed range, current-only saves, destructive reset, validation, roundtrip, atlas reuse/invalidation and bounded memory.');

const corpseSave=save(1337);corpseSave.snapshot.corpses=[{id:corpseSave.snapshot.defeated[0],kind:0,position:[10,20,30],items:[{id:'old-bone',quantity:2}]}];
assert.deepEqual(validateSave(JSON.parse(JSON.stringify(corpseSave))),corpseSave);
for(const mutate of [s=>s.snapshot.corpses[0].items[0].quantity=0,s=>s.snapshot.corpses[0].id='monster:unknown',s=>s.snapshot.character.inventory.push({id:'invented',quantity:1}),s=>s.snapshot.corpses[0].position[0]=Infinity]){const bad=structuredClone(corpseSave);mutate(bad);assert.throws(()=>validateSave(bad));}
console.log('PASS: corpse loot roundtrip and invalid inventory/corpse rejection.');
