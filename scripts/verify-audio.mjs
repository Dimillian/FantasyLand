import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mixFor,StepClock,ThunderQueue} from '../dist/soundscape-model.mjs';
import {Soundscape} from '../dist/soundscape.mjs';
const state={x:0,y:0,z:0,yaw:0,walked:0,grounded:true,speed:4,dayTime:10,biome:'Temperate forest',forest:'Oak woodland',weather:{windX:3,windZ:1,rain:0,snow:0},audio:{forest:.8,indoor:0,floor:'grass',water:[30,0,0,.8],waterKind:'river',fire:[0,0,0,0]}};
const fair=mixFor(state),storm=mixFor({...state,weather:{windX:30,rain:1,snow:0}}),inside=mixFor({...state,audio:{...state.audio,indoor:1}});
assert(storm.loops.rain>fair.loops.rain&&storm.loops.air>fair.loops.air);
assert.equal(storm.activity,0);assert(inside.loops.air<fair.loops.air*.2);assert.equal(inside.activity,0);
assert.equal(mixFor({...state,biome:'Tropical rainforest'}).bird,'bird-tropical');
assert.equal(mixFor({...state,biome:'Swamp',dayTime:23}).bird,'frog');
assert.equal(mixFor({...state,dayTime:23}).bird,'owl');
assert.equal(mixFor({...state,biome:'Desert'}).bird,'crow');
assert(mixFor({...state,forest:'Pine forest'}).loops.needles>fair.loops.needles);
for(const dayTime of [0,7,12,23])for(const biome of ['Desert','Alpine','Tropical coast','Swamp','Forest'])for(const rain of [0,.5,1])for(const indoor of [0,1]){
 const m=mixFor({...state,dayTime,biome,weather:{rain,snow:1-rain,windX:30},audio:{...state.audio,indoor}});
 assert(Object.values(m.loops).every(v=>Number.isFinite(v)&&v>=0&&v<=1));
}
const steps=new StepClock();assert.equal(steps.update(state),null);assert.equal(steps.update(state),null);
assert(steps.update({...state,walked:2.3,x:2.3}));
assert.equal(steps.update({...state,walked:3,x:3,grounded:false}),null);
assert(steps.update({...state,walked:3,x:3}).landing);
assert.equal(steps.update({...state,walked:20,x:200}),null);
assert.equal(steps.update({...state,walked:24,x:204},false),null);
const thunder=new ThunderQueue(),flash={...state,weather:{lightning:1},audio:{lightningEvent:[8,.1,.9,0],thunderSource:[3430,0,0]}};
assert.deepEqual(thunder.update(flash,0),[]);assert.equal(thunder.pending.length,1);
thunder.update(flash,.1);assert.equal(thunder.pending.length,1);assert.equal(thunder.update(flash,9.9).length,0);
assert.equal(thunder.update(flash,10).length,1);assert.equal(thunder.update(flash,11).length,0);
thunder.update({...flash,audio:{...flash.audio,lightningEvent:[9,.1,.9,0]}},12);
thunder.update({...flash,x:2000},13);assert.equal(thunder.pending.length,0);
console.log('PASS biome, weather, indoor mixes; movement cadence; delayed/unique/teleport-safe thunder');

// Graph lifecycle / voice budget against the actual player class.
class Param{value=0;setTargetAtTime(v){this.value=v;}}
class Node{constructor(){for(const k of ['gain','pan','frequency','Q','threshold','knee','ratio','attack','release','playbackRate'])this[k]=new Param();}connect(){return this;}disconnect(){this.disconnected=true;}start(){this.started=true;}stop(){this.onended?.();}}
class Context{currentTime=0;sampleRate=48000;state='suspended';destination=new Node();createGain(){return new Node();}createDynamicsCompressor(){return new Node();}createBiquadFilter(){return new Node();}createConvolver(){return new Node();}createBufferSource(){return new Node();}createStereoPanner(){return new Node();}createBuffer(ch,n,rate){const channels=Array.from({length:ch},()=>new Float32Array(n));return {length:n,numberOfChannels:ch,duration:n/rate,getChannelData:i=>channels[i]};}async resume(){this.state='running';}async suspend(){this.state='suspended';}}
globalThis.AudioContext=Context;
const muted=new Soundscape({master:0});muted.unlock();assert.equal(muted.ctx,null);
const audio=new Soundscape({master:.42,footsteps:.33});audio.load=async()=>{audio.ready=true;};
audio.setActive(true);audio.unlock();await Promise.resolve();assert.equal(audio.ctx.state,'running');assert.equal(audio.master.gain.value,.42);assert.equal(audio.gate.gain.value,1);
audio.buffers.set('test',{duration:1});for(let i=0;i<50;i++)audio.play('test','footsteps',.4);
assert.equal(audio.voices.size,18);audio.setActive(false);assert.equal(audio.voices.size,0);
await new Promise(resolve=>setTimeout(resolve,280));assert.equal(audio.ctx.state,'suspended');
audio.setActive(true);assert.equal(audio.ctx.state,'running');audio.setVolume('master',2);assert.equal(audio.master.gain.value,1);
audio.setActive(false);audio.setActive(true);await new Promise(resolve=>setTimeout(resolve,280));assert.equal(audio.ctx.state,'running');
console.log('PASS unlock, saved volumes, mute, voice cap, cleanup and suspend/resume race');
const manifest=JSON.parse(readFileSync(new URL('../dist/audio/manifest.json',import.meta.url)));
for(const [id,m] of Object.entries(manifest.sounds)){
 const b=readFileSync(new URL('../dist/audio/'+m.file,import.meta.url));assert.equal(b.toString('utf8',0,4),'RIFF');assert.equal(b.readUInt32LE(40),b.length-44);
 assert(m.peak<.95&&m.rms>.001,`${id}: unsafe or silent sound`);assert(Number.isFinite(m.rms));
 if(m.loop){const n=(b.length-44)/2,read=i=>b.readInt16LE(44+i*2)/32768;let energy=0;for(let i=1;i<n;i++)energy+=(read(i)-read(i-1))**2;assert(Math.abs(read(n-1)-read(0))<Math.sqrt(energy/n)*5+.002,`${id}: loop seam`);}
}
console.log(`PASS ${Object.keys(manifest.sounds).length} PCM assets, levels, headers and loop continuity`);
