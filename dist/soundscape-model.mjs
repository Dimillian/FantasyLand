// Pure mix policy: engine facts in, bounded gains and ecological activity out.
export const clamp=(x,a=0,b=1)=>Math.max(a,Math.min(b,Number.isFinite(x)?x:0));
export function mixFor(s){
  const a=s.audio||{},w=s.weather||{},biome=(s.biome||'').toLowerCase(),forest=clamp(a.forest??(s.forest?.8:0));
  const indoor=clamp(a.indoor),outdoor=1-indoor*.86,wind=clamp(Math.hypot(w.windX||0,w.windZ||0)/32),rain=clamp(w.rain),snow=clamp(w.snow);
  const severe=clamp(Math.max(rain*wind,snow*wind));
  const night=(s.dayTime??12)<5||(s.dayTime??12)>20;
  const wet=/swamp|wetland/.test(biome),tropical=/tropical|rainforest|jungle/.test(biome),dry=/desert|savanna/.test(biome),alpine=/alpine/.test(biome);
  const pine=/pine|cedar|fir|conifer/i.test(`${s.forest||''} ${biome}`);
  const water=clamp(a.water?.[3]),fire=clamp(a.fire?.[3]);
  const life=clamp(1-rain*1.12-snow*.95-wind*.48)*(1-indoor);
  const dawn=Math.max(0,1-Math.abs((s.dayTime??12)-7)/3);
  let bird=night?(wet?'frog':forest>.3?'owl':null):wet?'frog':tropical?'bird-tropical':/coast/.test(biome)||a.waterKind==='surf'?'gull':alpine||dry?'crow':forest>.25?'bird-forest':'bird-meadow';
  return {indoor,outdoor,severe,night,bird,activity:life*(night?.38:.65+dawn*.35)*(dry||alpine?.35:1),
    loops:{air:(.10+wind*.70)*outdoor,leaves:forest*(pine?.22:.62)*(.12+wind*.88)*outdoor,
      needles:forest*(pine?.62:.09)*(.08+wind*.92)*outdoor,
      rain:rain*(.20+.52*rain)*outdoor,roof:rain*indoor*.58,
      'snow-wind':snow*(.10+wind*.60)*outdoor,
      insects:(night?.16:.035)*(wet||tropical?1:dry?.7:.45)*life,
      stream:water*(a.waterKind==='river'?.50:a.waterKind==='lake'?.16:0)*outdoor,
      surf:water*(a.waterKind==='surf'?.54:0)*(.55+wind*.65)*outdoor,
      fire:fire*(indoor?.56:.32)}};
}
export class StepClock {
  constructor(){this.reset();}
  reset(){this.last=null;this.distance=0;this.left=false;}
  update(s,moving=true){
    const current={walked:s.walked||0,x:s.x||0,z:s.z||0,grounded:!!s.grounded};const old=this.last;this.last=current;
    if(!old||Math.hypot(current.x-old.x,current.z-old.z)>12||current.walked<old.walked){this.distance=0;return null;}
    if(!moving){this.distance=0;return null;}
    if(!current.grounded){this.distance=0;return null;}
    if(!old.grounded){this.distance=0;return {landing:true,pan:0};}
    const delta=current.walked-old.walked;
    if(delta<0.001)return null;
    this.distance+=Math.min(delta,2.5);
    const stride=(s.audio?.swimming?1.8:(s.speed||0)>6?2.9:2.25);
    if(this.distance<stride)return null;
    this.distance%=stride;this.left=!this.left;
    return {landing:false,pan:this.left?-.12:.12};
  }
}
// Cinematic propagation: retain a distance cue without separating a flash and
// its sound by the 6–60 seconds implied by our exaggerated horizon scale.
export function thunderAllowed(w={}) {
  return [0,4,5].includes(w.mode??0) && (w.stormStrength??0)>=.28;
}
export const thunderDelay=distance=>clamp(distance/22000,.08,.85);
export class ThunderQueue {
  constructor(){this.reset();}
  reset(){this.seen=null;this.pending=[];this.position=null;this.mode=null;this.invalidated=false;}
  clear(eventId=this.seen){this.pending=[];this.seen=eventId;}
  update(s,now){
    const a=s.audio||{},e=a.lightningEvent,w=s.weather||{},mode=w.mode??0;
    const unavailable=!thunderAllowed(w),moved=this.position&&Math.hypot(s.x-this.position[0],s.z-this.position[1])>150;
    const changed=this.mode!==null&&this.mode!==mode;
    this.invalidated=unavailable||changed||!!moved;
    this.mode=mode;this.position=[s.x||0,s.z||0];
    if(unavailable||moved){this.clear(e?.[0]);return [];}
    if(changed)this.clear();
    if(e&&w.lightning>.035&&e[1]>=0&&e[1]<.7&&e[0]!==this.seen){
      this.seen=e[0];const source=a.thunderSource||[s.x+6000,2000,s.z];
      const d=Math.hypot(source[0]-s.x,source[1]-(s.y||0),source[2]-s.z);
      if(this.pending.length<4)this.pending.push({at:now+thunderDelay(d),source,distance:d,cloud:e[2]<.62,id:e[0]});
    }
    const ready=this.pending.filter(v=>v.at<=now);this.pending=this.pending.filter(v=>v.at>now);return ready;
  }
}
