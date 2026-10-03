import {combatSound} from './combat-synthesis.mjs';
import {footstep} from './footstep-synthesis.mjs';
// Original procedural sound palette. No recordings, downloads or external assets.
export const RATE=22050;
export function rng(seed=1){let s=seed>>>0;return ()=>{s^=s<<13;s^=s>>>17;s^=s<<5;return (s>>>0)/4294967296;};}
const tau=Math.PI*2;
export function synth(kind,seconds,seed=1){
  if(kind.startsWith('combat-'))return combatSound(kind.slice(7),seconds,rng(seed),RATE);
  if(kind.startsWith('step-'))return footstep(kind.slice(5),seconds,rng(seed),RATE);
  const r=rng(seed),n=Math.round(seconds*RATE),a=new Float32Array(n);let low=0,mid=0,slow=0,phase=0;
  const bird=kind.startsWith('bird');
  const pitch=.86+r()*.28;
  for(let i=0;i<n;i++){
    const t=i/RATE,u=t/seconds,white=r()*2-1;
    low+=.014*(white-low);mid+=.19*(white-mid);slow+=.0018*(white-slow);
    const breath=.58+.20*Math.sin(tau*u*2+.6)+.12*Math.sin(tau*u*5+2.1);
    let v=0;
    if(kind==='air')v=(low*.85+slow*3)*breath;
    if(kind==='leaves')v=(white-mid)*.18*(.28+breath*.72)*(1+.32*Math.sin(t*17)*Math.sin(t*31));
    if(kind==='needles')v=(mid-low)*.58*breath;
    if(kind==='rain')v=(white*.17+mid*.24)*(.68+breath*.32);
    if(kind==='roof')v=low*1.3+mid*.16+((r()<.003)?(r()-.5)*.32:0);
    if(kind==='stream')v=mid*.40+low*.62+Math.sin(t*690+Math.sin(t*31)*8)*.016*breath;
    if(kind==='surf')v=(mid*.40+low*1.1)*Math.pow(.5+.5*Math.sin(tau*u*2),1.7);
    if(kind==='fire')v=low*.70+mid*.14+(r()<.0015?(r()-.5)*.9:0);
    if(kind==='insects')v=Math.sin(t*5300*tau)*.027*Math.pow(.5+.5*Math.sin(t*41),8)*(.30+breath*.7)+Math.sin(t*4200*tau)*.012*Math.pow(.5+.5*Math.sin(t*34),12);
    if(kind==='snow-wind')v=low*1.3+(mid-low)*.12*breath+Math.sin(tau*t*(310+Math.sin(t*.8)*9))*.009*breath;
    if(kind==='thunder'||kind==='thunder-cloud'){
      const cloud=kind==='thunder-cloud';
      // A broad-band electrical crack, then separate expanding pressure rolls.
      // Cloud discharges have a softer attack and more diffuse low-frequency body.
      const crack=((white-mid)*.46+mid*.95)*Math.exp(-t*27)+(mid-low)*.48*Math.exp(-t*9);
      const roll=(.55+.40*Math.sin(t*4.7+pitch)**2+.22*Math.sin(t*9.3)**2);
      const body=(slow*5.8+low*1.9+mid*.12)*roll;
      const pressure=Math.sin(t*48+Math.sin(t*5)*3)*.055+Math.sin(t*83+Math.sin(t*2)*4)*.035;
      const env=(1-Math.exp(-t*(cloud?8:24)))*Math.exp(-t*.55);
      v=crack*(cloud?.12:1.1)+(body+pressure)*env;
    }
    if(bird){
      const tropical=kind==='bird-tropical',meadow=kind==='bird-meadow';
      const pulse=t%(tropical?.24:.31),note=Math.floor(t/(tropical?.24:.31));
      const duration=.09+.045*Math.sin(note*2.7+seed),envelope=Math.pow(Math.max(0,Math.sin(Math.PI*pulse/duration)),2)*(pulse<duration?1:0);
      const hz=(tropical?1350:meadow?2900:2100)*pitch+(tropical?950:1600)*Math.sin(Math.PI*pulse/duration)+Math.sin(t*110)*35;
      phase+=tau*hz/RATE;
      v=(Math.sin(phase)+Math.sin(phase*2)*.12+white*.025)*envelope*.18*(note<5?1:0);
    }
    if(kind==='owl'){const env=Math.pow(Math.max(0,Math.sin(Math.PI*(t%.7)/.5)),2)*((t%.7)<.5?1:0);phase+=tau*(390+Math.sin(t*8)*35)*pitch/RATE;v=(Math.sin(phase)+Math.sin(phase*2)*.12)*env*.21;}
    if(kind==='frog'){const env=Math.pow(.5+.5*Math.sin(t*28),10)*Math.sin(Math.PI*u)**2;phase+=tau*(170+Math.sin(t*30)*35)*pitch/RATE;v=(Math.sin(phase)+Math.sin(phase*3)*.28+mid*.3)*env*.27;}
    if(kind==='gull'){const env=Math.sin(Math.PI*u)**2;phase+=tau*(1100-750*u+90*Math.sin(t*14))*pitch/RATE;v=(Math.sin(phase)+Math.sin(phase*2)*.35+white*.04)*env*.17;}
    if(kind==='crow'){const env=Math.sin(Math.PI*u)**2*Math.pow(.5+.5*Math.sin(t*24),.4);phase+=tau*(560-220*u)*pitch/RATE;v=(Math.sin(phase)+Math.sin(phase*2)*.45+Math.sin(phase*3)*.20+white*.25)*env*.13;}
    if(kind==='creak'){phase+=tau*(130+Math.sin(t*7)*55)/RATE;v=(Math.sin(phase)+Math.sin(phase*2)*.4)*Math.sin(Math.PI*u)**2*.04;}
    // Gentle onset/tail for all one-shots. Loop seams are treated separately.
    if(bird||['thunder','thunder-cloud','owl','frog','crow','creak'].includes(kind))v*=Math.min(t/.006,1,(seconds-t)/.06);
    a[i]=Math.tanh(v*1.35)*.80;
  }
  return a;
}
export function seamless(a){const k=Math.floor(RATE*.25),n=a.length-k,b=new Float32Array(n);for(let i=0;i<n;i++){b[i]=i<k?a[n+i]*(1-i/k)+a[i]*(i/k):a[i];}return b;}
export function wav(a){const b=Buffer.alloc(44+a.length*2);b.write('RIFF');b.writeUInt32LE(b.length-8,4);b.write('WAVEfmt ',8);b.writeUInt32LE(16,16);b.writeUInt16LE(1,20);b.writeUInt16LE(1,22);b.writeUInt32LE(RATE,24);b.writeUInt32LE(RATE*2,28);b.writeUInt16LE(2,32);b.writeUInt16LE(16,34);b.write('data',36);b.writeUInt32LE(a.length*2,40);for(let i=0;i<a.length;i++)b.writeInt16LE(Math.round(Math.max(-1,Math.min(1,a[i]))*32767),44+i*2);return b;}
