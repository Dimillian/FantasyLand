import assert from 'node:assert/strict';
import {synth} from './audio-synthesis.mjs';
for(const kind of ['swing','hit','block','hurt']){
 const length={swing:.32,hit:.30,block:.48,hurt:.34}[kind];
 const a=synth(`combat-${kind}`,length,81),b=synth(`combat-${kind}`,length,97);
 assert.deepEqual(a,synth(`combat-${kind}`,length,81));assert.notDeepEqual(a,b);
 let peak=0,energy=0,jump=0,mean=0;
 for(let i=0;i<a.length;i++){assert(Number.isFinite(a[i]));peak=Math.max(peak,Math.abs(a[i]));energy+=a[i]*a[i];mean+=a[i];if(i)jump=Math.max(jump,Math.abs(a[i]-a[i-1]));}
 assert.equal(a[0],0);assert.equal(a.at(-1),0);assert(peak<=.64&&peak>.2);assert(Math.abs(mean/a.length)<.015);assert(Math.sqrt(energy/a.length)>.015);assert(jump<.9);
 console.log(`PASS ${kind}: peak ${peak.toFixed(3)}, RMS ${Math.sqrt(energy/a.length).toFixed(3)}, zero boundaries, distinct deterministic variants`);
}
