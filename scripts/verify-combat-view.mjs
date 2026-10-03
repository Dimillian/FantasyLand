import assert from 'node:assert/strict';
import {equipmentRecipe,paintEquipment,createEquipmentSprite} from '../dist/equipment-sprites.mjs';
import {projectCombat,combatBarPosition} from '../dist/combat-view.mjs';
const f=Array(21).fill(0);const w=1280,h=720;
assert.deepEqual(projectCombat([0,0,-5],f,w,h),{x:w/2,y:h/2,depth:5});
assert.equal(projectCombat([0,0,5],f,w,h),null);
f[15]=Math.PI/2;assert(Math.abs(projectCombat([5,0,0],f,w,h).x-w/2)<.001);
f[16]=.3;assert(projectCombat([5,0,0],f,w,h).y>h/2,'Upward look lowers horizon-relative targets.');
for(const kind of ['sword','shield','axe','mace']){
 const r=equipmentRecipe(41,kind);assert.deepEqual(r,equipmentRecipe(41,kind));
 const points=[];let operations=0;
 const ctx={fillRect(...p){points.push(p);operations++;},beginPath(){operations++;},closePath(){},fill(){},moveTo(...p){points.push(p);},lineTo(...p){points.push(p);}};
 paintEquipment(ctx,r);assert(operations>35&&operations<5000);assert(points.flat().every(Number.isFinite));
 const sprite=createEquipmentSprite(r,()=>({getContext:()=>ctx}));assert.equal(sprite.width,746);assert.equal(sprite.height,420);
}
assert.notDeepEqual(equipmentRecipe(1),equipmentRecipe(10000));
console.log('PASS: deterministic layered equipment recipes, finite bounded sprite geometry, and camera-aligned combat projection.');

assert.deepEqual(combatBarPosition({x:400,y:-700,depth:.8},800,420),{x:400,y:70});
assert.deepEqual(combatBarPosition({x:790,y:900,depth:2},800,420),{x:734,y:320});
assert.equal(combatBarPosition(null,800,420),null);
console.log('PASS: point-blank and edge-of-screen health bars remain inside the viewport.');

// Exercise the actual depth-tested material bake, not just geometry calls.
const bake=(kind,light=1)=>{
 let result;
 const ctx={createImageData:(w,h)=>({data:new Uint8ClampedArray(w*h*4)}),putImageData:image=>{result=image.data;}};
 paintEquipment(ctx,equipmentRecipe(41,kind),{width:360,height:240,light});return result;
};
for(const kind of ['sword','shield']){
 const pixels=bake(kind),dim=bake(kind,.4);assert.deepEqual(pixels,bake(kind),'Recipe must bake deterministically');
 let covered=0,energy=0,dark=0;const palette=new Set();
 for(let i=0;i<pixels.length;i+=4){if(!pixels[i+3])continue;covered++;energy+=pixels[i]+pixels[i+1]+pixels[i+2];dark+=dim[i]+dim[i+1]+dim[i+2];palette.add(`${pixels[i]},${pixels[i+1]},${pixels[i+2]}`);}
 assert(covered>300&&covered<360*240*.60,'Equipment remains cropped around a clear world view');assert(palette.size>80,'Materials have meaningful texture variation');assert(dark<energy*.65,'Ambient light affects textured surfaces');
}
console.log('PASS: deterministic textured raster, transparent coverage, material variation and ambient response.');

// Pose work stays off the render loop; recipes are transferred as plain data.
const {EquipmentSpriteCache}=await import('../dist/equipment-cache.mjs');
const {registerEquipment}=await import('../dist/equipment-items.mjs');
registerEquipment('test-relic',({r,box})=>{box([0,.24,0],[.11,.25,.06],r.trim);});
const relic=equipmentRecipe(17,'test-relic');assert.deepEqual(relic,structuredClone(relic));
assert.throws(()=>equipmentRecipe(1,'unknown-item'),/Unknown equipment/);
assert.notEqual(relic.key,equipmentRecipe(18,'test-relic').key);
assert.notEqual(relic.key,equipmentRecipe(17,'test-relic',{materials:{trim:[1,2,3]}}).key);
const jobs=[];let closed=0;
globalThis.OffscreenCanvas=class {};
globalThis.Worker=class {postMessage(job){jobs.push(job);}terminate(){}};
const view=new EquipmentSpriteCache();view.initBaker();view.warmSprites(746,1);
assert.equal(jobs.length,1);view.sprite('mainHand',18,746,1);assert.equal(jobs.length,1);
view.worker.onmessage({data:{...jobs[0],image:{width:100,height:100,close(){closed++;}},originX:40,originY:200}});
assert.equal(jobs.length,2);assert.equal(jobs[1].meta.pose,18,'Requested pose takes priority over warm-up');
assert(view.sprite('mainHand',0,746,1));
view.setLoadout({mainHand:relic,offHand:null});view.sprite('mainHand',0,746,1);
assert.equal(view.sprite('offHand',0,746,1),null);
const before=closed;
view.worker.onmessage({data:{...jobs[1],image:{close(){closed++;}}}});
assert.equal(closed,before+1,'Stale equipment bake must be discarded after an item swap');
assert.equal(jobs.at(-1).recipe.key,relic.key,'Worker accepts custom recipes without knowing their type');
assert.equal(view.request('mainHand',0,746,1).options.hand,'right');
view.setLoadout({offHand:relic});assert.equal(view.request('offHand',0,746,1).options.hand,'left');
for(let i=0;i<8;i++)view.storeSprite(`large:${i}`,{width:1024,height:1024,close(){closed++;}});
assert(view.cacheBytes<=24*1048576);assert(closed>0);view.dispose();
delete globalThis.Worker;delete globalThis.OffscreenCanvas;
console.log('PASS: custom serializable recipes, loadout swaps, either hand, stale-worker rejection and bounded bitmap storage.');

// The complete shield silhouette must be opaque at every blocking pose. Project
// points just inside the board boundary, where the old displaced rim leaked sky.
const {shieldPose}=await import('../dist/equipment-rig.mjs');
for(const guard of [0,.25,.5,.75,1]){
 let pixels;const width=746,height=420;
 const ctx={createImageData:(w,h)=>({data:new Uint8ClampedArray(w*h*4)}),putImageData:image=>{pixels=image.data;}};
 paintEquipment(ctx,equipmentRecipe(19,'shield'),{width,height,guard});
 const {center,yaw}=shieldPose(guard);
 for(let i=0;i<360;i++)for(const radius of [.395,.403,.412,.418,.425]){
   const a=i*Math.PI/180,lx=Math.cos(a)*radius,ly=Math.sin(a)*radius;
   const p=[center[0]+lx*Math.cos(yaw)-.011*Math.sin(yaw),center[1]+ly,center[2]-lx*Math.sin(yaw)-.011*Math.cos(yaw)];
   const x=Math.floor(width/2+p[0]/p[2]*height*.70),y=Math.floor(height/2-p[1]/p[2]*height*.70);
   if(x>=0&&x<width&&y>=0&&y<height)assert(pixels[(y*width+x)*4+3]>0,`Shield rim gap at guard ${guard}, angle ${i}, radius ${radius}`);
 }
}
console.log('PASS: opaque shield body/rim through all five sampled guard poses.');
