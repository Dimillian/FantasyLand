const fs = require('fs');
const vm = require('vm');
const assert = require('assert/strict');
const html = fs.readFileSync(require('path').join(__dirname, '../dist/index.html'), 'utf8');
const adaptiveSource = fs.readFileSync(require('path').join(__dirname, '../dist/adaptive-resolution.js'), 'utf8').replace('export class', 'class');
const portraitSource = fs.readFileSync(require('path').join(__dirname, '../dist/portrait.js'), 'utf8').replace('export function', 'function');
const source = portraitSource + '\n' + adaptiveSource + '\n' + fs.readFileSync(require('path').join(__dirname, '../dist/app.js'), 'utf8').replace(/boot\(\);\s*$/, '').replace(/import \{ AdaptiveResolution \}[^\n]+\n/, '').replace(/import \{ paintPortrait \}[^\n]+\n/, '');
class Element {
  constructor(id='') { this.id=id; this.listeners={}; this.attributes={}; this.style={}; this.classes=new Set(); this.classList={add:(...names)=>names.forEach(name=>this.classes.add(name)),remove:(...names)=>names.forEach(name=>this.classes.delete(name)),toggle:(name,force)=>{const add=force ?? !this.classes.has(name); if(add)this.classes.add(name);else this.classes.delete(name);return add;},contains:name=>this.classes.has(name)}; this.clientWidth=800; this.clientHeight=500; this.width=800; this.height=500; this.tagName='DIV'; this.value=''; this.children=[]; this._text=''; }
  addEventListener(name, callback) { (this.listeners[name] ||= []).push(callback); }
  fire(name, data={}) { const event={target:this, preventDefault(){this.prevented=true;}, ...data}; for(const cb of this.listeners[name]||[]) cb(event); return event; }
  setAttribute(name,value){this.attributes[name]=value;}
  focus(){document.activeElement=this;}
  click(){this.fire('click');}
  get textContent(){return this._text+this.children.map(c=>c.textContent).join('');}
  set textContent(value){this._text=String(value);this.children=[];}
  get firstElementChild(){return this.children[0];}
  remove(){if(this.parent)this.parent.children=this.parent.children.filter(c=>c!==this);}
  removeAttribute(name){delete this.attributes[name];}
  getClientRects(){return this.classList.contains('hidden')?[]:[this.getBoundingClientRect()];}
  querySelectorAll(){return this.controls || this.children;}

  querySelector(){return this.child ||= new Element();}
  getBoundingClientRect(){return {width:800,height:500,left:0,top:0};}
  getContext(){return {};}
  setPointerCapture(){}
  append(...children){for(const child of children){this.children.push(child);child.parent=this;}}
  replaceChildren(...children){this._text='';this.children=children;}
}
const ids = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map((m)=>[m[1],new Element(m[1])]));
ids.world.tagName='CANVAS'; ids['map-canvas'].tagName='CANVAS';
const document = new Element('document');
document.body=new Element('body'); document.getElementById=id=>ids[id]; document.createElement=tag=>{const el=new Element(tag);el.tagName=tag.toUpperCase();return el;};
document.querySelectorAll=selector=>selector==='[data-close]' ? ['map','bag','character','skills','settings'].map(name=>ids[name+'-modal'].querySelector()) : selector==='.overlay' ? ['map','bag','character','skills','settings'].map(name=>ids[name+'-modal']) : [];
document.pointerLockElement=null;
document.exitPointerLock=()=>{document.pointerLockElement=null;document.fire('pointerlockchange');};
const stored={ 'wayfarer.exploration.v3': JSON.stringify({seed:1337,quality:2,sensitivity:1.4,x:900,z:800,waypoint:{x:5,z:8,name:'Old'},atlas:{x:9,z:8,span:700}}) };
let calls=0, looks=[], rejected;
const fakeWeatherCalls=[];
ids.world.requestPointerLock=()=>{calls++;};
const fakeGame={end_dialogue(){},settlement_destinations(){return [];},restore_clock(){},set_weather_mode:value=>fakeWeatherCalls.push(['mode',value]),set_weather_speed:value=>fakeWeatherCalls.push(['speed',value]),set_weather_paused:value=>fakeWeatherCalls.push(['paused',value]),set_reflections:value=>fakeWeatherCalls.push(['reflections',value]),set_enclosure:value=>fakeWeatherCalls.push(['enclosure',value]),look:(x,y)=>looks.push([x,y]),state:()=>({x:100,z:200,stamina:75}),map_data(){return new Uint8Array(320*320*4);},features(){return {};},landscape_destinations(){return [];},set_time(){},set_quality(){},set_ground_cover_density(){},set_meadow(){},set_antialiasing(){},set_shadows(){},set_lighting_mode(){},set_filter(){},set_render_resolution(){},render_resolution:()=>new Uint32Array([800,500]),return_to_spawn(){},teleport(){}};
const context=vm.createContext({document,window:new Element('window'),navigator:{gpu:{}},location:{href:'https://test.invalid/'},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(){},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:k=>stored[k],setItem:(k,v)=>stored[k]=v},fakeGame});
vm.runInContext(source,context);
const run=code=>vm.runInContext(code,context);
const key=code=>document.fire('keydown',{code,target:ids.world});
// Run the real boot/settings code against a GPU boundary stub. This verifies
// renderer calls and reload behavior, not merely the shape of saved fields.
function filterHarness(snapshot, destinations = [], href = 'https://test.invalid/') {
  const filterIds = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map((m) => [m[1], new Element(m[1])]));
  const filterDocument = new Element('document');
  filterDocument.body = new Element('body');
  filterDocument.getElementById = id => filterIds[id];
  filterDocument.createElement = tag => new Element(tag);
  filterDocument.querySelectorAll = selector => selector === '[data-close]' ? ['map', 'bag', 'character', 'skills', 'settings'].map(name => filterIds[name + '-modal'].querySelector()) : selector === '.overlay' ? ['map', 'bag', 'character', 'skills', 'settings'].map(name => filterIds[name + '-modal']) : [];
  const filterStore = { 'wayfarer.exploration.v4': JSON.stringify(snapshot) };
  const filterCalls = [], teleports = [], resolutionCalls = [], qualityCalls = [], groundCoverCalls = [], rendererEvents = [], weatherCalls = [], meadowCalls = [], aaCalls = [], lightingCalls = [];
  let destinationCalls = 0;
  const playerState = {x:snapshot.x,z:snapshot.z,stamina:100,health:100,mana:100};
  let selectedResolution = 0, selectedQuality = 1, surfaceWidth = 800, surfaceHeight = 500;
  const engine = {end_dialogue(){},settlement_destinations(){return [];},restore_clock(){},
    visit_interior:kind=>{rendererEvents.push(['interior',kind]);playerState.x=321;playerState.z=654;return 'The Copper Lantern';},
    // Keep these separate: existing renderer boot-order assertions stay strict.
    set_weather_mode:value=>{weatherCalls.push(['mode',value]);},
    set_weather_speed:value=>{weatherCalls.push(['speed',value]);},
    set_weather_paused:value=>{weatherCalls.push(['paused',value]);},
    set_reflections:value=>{weatherCalls.push(['reflections',value]);},
    set_enclosure:value=>{weatherCalls.push(['enclosure',value]);},
    set_time:hour=>{rendererEvents.push(['time',hour]);playerState.dayTime=hour;},
    landscape_destinations:()=>{destinationCalls++;return typeof destinations === 'function' ? destinations() : destinations;},
    set_shadows:value=>{rendererEvents.push(['shadows',value]);},
    set_lighting_mode:value=>{lightingCalls.push(value);rendererEvents.push(['lighting',value]);},
    set_antialiasing:value=>{aaCalls.push(value);},
    set_meadow:value=>{meadowCalls.push(value);},
    set_ground_cover_density:value=>{groundCoverCalls.push(value);rendererEvents.push(['groundCover',value]);},
    set_filter:(mode,strength)=>{filterCalls.push([mode,strength]);rendererEvents.push(['filter',mode,strength]);},
    set_render_resolution:height=>{selectedResolution=height;resolutionCalls.push(height);rendererEvents.push(['resolution',height]);},
    render_resolution:()=>{const height=selectedResolution===1 ? surfaceHeight : selectedResolution || [240,360,450][selectedQuality];return new Uint32Array([Math.round(surfaceWidth/surfaceHeight*height),height]);},
    world_size:()=>384000,
    set_quality:value=>{selectedQuality=value;qualityCalls.push(value);rendererEvents.push(['quality',value]);},
    resize:(width,height)=>{surfaceWidth=width;surfaceHeight=height;rendererEvents.push(['resize',width,height]);},
    teleport:(x,z)=>{teleports.push([x,z]);playerState.x=x;playerState.z=z;},
    face:(yaw,pitch)=>{playerState.yaw=yaw;playerState.pitch=pitch;},
    state:()=>({...playerState})
  };
  let readyFrames = 0;
  const filterContext = vm.createContext({document:filterDocument,window:new Element('window'),navigator:{gpu:{}},location:{href},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(callback){if (++readyFrames <= 2) queueMicrotask(()=>callback(0));},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:key=>filterStore[key],setItem:(key,value)=>filterStore[key]=value},fakeModule:{default:async()=>{},Game:{create:async()=>engine}}});
  const bootSource = source.replace(/await import\('\.\/pkg\/fantasy_land\.js(?:\?[^']*)?'\)/, 'fakeModule');
  vm.runInContext(bootSource,filterContext);
  return {ids:filterIds,get destinationCalls(){return destinationCalls;},calls:filterCalls,teleports,resolutionCalls,qualityCalls,groundCoverCalls,rendererEvents,weatherCalls,meadowCalls,aaCalls,lightingCalls,run:code=>vm.runInContext(code,filterContext),saved:()=>JSON.parse(filterStore['wayfarer.exploration.v4'])};
}

async function verifyInteriorTours() {
  const h=filterHarness({seed:42,x:12,z:34});await h.run('boot()');h.run('initialReady=true;');
  h.ids['study-select'].value='interior:inn';h.ids['study-form'].fire('submit');
  assert.deepEqual(h.rendererEvents.filter(e=>e[0]==='interior'),[['interior','inn']]);
  assert.equal(h.saved().x,321);assert.equal(h.saved().z,654);
  assert.equal(h.run('modal'),null);
  console.log('PASS: interior tours use the active world seed, visit the engine room and save the new position.');
}

async function verifyFilterSettings() {
  const existing = {seed:1337,x:637,z:222,quality:2,sensitivity:1.2,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
  const first = filterHarness(existing);
  assert.equal(first.run('filterMode'),1); assert.equal(first.run('filterStrength'),1);
  assert.equal(first.ids['filter-select'].value,'1'); assert.equal(first.ids['filter-strength'].value,'100');
  assert.equal(first.ids['filter-strength'].disabled,false);
  assert.equal(first.run('renderResolution'),1);
  assert.equal(first.ids['render-resolution'].value,'1');
  await first.run('boot()');
  assert.deepEqual(first.calls,[[1,1]],'Boot must apply Bloom at 100% to the renderer.');
  assert.deepEqual(first.resolutionCalls,[1]);
  assert.equal(first.ids['render-dimensions'].textContent,'Actual 800 × 500');
  assert.deepEqual(first.rendererEvents.map(event=>event[0]),['quality','resolution','filter','groundCover','shadows','lighting','resize'],'Preferences must apply after world quality and before resize.');
  assert.deepEqual(first.teleports,[[637,222]],'Existing v4 position must survive adding filters.');
  first.run('initialReady=true;');
  first.ids['filter-select'].value='2'; first.ids['filter-select'].fire('change');
  assert.deepEqual(first.calls.at(-1),[2,1]);
  first.ids['filter-strength'].value='135'; first.ids['filter-strength'].fire('input');
  assert.deepEqual(first.calls.at(-1),[2,1.35]);
  const persisted=first.saved();
  assert.equal(persisted.filterMode,2); assert.equal(persisted.filterStrength,1.35);
  assert.equal(persisted.x,existing.x); assert.equal(persisted.z,existing.z);
  assert.deepEqual(persisted.waypoint,existing.waypoint); assert.deepEqual(persisted.atlas,existing.atlas);
  const restored=filterHarness(persisted); await restored.run('boot()'); restored.run('initialReady=true;');
  assert.deepEqual(restored.calls,[[2,1.35]],'Saved mode and amount must reach the renderer on reload.');
  assert.equal(restored.ids['filter-strength'].value,'135');
  restored.ids['filter-select'].value='0'; restored.ids['filter-select'].fire('change');
  assert.deepEqual(restored.calls.at(-1),[0,1.35]); assert.equal(restored.ids['filter-strength'].disabled,true);
  const before=restored.calls.length;
  restored.ids['filter-strength'].value='50'; restored.ids['filter-strength'].fire('input');
  assert.equal(restored.calls.length,before,'Clean must ignore changes to its disabled amount.');
  assert.equal(restored.saved().filterStrength,1.35);
  restored.ids['filter-select'].value='1'; restored.ids['filter-select'].fire('change');
  assert.equal(restored.ids['filter-strength'].disabled,false); assert.deepEqual(restored.calls.at(-1),[1,1.35]);
  restored.ids['filter-strength'].value='0'; restored.ids['filter-strength'].fire('input');
  assert.deepEqual(restored.calls.at(-1),[1,0]); assert.equal(restored.saved().filterStrength,0);
  const zero=filterHarness(restored.saved()); await zero.run('boot()'); assert.deepEqual(zero.calls,[[1,0]],'Zero must survive reload instead of becoming the default.');
  restored.ids['filter-strength'].value='200'; restored.ids['filter-strength'].fire('input');
  assert.deepEqual(restored.calls.at(-1),[1,1.5]); assert.equal(restored.ids['filter-strength'].value,'150');
  assert.deepEqual(first.meadowCalls,[true]);
  first.ids['meadow-carpet'].value='off'; first.ids['meadow-carpet'].fire('change');
  assert.equal(first.meadowCalls.at(-1),false);
  const meadowReload=filterHarness(first.saved()); await meadowReload.run('boot()');
  assert.deepEqual(meadowReload.meadowCalls,[false]);
  assert.deepEqual(first.aaCalls,[1]);
  first.ids.antialiasing.value='2'; first.ids.antialiasing.fire('change');
  assert.equal(first.aaCalls.at(-1),2);
  const aaReload=filterHarness(first.saved()); await aaReload.run('boot()');
  assert.deepEqual(aaReload.aaCalls,[2]);
  first.ids['render-resolution'].value='adaptive'; first.ids['render-resolution'].fire('change');
  assert.equal(first.resolutionCalls.at(-1),540); assert.equal(first.ids['render-resolution'].value,'adaptive');
  const adaptiveReload=filterHarness(first.saved()); await adaptiveReload.run('boot()');
  assert.equal(adaptiveReload.resolutionCalls.at(-1),540);
  first.ids['render-resolution'].value='1'; first.ids['render-resolution'].fire('change');
  assert.equal(first.resolutionCalls.at(-1),1); assert.equal(first.saved().adaptiveResolution,false);
  assert.deepEqual(first.teleports,[[637,222]],'AA and adaptive settings must not move the player.');
  const comparison=filterHarness({...existing, antialiasing:2, renderResolution:540, adaptiveResolution:true});
  await comparison.run('boot()');comparison.run('initialReady=true; started=true; game.face(0,-0.1); beginBenchmark(false,true);');
  let time=0;
  for(let i=0;i<6;i++) {
    time+=3100;comparison.run(`updateBenchmark(${time},16.667);`);
    time+=15100;comparison.run(`updateBenchmark(${time},16.667);`);
  }
  const reports=JSON.parse(comparison.ids['benchmark-result'].attributes['data-report']);
  assert.deepEqual(reports.map(r=>[r.antialiasing,r.height]),[[0,420],[0,720],[1,420],[1,720],[2,420],[2,720]]);
  assert.equal(new Set(reports.map(r=>r.hour)).size,1,'All six passes must hold the same daylight.');
  assert.equal(comparison.aaCalls.at(-1),2,'AA comparison must restore the selected mode.');
  assert.equal(comparison.resolutionCalls.at(-1),540,'AA comparison must restore the adaptive target.');
  assert.ok(reports.every(r=>r.adaptiveSuspended),'Fixed-resolution benchmarks suspend adaptation.');
  comparison.run(`
    var recorderStopped=0, trackStopped=0;
    var oldRecorder;
    MediaRecorder=class {
      static isTypeSupported(){return true;}
      constructor(){oldRecorder=this;this.state='inactive';this.mimeType='video/mp4';}
      start(){this.state='recording';}
      stop(){recorderStopped++;this.state='inactive';}
    };
    canvas.captureStream=()=>({getTracks:()=>[{stop(){trackStopped++;}}]});
    game.set_time(8); game.teleport(637,222); state=game.state(); closeModal();
  `);
  comparison.ids['motion-record'].fire('click');
  comparison.run("game.teleport(900,800); state=game.state(); motionCapture.recorder.start(); openModal('bag'); saveProgress(); oldRecorder.onstop();");
  assert.equal(comparison.run('motionCapture'),null);
  assert.equal(comparison.run('recorderStopped'),1); assert.equal(comparison.run('trackStopped'),1);
  assert.equal(comparison.run('modal'),'bag','Opening a menu cancels automated walking first.');
  assert.equal(comparison.saved().x,637); assert.equal(comparison.saved().z,222,'Cancel must save the restored position.');
  assert.equal(comparison.ids['motion-status'].textContent,'Recording interrupted.','Stale async completion must not advertise an 8-second clip.');
  console.log('PASS: Bloom defaults; renderer initialization; filter selection and amount API calls; v4 position/waypoint/atlas preservation; reload persistence; Clean disabled state; zero strength; maximum clamp.');
}

async function verifyResolutionSettings() {
  const existing = {seed:1337,x:810,z:-160,quality:2,sensitivity:1,filterMode:2,filterStrength:0.8,renderResolution:360,waypoint:{x:900,z:-210,name:'The Pass'},atlas:{x:850,z:-180,span:8000}};
  const selected = filterHarness(existing); await selected.run('boot()'); selected.run('initialReady=true;');
  assert.deepEqual(selected.resolutionCalls,[360]);
  assert.equal(selected.ids['render-dimensions'].textContent,'Actual 576 × 360');
  selected.ids['render-resolution'].value='720'; selected.ids['render-resolution'].fire('change');
  assert.equal(selected.resolutionCalls.at(-1),720); assert.equal(selected.ids['render-dimensions'].textContent,'Actual 1152 × 720');
  const resolutionCallCount=selected.resolutionCalls.length;
  selected.ids['quality-select'].value='0'; selected.ids['quality-select'].fire('change');
  assert.equal(selected.qualityCalls.at(-1),0); assert.equal(selected.resolutionCalls.length,resolutionCallCount,'Changing world quality must not overwrite an explicit resolution.');
  assert.equal(selected.ids['render-dimensions'].textContent,'Actual 1152 × 720');
  const persisted=selected.saved();
  assert.equal(persisted.renderResolution,720); assert.equal(persisted.filterMode,2);
  assert.equal(persisted.x,existing.x); assert.equal(persisted.z,existing.z); assert.deepEqual(persisted.waypoint,existing.waypoint); assert.deepEqual(persisted.atlas,existing.atlas);
  const restored=filterHarness(persisted); await restored.run('boot()'); restored.run('initialReady=true;');
  assert.deepEqual(restored.resolutionCalls,[720]); assert.deepEqual(restored.calls,[[2,0.8]]);
  restored.ids['render-resolution'].value='1'; restored.ids['render-resolution'].fire('change');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 800 × 500');
  restored.ids.world.clientWidth=900;restored.ids.world.clientHeight=600;restored.run('resize()');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 900 × 600','Native actual dimensions must follow viewport resize.');
  restored.ids['render-resolution'].value='0'; restored.ids['render-resolution'].fire('change');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 360 × 240');
  restored.ids['quality-select'].value='1'; restored.ids['quality-select'].fire('change');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 540 × 360','Auto must follow world quality.');
  const invalid=filterHarness({...existing,renderResolution:999});
  assert.equal(invalid.run('renderResolution'),1);
  // Restoring the visual baseline must never restart the world or reset controls.
  const custom = filterHarness({...existing, groundCoverDensity:1, meadowCarpet:false, antialiasing:2, sunShadows:false, lightingMode:7, reflections:false, enclosure:false, adaptiveResolution:true, sensitivity:1.45, weatherMode:5, weatherSpeed:3, weatherPaused:true});
  await custom.run('boot()'); custom.run('initialReady=true;');
  assert.equal(custom.ids['look-status'].textContent,'Custom look');
  custom.ids['restore-visual-defaults'].fire('click');
  const baseline = custom.saved();
  for (const [key,value] of Object.entries({quality:1,antialiasing:1,filterMode:1,filterStrength:1,groundCoverDensity:4,meadowCarpet:true,sunShadows:true,lightingMode:7,reflections:true,enclosure:true,renderResolution:1,adaptiveResolution:false})) assert.equal(baseline[key],value,`Default ${key}`);
  for (const key of ['seed','x','z','waypoint','atlas']) assert.deepEqual(baseline[key],existing[key],`Restore defaults must preserve ${key}`);
  assert.equal(baseline.sensitivity,1.45); assert.equal(baseline.weatherMode,5); assert.equal(baseline.weatherSpeed,3); assert.equal(baseline.weatherPaused,true);
  assert.deepEqual(custom.teleports,[[810,-160]],'Restoring graphics never teleports the player.');
  assert.equal(custom.resolutionCalls.at(-1),1); assert.equal(custom.qualityCalls.at(-1),1);
  assert.equal(custom.groundCoverCalls.at(-1),4); assert.equal(custom.aaCalls.at(-1),1);
  assert.deepEqual(custom.calls.at(-1),[1,1]); assert.equal(custom.meadowCalls.at(-1),true);
  assert.equal(custom.lightingCalls.at(-1),7); assert.equal(custom.ids['lighting-mode'].value,'7');
  assert.equal(custom.ids['look-status'].textContent,'Recommended look');
  const baselineReload=filterHarness(baseline); await baselineReload.run('boot()');
  assert.equal(baselineReload.resolutionCalls.at(-1),1); assert.equal(baselineReload.ids['look-status'].textContent,'Recommended look');
  console.log('PASS: resolution defaults/boot ordering; explicit resolution independent of quality; Native resize; Auto quality; actual-size labels; v4 persistence.');
}

async function verifyLightingSettings() {
  const existing = {seed:1337,x:637,z:222,quality:1,sensitivity:1.2,antialiasing:2,filterMode:2,filterStrength:0.8,renderResolution:420,groundCoverDensity:2.25,meadowCarpet:false,sunShadows:false,reflections:false,enclosure:false,weatherMode:5,weatherSpeed:3,weatherPaused:true,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
  const selected = filterHarness(existing); await selected.run('boot()'); selected.run('initialReady=true;');
  assert.deepEqual(selected.lightingCalls,[7],'An older save without a lighting choice must use Full indirect at boot.');
  assert.equal(selected.ids['lighting-mode'].value,'7');
  assert.ok(selected.rendererEvents.findIndex(e=>e[0]==='lighting') < selected.rendererEvents.findIndex(e=>e[0]==='resize'),'Lighting must reach the renderer before first resize.');
  const unrelatedBefore = JSON.stringify({weather:selected.weatherCalls,aa:selected.aaCalls,meadow:selected.meadowCalls,teleports:selected.teleports});
  for (const mode of [1,3,7,0]) {
    const eventsBefore = selected.rendererEvents.length;
    selected.ids['lighting-mode'].value=String(mode); selected.ids['lighting-mode'].fire('change');
    assert.deepEqual(selected.rendererEvents.slice(eventsBefore),[['lighting',mode]],'Selecting lighting must only update its renderer preference.');
    const persisted=selected.saved();
    assert.equal(persisted.lightingMode,mode);
    for(const key of Object.keys(existing)) assert.deepEqual(persisted[key],existing[key],`Lighting must preserve ${key}.`);
    const restored=filterHarness(persisted); await restored.run('boot()');
    assert.deepEqual(restored.lightingCalls,[mode],`Saved lighting ${mode} must reach the renderer on reload.`);
    assert.equal(restored.ids['lighting-mode'].value,String(mode));
    assert.equal(JSON.stringify({weather:selected.weatherCalls,aa:selected.aaCalls,meadow:selected.meadowCalls,teleports:selected.teleports}),unrelatedBefore,'Lighting must preserve weather, AA, cover and location.');
  }
  for (const value of [2,4,8,-1,'invalid']) {
    const invalid=filterHarness({...existing,lightingMode:value}); await invalid.run('boot()');
    assert.deepEqual(invalid.lightingCalls,[7]); assert.equal(invalid.ids['lighting-mode'].value,'7');
  }
  const query=filterHarness({...existing,lightingMode:1},[],'https://test.invalid/?lighting=7'); await query.run('boot()'); query.run('initialReady=true;saveProgress();');
  assert.deepEqual(query.lightingCalls,[7],'The explicit local review URL must override the saved lighting mode.');
  assert.equal(query.saved().lightingMode,7);
  const invalidQuery=filterHarness({...existing,lightingMode:7},[],'https://test.invalid/?lighting=2'); await invalidQuery.run('boot()');
  assert.deepEqual(invalidQuery.lightingCalls,[7],'Unsupported review modes must fall back to Full indirect.');
  const recommended=filterHarness({seed:1337,x:637,z:222,lightingMode:7}); await recommended.run('boot()'); recommended.run('initialReady=true;');
  assert.equal(recommended.ids['look-status'].textContent,'Recommended look','Full indirect lighting is the chosen default.');
  recommended.ids['restore-visual-defaults'].fire('click');
  assert.equal(recommended.lightingCalls.at(-1),7); assert.equal(recommended.saved().lightingMode,7); assert.equal(recommended.ids['lighting-mode'].value,'7');
  assert.equal(recommended.ids['look-status'].textContent,'Recommended look');
  console.log('PASS: Full indirect lighting defaults; all lighting modes apply immediately and survive reload; unrelated preferences/progress retained; review URL override; invalid-mode fallback; baseline reset and summary.');
}

async function verifyRemovedFilterMigration() {
  const existing = {seed:1337,x:810,z:-160,quality:2,sensitivity:1.2,filterMode:3,filterStrength:0,asciiScale:3,asciiPalette:1,renderResolution:360,groundCoverDensity:2.25,sunShadows:false,weatherMode:5,weatherSpeed:3,weatherPaused:true,reflections:false,enclosure:false,waypoint:{x:900,z:-210,name:'The Pass'},atlas:{x:850,z:-180,span:8000}};
  const selected=filterHarness(existing); await selected.run('boot()'); selected.run('initialReady=true;saveProgress();');
  assert.deepEqual(selected.calls,[[1,1]],'The removed filter must migrate to visible Bloom at full strength.');
  assert.equal(selected.ids['filter-select'].value,'1'); assert.equal(selected.ids['filter-strength'].value,'100');
  assert.equal(selected.ids['filter-strength'].disabled,false);
  const persisted=selected.saved();
  assert.equal(persisted.filterMode,1); assert.equal(persisted.filterStrength,1);
  assert.ok(!('asciiScale' in persisted)); assert.ok(!('asciiPalette' in persisted));
  for(const key of Object.keys(existing).filter(key=>!['filterMode','filterStrength','asciiScale','asciiPalette'].includes(key))) assert.deepEqual(persisted[key],existing[key],`Filter migration must preserve ${key}.`);
  const restored=filterHarness(persisted); await restored.run('boot()');
  assert.deepEqual(restored.calls,[[1,1]],'Migrated Bloom must survive reload.');
  const filterOptions=html.match(/<select id="filter-select"[^>]*>([\s\S]*?)<\/select>/)?.[1];
  assert.deepEqual([...filterOptions.matchAll(/<option value="(\d+)"/g)].map(match=>Number(match[1])).sort(),[0,1,2]);
  for (const invalid of [3,-1,99,'invalid']) {
    selected.ids['filter-select'].value=String(invalid); selected.ids['filter-select'].fire('change');
    assert.deepEqual(selected.calls.at(-1),[1,1],'Unsupported filter modes must never reach the renderer.');
  }
  console.log('PASS: removed-filter migration to visible Bloom; obsolete preferences discarded; player, map and other settings retained; valid filter options only; migrated reload.');
}

async function verifyGroundCoverSettings() {
  const existing = {seed:1337,x:637,z:222,quality:2,sensitivity:1.2,filterMode:2,filterStrength:0.8,renderResolution:720,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
  const controls = html.match(/<input id="ground-cover-density"[^>]+>/)?.[0];
  assert.ok(controls, 'Ground-cover control must exist in the actual settings HTML.');
  for (const attr of ['type="range"','min="0"','max="400"','step="25"','value="400"','aria-describedby="ground-cover-density-help"']) assert.ok(controls.includes(attr), attr);
  const selected = filterHarness(existing);
  assert.equal(selected.run('groundCoverDensity'),4, 'Old v4 saves without a density preference receive the 400% default.');
  assert.equal(selected.ids['ground-cover-density'].value,'400');
  assert.equal(selected.ids['ground-cover-density-value'].textContent,'400% · 4×');
  await selected.run('boot()'); selected.run('initialReady=true;');
  assert.deepEqual(selected.groundCoverCalls,[4]);
  assert.ok(selected.rendererEvents.findIndex(e=>e[0]==='groundCover') < selected.rendererEvents.findIndex(e=>e[0]==='resize'), 'Density must reach the renderer before first resize.');
  const eventsBefore = selected.rendererEvents.length;
  const change = value => { selected.ids['ground-cover-density'].value=String(value); selected.ids['ground-cover-density'].fire('input'); };
  change(225);
  assert.deepEqual(selected.groundCoverCalls,[4,2.25], 'An input event must call the GPU API immediately.');
  assert.deepEqual(selected.rendererEvents.slice(eventsBefore),[['groundCover',2.25]], 'Density must not resize, alter quality, or rebuild through unrelated APIs.');
  assert.deepEqual(selected.teleports,[[637,222]], 'Live density changes must never teleport/reset the player.');
  assert.equal(selected.ids['ground-cover-density-value'].textContent,'225% · 2.25×');
  assert.equal(selected.ids['ground-cover-density'].attributes['aria-valuetext'],'225 percent, 2.25 times density');
  const persisted = selected.saved();
  for (const key of Object.keys(existing)) assert.deepEqual(persisted[key],existing[key], `Density must preserve existing ${key}.`);
  assert.equal(persisted.groundCoverDensity,2.25);
  const restored = filterHarness(persisted); await restored.run('boot()'); restored.run('initialReady=true;');
  assert.deepEqual(restored.groundCoverCalls,[2.25]);
  restored.ids['ground-cover-density'].value='0'; restored.ids['ground-cover-density'].fire('input');
  assert.equal(restored.groundCoverCalls.at(-1),0); assert.equal(restored.saved().groundCoverDensity,0);
  assert.equal(restored.ids['ground-cover-density-value'].textContent,'Off');
  assert.equal(restored.ids['ground-cover-density'].attributes['aria-valuetext'],'Off');
  const off = filterHarness(restored.saved()); await off.run('boot()');
  assert.deepEqual(off.groundCoverCalls,[0], 'Off must survive reload.');
  const count=selected.groundCoverCalls.length;
  selected.ids['quality-select'].value='0'; selected.ids['quality-select'].fire('change');
  assert.equal(selected.groundCoverCalls.length,count); assert.equal(selected.saved().groundCoverDensity,2.25, 'World quality must not replace explicit density.');
  for (const [value, expected] of [[-25,0],[650,4],['not a number',4],['Infinity',4]]) { change(value); assert.equal(selected.groundCoverCalls.at(-1),expected); assert.equal(selected.saved().groundCoverDensity,expected); assert.equal(selected.ids['ground-cover-density'].value,String(expected*100)); }
  change(400); assert.equal(selected.ids['ground-cover-density-value'].textContent,'400% · 4×');
  for (const [value, expected] of [[-3,0],[8,4],['invalid',4],['Infinity',4],[null,4]]) {
    const invalid=filterHarness({...existing,groundCoverDensity:value}); await invalid.run('boot()');
    assert.deepEqual(invalid.groundCoverCalls,[expected], 'Saved values must be finite and bounded before reaching WASM.');
  }
  assert.equal(selected.ids['sun-shadows'].value,'on');
  selected.ids['sun-shadows'].value='off'; selected.ids['sun-shadows'].fire('change');
  assert.deepEqual(selected.rendererEvents.at(-1),['shadows',false]);
  assert.equal(selected.saved().sunShadows,false);
  const shadowOff=filterHarness(selected.saved()); await shadowOff.run('boot()');
  assert.equal(shadowOff.ids['sun-shadows'].value,'off');
  assert.ok(shadowOff.rendererEvents.some(e=>e[0]==='shadows' && e[1]===false));
  console.log('PASS: accessible ground-cover slider; old-save default; immediate GPU input calls; boot order; 0/Off and reload persistence; multiplier labels; finite validation/clamping; independence of quality; all existing v4 progress and preferences preserved.');
}

async function verifyWeatherSettings() {
  const existing = {seed:1337,x:637,z:222,quality:2,sensitivity:1.2,filterMode:1,filterStrength:.8,renderResolution:720,groundCoverDensity:4,sunShadows:false,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
  const slider = html.match(/<input id="weather-speed"[^>]+>/)?.[0];
  assert.ok(slider,'The weather speed slider must exist in the actual HTML.');
  for (const attr of ['type="range"','min="0.25"','max="20"','step="0.25"','value="1"','aria-describedby="weather-speed-help"']) assert.ok(slider.includes(attr),attr);
  const modeSelect = html.match(/<select id="weather-mode"[^>]*>([\s\S]*?)<\/select>/)?.[1];
  assert.ok(modeSelect);
  const modeOptions = Object.fromEntries([...modeSelect.matchAll(/<option value="(\d+)"[^>]*>([^<]+)<\/option>/g)].map(match=>[match[1],match[2]]));
  assert.deepEqual(modeOptions,{0:'Automatic',1:'Clear',2:'Cloudy',3:'Rain',4:'Storm',5:'Tempest',6:'Snow',7:'Blizzard',8:'Overcast'},'Every visible mode must use the WASM mode number.');
  const h=filterHarness(existing); await h.run('boot()'); h.run('initialReady=true;');
  assert.equal(h.run('fatal'),false,'Boot must support the five environment APIs.');
  assert.deepEqual(h.weatherCalls,[['mode',0],['speed',1],['paused',false],['reflections',true],['enclosure',true]],'Old v4 saves receive the environment defaults.');
  assert.equal(h.ids['weather-mode'].value,'0'); assert.equal(h.ids['weather-speed'].value,'1');
  assert.equal(h.ids['weather-paused'].value,'running'); assert.equal(h.ids['water-reflections'].value,'on'); assert.equal(h.ids['enclosure-shading'].value,'on');
  const rendererBefore=JSON.stringify(h.rendererEvents), teleportBefore=JSON.stringify(h.teleports);
  const change=(id,value,event='change')=>{h.ids[id].value=String(value);h.ids[id].fire(event);};
  for (let mode=0;mode<=8;mode++) {
    change('weather-mode',mode); assert.deepEqual(h.weatherCalls.at(-1),['mode',mode]); assert.equal(h.saved().weatherMode,mode);
  }
  assert.match(h.ids['weather-mode-help'].textContent,/gradual override/);
  change('weather-speed',.25,'input'); assert.deepEqual(h.weatherCalls.at(-1),['speed',.25]);
  assert.equal(h.ids['weather-speed-value'].textContent,'0.25×'); assert.equal(h.ids['weather-speed'].attributes['aria-valuetext'],'0.25 times speed');
  change('weather-speed',20,'input'); change('weather-paused','paused');
  assert.deepEqual(h.weatherCalls.at(-1),['paused',true]); assert.equal(h.ids['weather-speed-value'].textContent,'20× · paused');
  change('water-reflections','off'); assert.deepEqual(h.weatherCalls.at(-1),['reflections',false]);
  change('enclosure-shading','off'); assert.deepEqual(h.weatherCalls.at(-1),['enclosure',false]);
  assert.equal(JSON.stringify(h.rendererEvents),rendererBefore,'Weather controls must not change quality, filters, shadows, time, cover, or size.');
  assert.equal(JSON.stringify(h.teleports),teleportBefore,'Weather controls must not move the player.');
  const persisted=h.saved();
  for(const key of Object.keys(existing)) assert.deepEqual(persisted[key],existing[key],`Weather must preserve existing ${key}.`);
  assert.equal(persisted.weatherMode,8);assert.equal(persisted.weatherSpeed,20);assert.equal(persisted.weatherPaused,true);assert.equal(persisted.reflections,false);assert.equal(persisted.enclosure,false);
  const restored=filterHarness(persisted); await restored.run('boot()');restored.run('initialReady=true;');
  assert.deepEqual(restored.weatherCalls,[['mode',8],['speed',20],['paused',true],['reflections',false],['enclosure',false]],'Saved weather and false toggles must reach WASM on reload.');
  assert.equal(restored.ids['weather-paused'].value,'paused');assert.equal(restored.ids['water-reflections'].value,'off');assert.equal(restored.ids['enclosure-shading'].value,'off');
  for(const [id,value,expected] of [['weather-paused','running',['paused',false]],['water-reflections','on',['reflections',true]],['enclosure-shading','on',['enclosure',true]]]) {
    restored.ids[id].value=value;restored.ids[id].fire('change');assert.deepEqual(restored.weatherCalls.at(-1),expected);
  }
  for(const [value,expected] of [[-3,.25],[0,.25],[99,20],['invalid',1],['Infinity',1],['-Infinity',1]]) {
    change('weather-speed',value,'input');assert.deepEqual(h.weatherCalls.at(-1),['speed',expected]);assert.equal(h.ids['weather-speed'].value,String(expected));assert.equal(h.saved().weatherSpeed,expected);
  }
  for(const value of [-1,9,1.5,'invalid','Infinity']) {
    change('weather-mode',value);assert.deepEqual(h.weatherCalls.at(-1),['mode',0]);assert.equal(h.ids['weather-mode'].value,'0');assert.equal(h.saved().weatherMode,0);
  }
  assert.match(h.ids['weather-mode-help'].textContent,/local climate/);
  for(const [value,expected] of [[-3,.25],[50,20],['invalid',1],['Infinity',1],[null,1]]) {
    const invalid=filterHarness({...existing,weatherSpeed:value});await invalid.run('boot()');
    assert.ok(invalid.weatherCalls.some(call=>call[0]==='speed' && call[1]===expected),'Saved speeds must be finite and bounded before WASM.');
  }
  for(const value of [-1,9,1.5,'invalid','Infinity',null]) {
    const invalid=filterHarness({...existing,weatherMode:value});await invalid.run('boot()');assert.deepEqual(invalid.weatherCalls[0],['mode',0]);
  }
  h.ids['compass-track'].parentElement={clientWidth:320};
  h.run("modal='settings';state.dayTime=9;state.weather={label:'Blizzard',modeLabel:'Automatic',temperature:-5.7,windX:22,windZ:4,cloudCover:.9,rain:0,snow:.82,wetness:.6,snowCover:.5};state.reflectionDraws=12;updateHUD(100);");
  assert.equal(h.ids['weather-hud'].textContent,'Blizzard · -6°C');
  assert.match(h.ids['weather-current'].textContent,/Blizzard · -6°C · Automatic/);
  assert.match(h.ids['weather-air'].textContent,/Wind fierce · cloud 90% · rain 0% · snow 82%/);
  assert.equal(h.ids['weather-ground'].textContent,'Ground wetness 60% · snow cover 50%');
  assert.match(h.ids.diagnostics.textContent,/12 reflection draws/);
  h.run("state.weather={label:'Fair',temperature:NaN,windX:Infinity,windZ:0,cloudCover:5,rain:-2};updateHUD(200);");
  assert.doesNotMatch(h.ids['weather-hud'].textContent+h.ids['weather-current'].textContent+h.ids['weather-air'].textContent+h.ids['weather-ground'].textContent,/NaN|Infinity|undefined/);
  assert.match(h.ids['weather-air'].textContent,/cloud 100% · rain 0% · snow —/);
  h.run('state.weather=undefined;updateHUD(300);'); // Early or incomplete engine state must not break the HUD.
  console.log('PASS: weather API boot/defaults; all nine modes; live speed/pause/reflection/enclosure calls; separate renderer events; finite bounds; v4 progress and visual preferences; reload including false toggles; live weather HUD and reflection diagnostics.');
}

async function verifyLandscapeDestinations() {
  const existing={seed:1337,x:637,z:222,quality:2,sunShadows:false,groundCoverDensity:4,sensitivity:1.2,filterMode:2,filterStrength:1.1,renderResolution:720,atlas:{x:640,z:225,span:6000}};
  const names=['Ancient woodland','Granite highlands','Windswept coast','Wet lowlands','Sandstone country','Meadowlands','Alpine heights'];
  const destinations=names.map((name,i)=>({name,x:1000+i*320,z:-1000-i*450,yaw:i*0.3,pitch:-0.04}));
  const h=filterHarness(existing,destinations); await h.run('boot()');h.run('initialReady=true;');
  assert.equal(h.destinationCalls,0,'Boot must not scan destinations.');
  h.run("openModal('settings')");assert.equal(h.destinationCalls,0,'Opening settings must not scan until the selector is used.');
  assert.equal(h.run('selectedLandscapeDestination()'),null);
  h.ids['landscape-destination'].fire('focus');assert.equal(h.destinationCalls,1);
  assert.equal(h.ids['landscape-destination'].children.length,7);
  assert.deepEqual(h.ids['landscape-destination'].children.map(option=>option.textContent),names);
  assert.equal(h.ids['travel-landscape'].disabled,true);
  h.ids['landscape-destination'].fire('pointerdown');assert.equal(h.destinationCalls,1,'The world destination list must be cached.');
  h.ids['landscape-travel-form'].fire('submit');assert.deepEqual(h.teleports,[[637,222]],'The placeholder must never teleport.');
  h.ids['landscape-destination'].value='2';h.ids['landscape-destination'].fire('change');assert.equal(h.ids['travel-landscape'].disabled,false);
  h.ids['landscape-travel-form'].fire('submit');assert.deepEqual(h.teleports.at(-1),[1640,-1900]);assert.equal(h.run('state.yaw'),0.6);assert.equal(h.run('state.pitch'),-0.04);
  assert.equal(h.run('modal'),null);assert.equal(h.ids.toast.textContent,'Arrived at Windswept coast.');
  const saved=h.saved();assert.equal(saved.x,1640);assert.equal(saved.z,-1900);assert.equal(saved.waypoint.name,'Windswept coast');
  for(const key of Object.keys(existing).filter(key=>!['x','z'].includes(key)))assert.deepEqual(saved[key],existing[key],`Travel must preserve ${key}.`);
  h.ids['landscape-destination'].value='99';h.ids['landscape-destination'].fire('change');assert.equal(h.ids['travel-landscape'].disabled,true);
  h.ids['landscape-travel-form'].fire('submit');assert.equal(h.teleports.length,2);
  const unsafe=filterHarness(existing,[{name:'Valid <landscape>',x:1,z:2},{name:'Bad',x:Infinity,z:0},{name:'Outside',x:900000,z:0},{name:'',x:1,z:2},null]);
  await unsafe.run('boot()');unsafe.run('initialReady=true;');unsafe.ids['landscape-destination'].fire('focus');
  assert.equal(unsafe.ids['landscape-destination'].children.length,1);assert.equal(unsafe.ids['landscape-destination'].children[0].textContent,'Valid <landscape>','Names must be text, never HTML.');
  console.log('PASS: lazy cached landscape destinations; seven generated entries; explicit selection/travel; actual coordinates and saved waypoint; empty/invalid selection safety; destination validation; preservation of sun shadows and all visual/atlas settings.');
}

function verifyAtlasRoutes() {
  const strokes = [];
  let path = [], dash = [7, 3], savedStyle;
  const pen = {
    lineWidth:9, strokeStyle:'original', lineCap:'butt', lineJoin:'miter',
    save(){savedStyle={width:this.lineWidth,color:this.strokeStyle,cap:this.lineCap,join:this.lineJoin,dash:[...dash]};},
    restore(){this.lineWidth=savedStyle.width;this.strokeStyle=savedStyle.color;this.lineCap=savedStyle.cap;this.lineJoin=savedStyle.join;dash=savedStyle.dash;},
    setLineDash(value){dash=Array.from(value);},beginPath(){path=[];},
    moveTo(x,y){path.push(['move',x,y]);},lineTo(x,y){path.push(['line',x,y]);},
    stroke(){strokes.push({width:this.lineWidth,color:this.strokeStyle,dash:[...dash],path:[...path]});}
  };
  context.routePen=pen;
  run('map.x=0;map.z=0;map.span=6000;');
  const routes={roads:[[[100,100],[200,200],[300,300]]],routes:[{id:1,kind:'main',points:[[0,0],[500,400]]},{id:2,kind:'lane',points:[[30,40],[200,150]]},{id:3,kind:'trail',points:[[20,80],[160,220]]}]};
  context.routeFeatures=routes;
  run('drawMapRoutes(routePen,routeFeatures,map.span)');
  assert.deepEqual(strokes.map(stroke=>stroke.width),[.7,1,1.5]);
  assert.deepEqual(strokes.map(stroke=>stroke.dash),[[2,4],[],[]]);
  assert.equal(new Set(strokes.map(stroke=>stroke.color)).size,3);
  assert.equal(strokes[2].path.length,2,'New routes must replace duplicate legacy roads.');
  assert.deepEqual(strokes[2].path[0],['move',400,250],'Route coordinates must use the same atlas projection as features.');
  assert.equal(pen.lineWidth,9);assert.equal(pen.strokeStyle,'original');assert.deepEqual(dash,[7,3],'Trail dashes must not leak into other atlas layers.');
  strokes.length=0;run('map.span=40000;drawMapRoutes(routePen,routeFeatures,map.span)');
  assert.equal(strokes.length,2,'Trails are hidden beyond the close survey.');
  assert.ok(strokes.every(stroke=>stroke.dash.length===0));
  assert.ok(strokes[1].width<=1.5 && strokes[1].width>=.9,'Regional roads stay thin in screen pixels.');
  strokes.length=0;context.routeFeatures={routes:[],roads:routes.roads};run('drawMapRoutes(routePen,routeFeatures,6000)');
  assert.equal(strokes.length,0,'An empty route list is authoritative.');
  strokes.length=0;context.routeFeatures={roads:routes.roads};run('drawMapRoutes(routePen,routeFeatures,6000)');
  assert.equal(strokes.length,1);assert.equal(strokes[0].width,1.5);assert.deepEqual(strokes[0].dash,[]);assert.equal(strokes[0].path.length,3);
  console.log('PASS: atlas route precedence; three thin road classes; trail dash/zoom visibility; world-coordinate projection; legacy fallback; drawing state isolation.');
}

async function verifySkyAndWalkControls() {
  const h = filterHarness({seed:1337,x:100,z:200,quality:1}); await h.run('boot()'); h.run('initialReady=true;');
  for (const [id,hour] of [['sky-dawn',6.4],['sky-day',12],['sky-dusk',17.7],['sky-night',22]]) {
    h.ids[id].fire('click'); assert.deepEqual(h.rendererEvents.at(-1),['time',hour]); assert.equal(h.ids['time-setting'].value,String(hour));
  }
  h.run(`walkingJourneys=[{name:'Test pass',x:100,z:200,yaw:1,pitch:0,minutes:6,points:[[100,200],[300,200],[300,500]]}];`);
  h.ids['walking-select'].value='0';h.ids['walking-select'].fire('change');assert.equal(h.ids['walking-start'].disabled,false);
  h.ids['walking-form'].fire('submit');assert.deepEqual(h.teleports.at(-1),[100,200]);assert.equal(h.run('activeJourney.next'),1);
  h.run('state.x=300;state.z=200;updateWalk();');assert.equal(h.run('waypoint.z'),500);
  h.run('state.x=300;state.z=500;updateWalk();');assert.equal(h.run('activeJourney'),null);assert.equal(h.run('waypoint'),null);
  h.run(`naturalWonders=[{name:'Stone arch',x:500,z:800,yaw:0.4,pitch:0.1,landmark_x:530,landmark_z:850}];`);
  h.ids['wonders-select'].value='0';h.ids['wonders-select'].fire('change');assert.equal(h.ids['wonders-travel'].disabled,false);
  h.ids['wonders-form'].fire('submit');assert.deepEqual(h.teleports.at(-1),[500,800]);assert.equal(h.run('waypoint.x'),530);assert.equal(h.run('waypoint.z'),850);
  console.log('PASS: celestial presets; natural-wonder viewpoint travel; continuous walk navigation and completion.');
}

async function main(){
  assert.equal(run('saved.x'),undefined); assert.equal(run('saved.waypoint'),undefined); assert.equal(run('quality'),2); assert.equal(run('sensitivity'),1.4);
  run('game=fakeGame;initialReady=true;state={x:100,z:200,stamina:75,health:100,mana:100,dayTime:9};');
  ids.world.fire('click',{clientX:10,clientY:20});
  assert.equal(calls,1);assert.equal(run('started'),true);assert.equal(run('lockPending'),true);
  document.pointerLockElement=ids.world;document.fire('pointerlockchange');
  assert.equal(run('locked'),true);
  document.fire('mousemove',{movementX:10,movementY:-5}); assert.deepEqual(looks.at(-1),[14,-7]);
  key('Escape');assert.equal(run('focusedLook'),false);assert.equal(run('locked'),false);
  ids.world.requestPointerLock=()=>Promise.reject(new Error('Blocked by host'));
  ids.world.fire('click',{clientX:10,clientY:10}); await new Promise(setImmediate);
  assert.equal(run('focusedLook'),true);assert.equal(run('pointerLockFallback'),true);
  document.fire('mousemove',{clientX:30,clientY:20});assert.deepEqual(looks.at(-1),[28,14]);
  key('Escape');const count=looks.length;document.fire('mousemove',{clientX:60,clientY:30});assert.equal(looks.length,count);
  // Escape during a pending request must survive both late rejection and success.
  ids.world.requestPointerLock=()=>new Promise((_,reject)=>rejected=reject);
  ids.world.fire('click',{clientX:5,clientY:5});assert.equal(run('lockPending'),true);
  key('Escape');rejected(new Error('Late'));await new Promise(setImmediate);assert.equal(run('focusedLook'),false);
  document.pointerLockElement=ids.world;document.fire('pointerlockchange');assert.equal(document.pointerLockElement,null);assert.equal(run('focusedLook'),false);
  key('KeyM');assert.equal(run('modal'),'map');
  run('map.span=27000;map.x=450;map.z=900;');key('KeyM');assert.equal(run('modal'),null);
  key('Tab');assert.equal(run('modal'),'map');assert.equal(run('map.span'),27000);assert.equal(run('map.x'),450);
  const tabEvent=key('Tab');assert.equal(tabEvent.prevented,true);assert.equal(run('modal'),'map');
  run(`map.visibleFeatures=[{name:'First',x:40,z:50,px:150,py:130},{name:'Last',x:80,z:90,px:210,py:180}];map.selected=null;`);
  document.fire('keydown',{code:'BracketLeft',target:ids['map-canvas']});assert.equal(run('map.selected.name'),'Last');
  document.fire('keydown',{code:'BracketRight',target:ids['map-canvas']});assert.equal(run('map.selected.name'),'First');
  document.fire('keydown',{code:'Enter',target:ids['map-canvas']});assert.equal(run('map.selected.x'),run('map.x'));assert.equal(run('map.selected.z'),run('map.z'));
  const before=run('JSON.stringify(screenToWorld(140,180))');run('zoomMap(.7,140,180)');const after=run('JSON.stringify(screenToWorld(140,180))');assert.deepEqual(JSON.parse(before),JSON.parse(after));
  key('KeyI');assert.equal(run('modal'),'bag');key('KeyC');assert.equal(run('modal'),'character');assert.equal(ids['character-stamina'].textContent,'75 / 100');key('KeyK');assert.equal(run('modal'),'skills');key('Escape');assert.equal(run('modal'),null);
  assert.equal(key('Space').prevented,true);assert.equal(run('jumpQueued'),true);
  // Exercise E through the real keyboard/modal code and factual topic callback.
  let ends=0, questions=[];
  fakeGame.end_dialogue=()=>ends++;
  fakeGame.interact=()=>({name:'Mira Vale',role:'ranger',detail:'Resident of Alderfield',text:'Good morning.',topics:[{id:'inn',label:'Nearest inn'}]});
  fakeGame.dialogue=topic=>{questions.push(topic);return {text:'The Birch Inn is east, about 60 paces.'};};
  key('KeyE');assert.equal(run('modal'),'dialogue');assert.equal(ids['dialogue-name'].textContent,'Mira Vale');
  assert.equal(document.activeElement,ids['dialogue-topics'].children[0]);assert.equal(run('locked'),false);
  ids['dialogue-topics'].children[0].fire('click');assert.deepEqual(questions,['inn']);assert.match(ids['dialogue-text'].textContent,/east/);
  key('Digit1');assert.equal(questions.length,2);assert.equal(ids['dialogue-text'].children.length,3);
  assert.match(ids['dialogue-text'].textContent,/Good morning/);
  assert.equal(key('KeyM').prevented,undefined);assert.equal(run('modal'),'dialogue');
  const modified=document.fire('keydown',{code:'Digit1',ctrlKey:true,target:document.activeElement});assert.equal(modified.prevented,undefined);assert.equal(questions.length,2);
  const logKey=document.fire('keydown',{code:'ArrowDown',target:ids['dialogue-text']});assert.equal(logKey.prevented,undefined);
  ids['dialogue-modal'].controls=[ids['dialogue-close'],...ids['dialogue-topics'].children,ids['dialogue-text']];
  ids['dialogue-text'].focus();key('Tab');assert.equal(document.activeElement,ids['dialogue-close']);
  document.fire('keydown',{code:'Tab',shiftKey:true,target:document.activeElement});assert.equal(document.activeElement,ids['dialogue-text']);
  key('Escape');assert.equal(run('modal'),null);assert.equal(ends,1);assert.equal(ids['dialogue-modal'].classList.contains('hidden'),true);
  fakeGame.interact=()=> 'The door opens.';key('KeyE');assert.equal(run('modal'),null);
  // Every long question has a short visible label, and the three final
  // directions have explicit letter commands rather than unreachable numbers.
  run(`renderConversation({name:'Test',role:'mage',detail:'',text:'Hello',topics:Array.from({length:12},(_,i)=>({id:'t'+i,label:'Topic '+i}))});modal='dialogue';`);
  key('KeyA');key('KeyB');key('KeyC');assert.deepEqual(questions.slice(-3),['t9','t10','t11']);
  key('Escape');
  console.log('PASS: E conversation focus, factual topic response, Escape resumes life, and door interaction stays in the world.');

  verifyAtlasRoutes();
  await verifyInteriorTours();
  await verifyFilterSettings();
  await verifyResolutionSettings();
  await verifyLightingSettings();
  await verifyRemovedFilterMigration();
  await verifyGroundCoverSettings();
  await verifyWeatherSettings();
  await verifyLandscapeDestinations();
  await verifySkyAndWalkControls();
  run(`var benchmarkCalls=[];game={...fakeGame,face:(...v)=>benchmarkCalls.push(['face',...v]),teleport:(...v)=>benchmarkCalls.push(['teleport',...v]),return_to_spawn:()=>benchmarkCalls.push(['spawn']),set_render_resolution:v=>benchmarkCalls.push(['resolution',v]),state:()=>({x:10,z:20,yaw:1,pitch:.1,seed:1337}),is_ready:()=>true};state=game.state();renderResolution=540;beginBenchmark(false);`);
  assert.equal(run('benchmark.height'),420);
  document.hidden=true;document.fire('visibilitychange');
  assert.equal(run('benchmark'),null);
  assert.equal(run(`benchmarkCalls.filter(c=>c[0]==='resolution').at(-1)[1]`),540);
  document.hidden=false;run('beginBenchmark(true);');key('Escape');
  assert.equal(run('benchmark'),null);
  assert.deepEqual(JSON.parse(run(`JSON.stringify(benchmarkCalls.filter(c=>c[0]==='teleport').at(-1))`)),['teleport',10,20]);
  assert.equal(ids['benchmark-start'].disabled,false);assert.equal(ids['benchmark-walk'].disabled,false);
  document.hidden=true;
  run('var hiddenTicks=0;game={...game,tick:()=>hiddenTicks++};renderFrame(1000);');
  assert.equal(run('hiddenTicks'),0,'Hidden previews must submit no engine/GPU frames.');
  assert.equal(run('lastFrame'),0,'Resume must not integrate the hidden time interval.');
  document.hidden=false;
  console.log('PASS: browser benchmark cancellation on hidden tab/Escape restores explicit resolution and original walk-test location.');
  console.log('PASS: save migration; synchronous click capture; captured look; rejected-capture focused look; Escape; late rejection/success; retained atlas; modal Tab accessibility; cursor-anchored zoom; I/C/K panels; Space jump.');
}
function verifyStreamingLifecycle() {
// Real worker coordinator: transfers are bounded and failures restore fallback.
let testWorker;
context.Worker = class {
  constructor(){testWorker=this;this.messages=[];}
  postMessage(message){this.messages.push(message);}
  terminate(){this.terminated=true;}
};
let asyncChanges=[], accepted=[];
fakeGame.set_async_streaming=v=>asyncChanges.push(v);
let streamJobs=[[1,0,0,0,0,1],[2,4,0,0,0,0],[3,3,0,0,0,0]];
fakeGame.next_stream_job=()=>streamJobs.shift() || [];
fakeGame.accept_stream_result=(ticket,bytes)=>{accepted.push(ticket);return true;};
run('game=fakeGame; startStreamingWorker();');
assert.deepEqual(asyncChanges,[],'local streaming continues until the worker is ready');
testWorker.onmessage({data:{type:'ready'}});assert.deepEqual(asyncChanges,[true]);run('pumpStreaming();');
assert.equal(testWorker.messages.filter(m=>m.type==='generate').length,2);
run('pumpStreaming();');
assert.equal(testWorker.messages.filter(m=>m.type==='generate').length,2,'at most two in-flight packets');
testWorker.onmessage({data:{type:'mesh',ticket:1,bytes:new Uint8Array(8)}});
run('pumpStreaming();');assert.deepEqual(accepted,[1]);
assert.equal(testWorker.messages.filter(m=>m.type==='generate').length,3);
testWorker.onmessage({data:{type:'error',message:'intentional worker failure test'}});
assert.equal(testWorker.terminated,true);assert.equal(asyncChanges.at(-1),false);
assert.equal(run('streamResults.length'),0);
console.log('Worker lifecycle passed: bounded jobs, deferred uploads, recoverable failure.');

// A tab can be hidden longer than the timeout while the worker finishes.
asyncChanges=[];streamJobs=[[4,0,0,0,0,1],[5,0,1,0,0,1]];
run('startStreamingWorker();');testWorker.onmessage({data:{type:'ready'}});run('pumpStreaming();');
let resumeNow=200000;
context.performance.now=()=>resumeNow;
fakeGame.accept_stream_result=(ticket,bytes)=>{accepted.push(ticket);resumeNow+=3;return true;};
run('streamDeadline=100;');
testWorker.onmessage({data:{type:'mesh',ticket:4,bytes:new Uint8Array(8)}});
testWorker.onmessage({data:{type:'mesh',ticket:5,bytes:new Uint8Array(8)}});
run('pumpStreaming();');
assert.equal(testWorker.terminated,undefined,'completed packets survive a long hidden tab');
assert.equal(run('streamOutstanding'),1,'an upload exceeding the frame budget defers the second packet');
assert.equal(run('streamResults.length'),1,'queued completion must not be mistaken for a timeout');
run('pumpStreaming();');
assert.equal(run('streamOutstanding'),0);
run('stopStreamingWorker();');context.performance.now=()=>0;
console.log('Worker resume passed: completed results survive elapsed hidden-tab deadlines.');

let coordination;
context.BroadcastChannel=class { constructor(){coordination=this;this.messages=[];} postMessage(m){this.messages.push(m);} };
context.setInterval=()=>1;
run('benchmark=null; document.hidden=false; initRenderCoordination();');
assert.equal(ids.world.attributes['data-render-active'],'true');
const ownClaim=run('renderClaim');
coordination.onmessage({data:{type:'claim',id:'second-view',stamp:ownClaim+100}});
assert.equal(run('otherViewActive'),true);
assert.equal(ids.world.attributes['data-render-active'],'false');
const staleSave=stored['wayfarer.exploration.v4'];
run('adaptive.active=true; adaptive.stableSince=-60000; saveProgress(); renderFrame(1000);');
assert.equal(run('adaptive.active'),false,'Paused view must actually freeze the governor through renderFrame.');
assert.equal(run('lastFrame'),0);
assert.equal(stored['wayfarer.exploration.v4'],staleSave,'paused previews cannot overwrite progress');
run('claimRenderer();');assert.equal(run('otherViewActive'),false);
const messageCount=coordination.messages.length;
run('claimRenderer();');assert.equal(coordination.messages.length,messageCount,'normal input does not continually reset frame timing');
coordination.onmessage({data:{type:'claim',id:'second-view',stamp:ownClaim}});
assert.equal(run('otherViewActive'),false,'stale claims cannot steal the renderer');
console.log('Single-view rendering passed: pause, input takeover, stale claims and progress protection.');

}
main().then(verifyStreamingLifecycle).catch(error=>{console.error(error);process.exitCode=1;});
