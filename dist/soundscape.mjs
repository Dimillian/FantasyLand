import {mixFor,StepClock,ThunderQueue,clamp} from './soundscape-model.mjs?v=thunder-sync-1';
const BASE=new URL('./audio/',import.meta.url);
const DEFAULTS={master:.75,ambience:.80,footsteps:.45,wildlife:.75};
export class Soundscape {
  constructor(saved={}){
    this.volumes={};for(const [k,v] of Object.entries(DEFAULTS))this.volumes[k]=clamp(Number.isFinite(Number(saved?.[k]))?Number(saved[k]):v);
    this.ctx=null;this.ready=false;this.active=false;this.loading=false;this.failed=false;this.buffers=new Map();this.loops=new Map();this.voices=new Set();
    this.steps=new StepClock();this.thunder=new ThunderQueue();this.nextBird=0;this.variant=0;this.lastVariants={};this.latest={};this.lastUpdate=-1;this.activeEpoch=0;
  }
  setVolume(bus,value){if(bus in this.volumes)this.volumes[bus]=clamp(value);this.applyVolumes();}
  applyVolumes(){if(!this.ctx)return;const t=this.ctx.currentTime;for(const [name,gain] of Object.entries(this.buses||{}))gain.gain.setTargetAtTime(this.volumes[name]??1,t,.06);}
  // Called on the original click/key gesture; no autoplay or permission prompt.
  unlock(){
    if(this.failed||this.volumes.master<=0)return;
    try{
      if(!this.ctx){const Context=globalThis.AudioContext||globalThis.webkitAudioContext;if(!Context){this.failed=true;return;}
        this.ctx=new Context({latencyHint:'interactive'});this.buildGraph();this.load().catch(e=>{console.warn('Soundscape unavailable',e);this.failed=true;this.setActive(false);});
      }
      if(this.active&&this.ctx.state!=='running')this.ctx.resume().catch(()=>{});
    }catch(e){this.failed=true;console.warn('Soundscape unavailable',e);}
  }
  buildGraph(){
    const c=this.ctx;this.master=c.createGain();this.master.gain.value=0;
    const limiter=c.createDynamicsCompressor();limiter.threshold.value=-8;limiter.knee.value=12;limiter.ratio.value=8;limiter.attack.value=.006;limiter.release.value=.25;
    const highpass=c.createBiquadFilter();highpass.type='highpass';highpass.frequency.value=35;
    this.master.connect(highpass);highpass.connect(limiter);limiter.connect(c.destination);
    this.gate=c.createGain();this.gate.gain.value=this.active?1:0;this.gate.connect(this.master);
    this.buses={master:this.master};
    this.outdoorFilter=c.createBiquadFilter();this.outdoorFilter.type='lowpass';this.outdoorFilter.frequency.value=16000;this.outdoorFilter.Q.value=.45;this.outdoorFilter.connect(this.gate);
    for(const name of ['ambience','footsteps','wildlife']){const g=c.createGain();this.buses[name]=g;g.connect(name==='footsteps'?this.gate:this.outdoorFilter);}
    this.room=c.createConvolver();this.roomSend=c.createGain();this.roomSend.gain.value=0;this.room.connect(this.roomSend);this.roomSend.connect(this.gate);
    const ir=c.createBuffer(2,Math.floor(c.sampleRate*.65),c.sampleRate);let seed=91;
    for(let ch=0;ch<2;ch++){const d=ir.getChannelData(ch);for(let i=0;i<d.length;i++){seed=(Math.imul(seed,1664525)+1013904223)>>>0;d[i]=((seed/4294967296)*2-1)*Math.exp(-i/c.sampleRate*10)*.25;}}
    this.room.buffer=ir;this.applyVolumes();
  }
  async load(){
    this.loading=true;
    const response=await fetch(new URL('manifest.json',BASE));if(!response.ok)throw Error('Audio manifest unavailable');
    const manifest=await response.json();const entries=Object.entries(manifest.sounds);let cursor=0;
    // Four decodes at a time. No synthesis or per-sample work in the game RAF.
    await Promise.all(Array.from({length:4},async()=>{while(cursor<entries.length){const [id,entry]=entries[cursor++];const res=await fetch(new URL(`${entry.file}?v=${manifest.version}`,BASE));if(!res.ok)throw Error(`Missing sound ${id}`);const buffer=await this.ctx.decodeAudioData(await res.arrayBuffer());this.buffers.set(id,buffer);}}));
    for(const [id,entry] of entries){if(!entry.loop)continue;const c=this.ctx,source=c.createBufferSource(),gain=c.createGain(),pan=c.createStereoPanner();source.buffer=this.buffers.get(id);source.loop=true;gain.gain.value=0;source.connect(gain);gain.connect(pan);pan.connect(this.buses.ambience);source.start(0,Math.random()*source.buffer.duration);this.loops.set(id,{source,gain,pan});}
    this.ready=true;this.loading=false;if(this.active)this.ctx.resume().catch(()=>{});
  }
  setActive(active){
    active=!!active&&!this.failed;if(this.active===active)return;this.active=active;const epoch=++this.activeEpoch;
    this.steps.reset();this.thunder.reset();this.nextBird=0;
    if(!this.ctx)return;const c=this.ctx,t=c.currentTime;this.gate.gain.setTargetAtTime(active?1:0,t,.08);
    if(active){c.resume().catch(()=>{});}else{
      for(const v of [...this.voices]){try{v.source.stop();}catch{}}
      setTimeout(()=>{if(!this.active&&epoch===this.activeEpoch)c.suspend().catch(()=>{});},260);
    }
  }
  pick(prefix,count=3){let i=Math.floor(Math.random()*count);if(i===this.lastVariants[prefix])i=(i+1)%count;this.lastVariants[prefix]=i;return `${prefix}-${i}`;}
  panAt(source,s){const dx=source[0]-(s.x||0),dz=source[2]-(s.z||0);return clamp((dx*Math.cos(s.yaw||0)+dz*Math.sin(s.yaw||0))/Math.max(1,Math.hypot(dx,dz)),-1,1)*.85;}
  play(id,bus,gain,{pan=0,rate=1,source=null,lowpass=14000,reverb=0,tag=null}={}){
    if(!this.ready||!this.active||this.voices.size>=18||!this.buffers.has(id))return;
    const c=this.ctx,node=c.createBufferSource(),g=c.createGain(),p=c.createStereoPanner(),filter=c.createBiquadFilter();
    node.buffer=this.buffers.get(id);node.playbackRate.value=rate;g.gain.value=gain;p.pan.value=pan;filter.type='lowpass';filter.frequency.value=lowpass;filter.Q.value=.4;
    node.connect(filter);filter.connect(g);g.connect(p);p.connect(this.buses[bus]);
    let send=null;if(reverb>0){send=c.createGain();send.gain.value=reverb*this.volumes[bus];p.connect(send);send.connect(this.room);}
    const voice={source:node,gain:g,pan:p,position:source,tag,fading:false};this.voices.add(voice);
    node.onended=()=>{this.voices.delete(voice);node.disconnect();filter.disconnect();g.disconnect();p.disconnect();send?.disconnect();};node.start();
  }
  cancelThunder(clearQueue=true){
    if(clearQueue)this.thunder.clear();
    if(!this.ctx)return;
    const t=this.ctx.currentTime;
    for(const v of this.voices)if(v.tag==='thunder'&&!v.fading){
      v.fading=true;v.gain.gain.cancelScheduledValues(t);
      v.gain.gain.setTargetAtTime(0,t,.045);
      try{v.source.stop(t+.25);}catch{}
    }
  }
  // Compact packet sampled immediately after rendering, not on the 10 Hz HUD.
  updateLightningFrame(f){
    this.updateThunder({x:f[7],y:f[8],z:f[9],yaw:f[10],weather:{lightning:f[4],stormStrength:f[5],mode:f[6]},
      audio:{lightningEvent:f.slice(0,4),thunderSource:f.slice(11,14)}});
  }
  updateThunder(s){
    if(!this.ready||!this.active||this.ctx.state!=='running')return;
    const strikes=this.thunder.update(s,this.ctx.currentTime);
    if(this.thunder.invalidated)this.cancelThunder(false);
    // Bound overlapping rolls separately so accelerated weather cannot bury
    // the scene under a dozen long rumbles.
    for(const strike of strikes){
      const rolls=[...this.voices].filter(v=>v.tag==='thunder'&&!v.fading);
      if(rolls.length>=3){const oldest=rolls[0];oldest.fading=true;oldest.gain.gain.setTargetAtTime(0,this.ctx.currentTime,.06);oldest.source.stop(this.ctx.currentTime+.3);}
      this.play(this.pick(strike.cloud?'thunder-cloud':'thunder'),'ambience',strike.cloud?.85:1.05,
        {tag:'thunder',source:strike.source,pan:this.panAt(strike.source,s),rate:.94+Math.random()*.12,lowpass:strike.cloud?2400:6500});
    }
  }
  update(s,{moving=true,dialogue=false,lightningPerFrame=false}={}){
    this.latest=s;if(!this.ready||!this.active||this.ctx.state!=='running')return;
    const c=this.ctx,t=c.currentTime;if(t-this.lastUpdate<.055)return;this.lastUpdate=t;
    const mix=mixFor(s),a=s.audio||{},w=s.weather||{};
    this.outdoorFilter.frequency.setTargetAtTime(mix.indoor>0?1700:16000,t,.5);
    this.roomSend.gain.setTargetAtTime(mix.indoor*.22,t,.4);
    const gust=(.80+clamp(w.gust)*.35)*(.82+.12*Math.sin(t*.73+(s.x||0)*.009)+.06*Math.sin(t*1.81+(s.z||0)*.008));
    for(const [id,l] of this.loops){let v=mix.loops[id]||0;if(['air','leaves','needles','snow-wind'].includes(id))v*=gust;
      l.gain.gain.setTargetAtTime(v*(dialogue?.55:1),t,.65);
      const position=id==='fire'?a.fire:['stream','surf'].includes(id)?a.water:null;
      l.pan.pan.setTargetAtTime(position?this.panAt(position,s):id==='leaves'?-.3:id==='needles'?.3:0,t,.4);
    }
    for(const v of this.voices)if(v.position)v.pan.pan.setTargetAtTime(this.panAt(v.position,s),t,.10);
    const step=this.steps.update(s,moving&&!dialogue);
    if(step){const material=['grass','leaves','mud','gravel','stone','wood','sand','snow','water'].includes(a.floor)?a.floor:'grass';
      this.play(this.pick(`step-${material}`,4),'footsteps',step.landing?.55:.40,{pan:step.pan,rate:.92+Math.random()*.16,reverb:mix.indoor});}
    if(!lightningPerFrame)this.updateThunder(s);
    if(t>this.nextBird){this.nextBird=t+3+Math.random()*9;
      if(mix.bird&&Math.random()<mix.activity&&!dialogue){const angle=Math.random()*Math.PI*2,distance=10+Math.random()*35,position=[s.x+Math.sin(angle)*distance,s.y+4,s.z+Math.cos(angle)*distance];
        this.play(this.pick(mix.bird),'wildlife',.18+Math.random()*.18,{source:position,pan:this.panAt(position,s),rate:.88+Math.random()*.25,lowpass:8500});}
      if(mix.indoor&&Math.random()<.12)this.play(this.pick('creak'),'ambience',.25,{pan:Math.random()-.5,reverb:1});
    }
  }
  get status(){return this.failed?'unavailable':!this.ctx?'click to enable':this.loading?'loading':this.ctx.state;}
  get diagnostics(){return {status:this.status,active:this.active,loops:this.loops.size,voices:this.voices.size,pendingThunder:this.thunder.pending.length,decodedMiB:[...this.buffers.values()].reduce((n,b)=>n+b.length*b.numberOfChannels*4,0)/1048576};}
}
