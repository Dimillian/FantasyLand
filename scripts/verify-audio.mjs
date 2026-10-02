import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {mixFor,StepClock,ThunderQueue,thunderDelay} from '../dist/soundscape-model.mjs';
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
const thunder=new ThunderQueue(),flash={...state,weather:{lightning:1,stormStrength:1,mode:5},audio:{lightningEvent:[8,.1,.9,0],thunderSource:[3430,0,0]}};
assert.deepEqual(thunder.update(flash,0),[]);assert.equal(thunder.pending.length,1);
thunder.update(flash,.1);assert.equal(thunder.pending.length,1);assert.equal(thunder.update(flash,.12).length,0);
assert.equal(thunder.update(flash,.2).length,1);assert.equal(thunder.update(flash,11).length,0);
thunder.update({...flash,audio:{...flash.audio,lightningEvent:[9,.1,.9,0]}},12);
thunder.update({...flash,x:2000},13);assert.equal(thunder.pending.length,0);
// Weather overrides clear even while smoothed rain/severity still look stormy.
for(const mode of [1,2,3,6,7,8]){
 const q=new ThunderQueue();q.update(flash,0);assert.equal(q.pending.length,1);
 assert.deepEqual(q.update({...flash,weather:{...flash.weather,mode}},.1),[]);
 assert.equal(q.pending.length,0);assert(q.invalidated);
 assert.deepEqual(q.update({...flash,weather:{...flash.weather,mode}},30),[]);
}
const arrival=new ThunderQueue();arrival.update({...state,weather:{mode:1,stormStrength:0}},0);arrival.update(flash,.1);assert.equal(arrival.pending.length,1,'A fresh rendered strike is retained when entering storm mode');
const front=new ThunderQueue();front.update({...flash,weather:{...flash.weather,mode:0}},0);
front.update({...flash,weather:{mode:0,stormStrength:.1,lightning:0}},.1);assert.equal(front.pending.length,0);
assert(thunderDelay(21000)<=.85);assert(thunderDelay(2000)<thunderDelay(15000));
// No duplicate on the return stroke, paused envelope, or continuing frame packets.
const frames=new ThunderQueue();frames.update(flash,0);frames.update({...flash,audio:{...flash.audio,lightningEvent:[8,.36,.9,0]}},.08);
assert.equal(frames.pending.length,1);assert.equal(frames.update(flash,1).length,1);assert.equal(frames.update(flash,2).length,0);
console.log('PASS biome, weather, indoor mixes; movement cadence; delayed/unique/teleport-safe thunder');

// Graph lifecycle / voice budget against the actual player class.
class Param{
 value=0;events=[];held=0;
 cancelScheduledValues(t){this.events=this.events.filter(e=>e.time<t);}
 cancelAndHoldAtTime(t){this.held++;this.events=[];}
 setValueAtTime(v,t){this.value=v;this.events.push({value:v,time:t});}
 setTargetAtTime(v,t){this.value=v;this.events.push({value:v,time:t});}
}
class Node{constructor(){for(const k of ['gain','pan','frequency','Q','threshold','knee','ratio','attack','release','playbackRate'])this[k]=new Param();}connect(){return this;}disconnect(){this.disconnected=true;}start(t){this.started=true;this.startTime=t;}stop(t){this.stopTime=t;this.onended?.();}}
class Context{constructor(options){this.options=options;}currentTime=0;sampleRate=48000;state='suspended';destination=new Node();createGain(){return new Node();}createDynamicsCompressor(){return new Node();}createBiquadFilter(){return new Node();}createConvolver(){return new Node();}createBufferSource(){return new Node();}createStereoPanner(){return new Node();}createBuffer(ch,n,rate){const channels=Array.from({length:ch},()=>new Float32Array(n));return {length:n,numberOfChannels:ch,duration:n/rate,getChannelData:i=>channels[i]};}async resume(){this.state='running';}async suspend(){this.state='suspended';}}
globalThis.AudioContext=Context;
const muted=new Soundscape({master:0});muted.unlock();assert.equal(muted.ctx,null);
const audio=new Soundscape({master:.42,footsteps:.33});audio.load=async()=>{audio.ready=true;};
audio.setActive(true);audio.unlock();await Promise.resolve();assert.equal(audio.ctx.state,'running');assert.equal(audio.master.gain.value,.42);assert.equal(audio.gate.gain.value,1);
audio.buffers.set('test',{duration:1});for(let i=0;i<50;i++)audio.play('test','footsteps',.4);
assert.equal(audio.voices.size,18);audio.setActive(false);assert.equal(audio.voices.size,0);
await new Promise(resolve=>setTimeout(resolve,280));assert.equal(audio.ctx.state,'suspended');
audio.setActive(true);assert.equal(audio.ctx.state,'running');audio.setVolume('master',2);assert.equal(audio.master.gain.value,1);
audio.setActive(false);audio.setActive(true);await new Promise(resolve=>setTimeout(resolve,280));assert.equal(audio.ctx.state,'running');
audio.buffers.set('thunder-0',{duration:7});audio.buffers.set('thunder-cloud-0',{duration:7});audio.pick=prefix=>`${prefix}-0`;
audio.ctx.currentTime=3;
// The real player consumes a render-frame packet even between HUD updates.
const packet=[100,.04,.9,0,1,1,5,0,0,0,0,3430,0,0];
audio.updateLightningFrame(packet);assert.equal(audio.thunder.pending.length,1);
audio.ctx.currentTime=3.2;audio.updateLightningFrame(packet);assert.equal([...audio.voices].filter(v=>v.tag==='thunder').length,1);
audio.play('test','footsteps',.2);audio.ctx.currentTime=3.3;
const clearPacket=[...packet];clearPacket[6]=1;audio.updateLightningFrame(clearPacket);
assert.equal(audio.thunder.pending.length,0);assert.equal([...audio.voices].filter(v=>v.tag==='thunder').length,0);
assert.equal(audio.voices.size,1,'Clearing thunder must preserve unrelated sounds');
// In-cloud events select the diffuse roll, and explicit UI cancellation removes it.
audio.ctx.currentTime=4;audio.updateLightningFrame([...packet.slice(0,6),5,...packet.slice(7)]);
audio.ctx.currentTime=4.1;const cloudPacket=[...packet];cloudPacket[0]=101;cloudPacket[2]=.2;
audio.updateLightningFrame(cloudPacket);audio.ctx.currentTime=4.3;audio.updateLightningFrame(cloudPacket);
assert.equal([...audio.voices].filter(v=>v.tag==='thunder').length,1);audio.cancelThunder();assert.equal(audio.voices.size,1);
console.log('PASS unlock, saved volumes, mute, voice cap, cleanup and suspend/resume race');
const manifest=JSON.parse(readFileSync(new URL('../dist/audio/manifest.json',import.meta.url)));
for(const [id,m] of Object.entries(manifest.sounds)){
 const b=readFileSync(new URL('../dist/audio/'+m.file,import.meta.url));assert.equal(b.toString('utf8',0,4),'RIFF');assert.equal(b.readUInt32LE(40),b.length-44);
 assert(m.peak<.95&&m.rms>.001,`${id}: unsafe or silent sound`);assert(Number.isFinite(m.rms));
 if(m.loop){const n=(b.length-44)/2,read=i=>b.readInt16LE(44+i*2)/32768;let energy=0;for(let i=1;i<n;i++)energy+=(read(i)-read(i-1))**2;assert(Math.abs(read(n-1)-read(0))<Math.sqrt(energy/n)*5+.002,`${id}: loop seam`);}
}
console.log(`PASS ${Object.keys(manifest.sounds).length} PCM assets, levels, headers and loop continuity`);

// Playback headroom and continuous fades, independent of the render frame.
assert.equal(audio.ctx.options.latencyHint,'balanced');
audio.play('test','footsteps',.4);
const stepVoice=[...audio.voices].at(-1);
assert(stepVoice.source.startTime>=audio.ctx.currentTime+.02);
audio.setActive(false);
assert(stepVoice.source.stopTime>audio.ctx.currentTime);
assert.equal(stepVoice.gain.gain.value,0);
audio.setActive(true);
// Long walks continually vary gusts and stereo position. Future ramp queues
// remain bounded, and unchanged targets (e.g. the indoor filter) are not added.
const loop={source:new Node(),gain:new Node(),pan:new Node()};audio.loops.set('air',loop);
const beforeHolds=audio.outdoorFilter.frequency.held;
for(let i=0;i<12000;i++){
 audio.ctx.currentTime=10+i*.1;
 audio.update({...state,weather:{...state.weather,gust:Math.sin(i*.017)*.5+.5}},{moving:false});
 assert(loop.gain.gain.events.length<=1);
 assert(loop.pan.pan.events.length<=1);
}
assert.equal(audio.outdoorFilter.frequency.held,beforeHolds+1);
// Older WebKit's fallback retains the current value before replacing a ramp.
const fallback=new Param();fallback.cancelAndHoldAtTime=undefined;
audio.buses.footsteps.gain=fallback;fallback.value=.3;
audio.setVolume('footsteps',.5);
assert.equal(fallback.events[0].value,.3);assert.equal(fallback.events.at(-1).value,.5);
console.log('PASS scheduled playback lead, faded interruption, 20-minute automation soak and WebKit fallback');

// Catch the previous broadband-static / single-sample-pop footstep palette.
// This inspects the actual distributed PCM, not just synthesizer parameters.
for(const [id,m] of Object.entries(manifest.sounds)){
 if(!id.startsWith('step-'))continue;
 const b=readFileSync(new URL('../dist/audio/'+m.file,import.meta.url));
 let energy=0,difference=0,maxJump=0,last=0;
 assert.equal(b.readInt16LE(44),0,`${id}: onset click`);
 assert.equal(b.readInt16LE(b.length-2),0,`${id}: tail click`);
 for(let i=44;i<b.length;i+=2){const v=b.readInt16LE(i)/32768;energy+=v*v;
  if(i>44){difference+=(v-last)**2;maxJump=Math.max(maxJump,Math.abs(v-last));}last=v;}
 assert(difference/energy<.35,`${id}: excessive uncorrelated high-frequency noise`);
 assert(maxJump<.12,`${id}: sample impulse / pop`);
}
console.log('PASS all 36 shipped footstep variants: smooth boundaries, bounded sample jumps and filtered texture');
