import assert from 'node:assert/strict';
import {AdaptiveResolution} from '../dist/adaptive-resolution.js';
const simulate=(controller,duration,gap,start=0,eligible=true)=>{
  const changes=[];
  for(let now=start;now<start+duration;now+=gap){const result=controller.sample(now,gap,eligible);if(result!==null)changes.push({now,height:result});}
  return changes;
};
for(const hz of [60,120]){
  const controller=new AdaptiveResolution(540);
  assert.equal(simulate(controller,12000,1000/hz).length,0,'No premature quality probe.');
  const changes=simulate(controller,12000,1000/hz,12000);
  assert.equal(changes.length,1);assert.equal(changes[0].height,600);
}
const overloaded=new AdaptiveResolution(720);
const downs=simulate(overloaded,40000,33.333);
assert.deepEqual(downs.map(c=>c.height),[660,600,540,480,420]);
assert.ok(downs.every((c,i)=>i===0 || c.now-downs[i-1].now>4000),'No rapid resize oscillation.');
const hitches=new AdaptiveResolution(540);
for(let now=0;now<15000;now+=16.667)hitches.sample(now,Math.floor(now/16.667)%90===0?90:16.667,true);
assert.equal(hitches.height,540,'Isolated hitches must not lower resolution.');
const probe=new AdaptiveResolution(540);
const up=simulate(probe,24000,16.667);assert.equal(up[0].height,600);
const down=simulate(probe,7000,33.333,24000);assert.equal(down[0].height,540);
assert.equal(simulate(probe,40000,16.667,31000).length,0,'Failed probe must back off.');
const frozen=new AdaptiveResolution(540);
assert.equal(simulate(frozen,60000,33.333,0,false).length,0);
assert.equal(simulate(frozen,3000,33.333,60000,true).length,0,'Resume needs a fresh warmup.');
console.log('PASS: 60/120Hz cadence; stable floor; isolated hitches; failed-probe cooldown; hidden/modal/benchmark freeze and resume.');
