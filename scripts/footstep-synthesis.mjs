// Offline boot-contact synthesis. Every contact has a finite attack/release;
// texture is filtered friction, never raw sample impulses or electrical chirps.
const TAU=Math.PI*2;
const profiles={
 grass: {cutoff:1150,body:.115,texture:.34,decay:30,brush:.12,grains:3},
 leaves:{cutoff:1900,body:.100,texture:.40,decay:27,brush:.13,grains:8},
 mud:   {cutoff:520, body:.145,texture:.38,decay:24,brush:.14,grains:2},
 gravel:{cutoff:1450,body:.150,texture:.42,decay:34,brush:.08,grains:9},
 stone: {cutoff:1700,body:.170,texture:.27,decay:58,brush:.025,grains:2},
 wood:  {cutoff:900, body:.140,texture:.20,decay:48,brush:.035,grains:1},
 sand:  {cutoff:800, body:.095,texture:.36,decay:27,brush:.13,grains:4},
 snow:  {cutoff:1250,body:.110,texture:.38,decay:24,brush:.14,grains:6},
 water: {cutoff:1100,body:.095,texture:.44,decay:26,brush:.12,grains:5},
};
export function footstep(material,seconds,random,rate){
 const p=profiles[material];if(!p)throw Error(`Unknown footstep material: ${material}`);
 const out=new Float32Array(Math.round(seconds*rate));
 const pitch=.93+random()*.14,heel=.012+random()*.004,toe=.060+random()*.018;
 const grains=Array.from({length:p.grains},()=>({at:.024+random()*.135,width:.005+random()*.016,gain:.10+random()*.23}));
 const alpha=1-Math.exp(-TAU*p.cutoff/rate);
 let smooth1=0,smooth2=0,bodyNoise=0;
 for(let i=0;i<out.length;i++){
  const t=i/rate,n=random()*2-1;
  smooth1+=alpha*(n-smooth1);smooth2+=alpha*(smooth1-smooth2);
  bodyNoise+=.045*(n-bodyNoise);
  const h=Math.max(0,t-heel),to=Math.max(0,t-toe);
  const contact=t>heel?(1-Math.exp(-h*600))*Math.exp(-h*p.decay):0;
  const roll=t>toe?(1-Math.exp(-to*160))*Math.exp(-to*24):0;
  // Heel weight and quieter sole roll give one coherent step, without a
  // sustained pitched sweep. Soft materials damp the high-frequency contact.
  const body=Math.sin(TAU*82*pitch*h)*Math.exp(-h*55)*contact;
  let grain=0;
  for(const g of grains){const q=(t-g.at)/g.width;if(Math.abs(q)<1)grain+=g.gain*(.5+.5*Math.cos(Math.PI*q));}
  let v=p.body*body+bodyNoise*.24*contact;
  v+=smooth2*p.texture*(contact*.75+roll*p.brush*3+grain);
  if(material==='wood')v+=Math.sin(TAU*165*pitch*h)*Math.exp(-h*65)*contact*.035;
  // Smooth finite tail; exact zero boundaries at every playback rate.
  const edge=Math.min(1,t/.008,(seconds-t)/.045);
  out[i]=Math.tanh(v)*Math.max(0,edge)*.9;
 }
 out[0]=0;out[out.length-1]=0;
 return out;
}
