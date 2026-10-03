// Offline foley design: filtered air, cloth friction, damped wood/armour modes.
// No pitched sweeps or sustained pure-tone oscillators. Runtime just plays PCM.
const TAU=2*Math.PI;
export function combatSound(kind,seconds,random,rate){
  const out=new Float32Array(Math.round(rate*seconds));
  const tone=.90+random()*.2;
  const modes=(kind==='block'?[182,291,437,703,1261,2137,3473]:kind==='hit'?[89,147,237,593,1499]:[79,133,249]).map((hz,i)=>{
    const decay=(kind==='block'?(i<4?30:19):52)+random()*16;
    const radius=Math.exp(-decay/rate);
    return {a:2*radius*Math.cos(TAU*hz*tone/rate),b:radius*radius,y1:0,y2:0,gain:(kind==='block'?(i<4?.35:.055):.23)/(1+i*.6)};
  });
  let air=0,air2=0,body=0,cloth=0,dc=0;
  const contacts=Array.from({length:5},(_,i)=>({at:.014+i*.027+random()*.012,width:.009+random()*.012,volume:.3+random()*.3}));
  for(let i=0;i<out.length;i++){
    const t=i/rate,n=random()*2-1;
    body+=.047*(n-body);cloth+=.18*(n-cloth);
    let v=0;
    if(kind==='swing'){
      // Accelerating blade: broad air rises quickly, then relaxes; the glove
      // and sleeve add a soft irregular swish rather than a laser-like note.
      const q=t/seconds,crest=Math.exp(-(((q-.36)/.20)**2));
      const cutoff=550+crest*2250,alpha=1-Math.exp(-TAU*cutoff/rate);
      air+=alpha*(n-air);air2+=alpha*(air-air2);
      const envelope=Math.sin(Math.PI*q)**2*crest;
      v=(air2-body)*envelope*.72+cloth*.10*Math.sin(Math.PI*q)**2;
    }else{
      const attack=1-Math.exp(-t*1400),hit=attack*Math.exp(-t*100);
      let friction=0;
      for(const c of contacts){const q=(t-c.at)/c.width;if(Math.abs(q)<1)friction+=(.5+.5*Math.cos(Math.PI*q))*c.volume;}
      const excitation=n*hit;
      let resonance=0;
      for(const m of modes){const next=m.a*m.y1-m.b*m.y2+excitation*.006;m.y2=m.y1;m.y1=next;resonance+=next*m.gain;}
      if(kind==='block')v=(n-cloth)*hit*.22+cloth*hit*.65+resonance*.85+cloth*friction*.07;
      else if(kind==='hit')v=body*attack*Math.exp(-t*29)*1.9+cloth*hit*.85+resonance*.25+cloth*friction*.13;
      else v=body*attack*Math.exp(-t*26)*1.8+cloth*hit*.36+resonance*.22+cloth*friction*.09;
    }
    // Remove DC, retain headroom and feather both boundaries against clicks.
    dc+=.0012*(v-dc);v-=dc;
    const edge=Math.max(0,Math.min(1,t/.002,(seconds-t)/.035));
    out[i]=Math.tanh(v*1.7)*edge;
  }
  let peak=0;for(const v of out)peak=Math.max(peak,Math.abs(v));
  const target=kind==='swing'?.38:kind==='block'?.63:kind==='hit'?.55:.46;
  const gain=target/Math.max(.001,peak);
  for(let i=0;i<out.length;i++)out[i]*=gain;
  out[0]=0;out[out.length-1]=0;
  return out;
}
