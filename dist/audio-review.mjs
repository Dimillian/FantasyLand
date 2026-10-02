import {Soundscape} from './soundscape.mjs?v=footstep-foley-3';
import {mixFor} from './soundscape-model.mjs';
const $=id=>document.getElementById(id),sound=new Soundscape({master:.65}),s={x:0,y:0,z:0,yaw:0,walked:0,speed:4,grounded:true,dayTime:8,biome:'Forest',forest:'Oak woodland',weather:{},audio:{}};
let enabled=false,walking=false,last=performance.now(),analyser=null,signal=null,strike=0,maxPeak=0;
function scene(){sound.cancelThunder();s.biome='Forest';s.forest='Oak woodland';s.dayTime=8;s.weather={mode:1,stormStrength:0,windX:4,windZ:2,gust:.2,rain:0,snow:0,lightning:0};s.audio={forest:.8,indoor:0,water:[24,0,10,0],waterKind:'river',fire:[-3,0,2,0],floor:$('surface').value};
 switch($('scene').value){
 case 'pine':s.forest='Pine forest';s.weather.windX=14;s.weather.gust=.7;break;
 case 'meadow':s.biome='Grassland';s.audio.forest=.08;break;
 case 'marsh':s.biome='Swamp';s.dayTime=23;s.audio.water[3]=.6;s.audio.waterKind='lake';break;
 case 'jungle':s.biome='Tropical rainforest';s.audio.forest=1;s.audio.water[3]=.6;break;
 case 'coast':s.biome='Tropical coast';s.audio.forest=.05;s.audio.water[3]=1;s.audio.waterKind='surf';s.weather.windX=12;break;
 case 'desert':s.biome='Desert';s.audio.forest=0;s.weather.windX=18;break;
 case 'snow':s.biome='Alpine';s.audio.forest=.1;s.weather.windX=28;s.weather.snow=1;s.weather.gust=.9;break;
 case 'storm':s.weather.mode=5;s.weather.stormStrength=1;s.weather.rain=1;s.weather.windX=32;s.weather.gust=1;break;
 case 'inn':s.audio.indoor=1;s.audio.fire[3]=1;s.weather.rain=.8;break;
 }sound.nextBird=0;
}
scene();$('scene').onchange=scene;
$('enable').onclick=()=>{enabled=true;sound.setActive(true);sound.unlock();};
$('mute').onclick=()=>{enabled=false;sound.setActive(false);};
$('walk').onclick=()=>{walking=!walking;$('walk').textContent=`Walk: ${walking?'on':'off'}`;$('walk').setAttribute('aria-pressed',String(walking));};
$('surface').onchange=()=>s.audio.floor=$('surface').value;
$('step').onclick=()=>sound.play(sound.pick(`step-${$('surface').value}`,4),'footsteps',.40,{reverb:s.audio.indoor});
$('call').onclick=()=>{const bird=mixFor(s).bird;if(bird)sound.play(sound.pick(bird),'wildlife',.3,{pan:.4});};
$('thunder').onclick=()=>{s.weather.mode=5;s.weather.stormStrength=1;s.audio.lightningEvent=[++strike,.1,.8,0];s.audio.thunderSource=[1029,0,0];s.weather.lightning=1;setTimeout(()=>s.weather.lightning=0,450);};
$('volume').oninput=()=>{const v=Number($('volume').value);sound.setVolume('master',v/100);$('volume-label').value=`${v}%`;};
document.addEventListener('visibilitychange',()=>sound.setActive(enabled&&!document.hidden));
setInterval(()=>{const now=performance.now(),dt=Math.min(.25,(now-last)/1000);last=now;if(document.hidden)return;
 if(walking){s.walked+=4*dt;s.x+=4*dt;}sound.update(s);
 if(sound.ready&&!analyser){analyser=sound.ctx.createAnalyser();analyser.fftSize=1024;sound.master.connect(analyser);signal=new Float32Array(analyser.fftSize);}
 let peak=0;if(analyser){analyser.getFloatTimeDomainData(signal);for(const x of signal)peak=Math.max(peak,Math.abs(x));maxPeak=Math.max(peak,maxPeak);$('level').value=peak;}
 const d=sound.diagnostics;$('status').textContent=`Audio ${d.status} · ${d.active?'playing':'paused'}`;
 $('telemetry').textContent=`${d.loops} ambience layers · ${d.voices} active short sounds · ${d.pendingThunder} thunder queued\nDecoded audio ${d.decodedMiB.toFixed(1)} MiB · signal peak ${peak.toFixed(3)} · session peak ${maxPeak.toFixed(3)}\nSurface: ${s.audio.floor} · wildlife: ${mixFor(s).bird||'quiet'} · indoor: ${s.audio.indoor? 'yes':'no'}`;
},100);
