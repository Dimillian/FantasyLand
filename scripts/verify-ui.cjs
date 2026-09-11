const fs = require('fs');
const vm = require('vm');
const assert = require('assert/strict');
const html = fs.readFileSync(require('path').join(__dirname, '../dist/index.html'), 'utf8');
const source = fs.readFileSync(require('path').join(__dirname, '../dist/app.js'), 'utf8').replace(/boot\(\);\s*$/, '');
class Element {
  constructor(id='') { this.id=id; this.listeners={}; this.attributes={}; this.style={}; this.classes=new Set(); this.classList={add:(...names)=>names.forEach(name=>this.classes.add(name)),remove:(...names)=>names.forEach(name=>this.classes.delete(name)),toggle:(name,force)=>{const add=force ?? !this.classes.has(name); if(add)this.classes.add(name);else this.classes.delete(name);return add;},contains:name=>this.classes.has(name)}; this.clientWidth=800; this.clientHeight=500; this.width=800; this.height=500; this.tagName='DIV'; this.value=''; }
  addEventListener(name, callback) { (this.listeners[name] ||= []).push(callback); }
  fire(name, data={}) { const event={target:this, preventDefault(){this.prevented=true;}, ...data}; for(const cb of this.listeners[name]||[]) cb(event); return event; }
  setAttribute(name,value){this.attributes[name]=value;}
  focus(){document.activeElement=this;}
  querySelector(){return this.child ||= new Element();}
  getBoundingClientRect(){return {width:800,height:500,left:0,top:0};}
  getContext(){return {};}
  setPointerCapture(){}
  append(child){(this.children ||= []).push(child);}
}
const ids = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map((m)=>[m[1],new Element(m[1])]));
ids.world.tagName='CANVAS'; ids['map-canvas'].tagName='CANVAS';
const document = new Element('document');
document.body=new Element('body'); document.getElementById=id=>ids[id]; document.createElement=tag=>new Element(tag);
document.querySelectorAll=selector=>selector==='[data-close]' ? ['map','bag','character','skills','settings'].map(name=>ids[name+'-modal'].querySelector()) : selector==='.overlay' ? ['map','bag','character','skills','settings'].map(name=>ids[name+'-modal']) : [];
document.pointerLockElement=null;
document.exitPointerLock=()=>{document.pointerLockElement=null;document.fire('pointerlockchange');};
const stored={ 'wayfarer.exploration.v3': JSON.stringify({seed:1337,quality:2,sensitivity:1.4,x:900,z:800,waypoint:{x:5,z:8,name:'Old'},atlas:{x:9,z:8,span:700}}) };
let calls=0, looks=[], rejected;
ids.world.requestPointerLock=()=>{calls++;};
const fakeGame={look:(x,y)=>looks.push([x,y]),state:()=>({x:100,z:200,stamina:75}),map_data(){return new Uint8Array(320*320*4);},features(){return {};},landscape_destinations(){return [];},set_time(){},set_quality(){},set_ground_cover_density(){},set_shadows(){},set_filter(){},set_ascii(){},set_render_resolution(){},render_resolution:()=>new Uint32Array([800,500]),return_to_spawn(){},teleport(){}};
const context=vm.createContext({document,window:new Element('window'),navigator:{gpu:{}},location:{href:'https://test.invalid/'},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(){},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:k=>stored[k],setItem:(k,v)=>stored[k]=v},fakeGame});
vm.runInContext(source,context);
const run=code=>vm.runInContext(code,context);
const key=code=>document.fire('keydown',{code,target:ids.world});
// Run the real boot/settings code against a GPU boundary stub. This verifies
// renderer calls and reload behavior, not merely the shape of saved fields.
function filterHarness(snapshot, destinations = []) {
  const filterIds = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map((m) => [m[1], new Element(m[1])]));
  const filterDocument = new Element('document');
  filterDocument.body = new Element('body');
  filterDocument.getElementById = id => filterIds[id];
  filterDocument.createElement = tag => new Element(tag);
  filterDocument.querySelectorAll = selector => selector === '[data-close]' ? ['map', 'bag', 'character', 'skills', 'settings'].map(name => filterIds[name + '-modal'].querySelector()) : selector === '.overlay' ? ['map', 'bag', 'character', 'skills', 'settings'].map(name => filterIds[name + '-modal']) : [];
  const filterStore = { 'wayfarer.exploration.v4': JSON.stringify(snapshot) };
  const filterCalls = [], teleports = [], asciiCalls = [], resolutionCalls = [], qualityCalls = [], groundCoverCalls = [], rendererEvents = [];
  let destinationCalls = 0;
  const playerState = {x:snapshot.x,z:snapshot.z,stamina:100,health:100,mana:100};
  let selectedResolution = 0, selectedQuality = 1, surfaceWidth = 800, surfaceHeight = 500;
  const engine = {
    landscape_destinations:()=>{destinationCalls++;return typeof destinations === 'function' ? destinations() : destinations;},
    set_shadows:value=>{rendererEvents.push(['shadows',value]);},
    set_ground_cover_density:value=>{groundCoverCalls.push(value);rendererEvents.push(['groundCover',value]);},
    set_filter:(mode,strength)=>{filterCalls.push([mode,strength]);rendererEvents.push(['filter',mode,strength]);},
    set_ascii:(scale,palette)=>{asciiCalls.push([scale,palette]);rendererEvents.push(['ascii',scale,palette]);},
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
  const filterContext = vm.createContext({document:filterDocument,window:new Element('window'),navigator:{gpu:{}},location:{href:'https://test.invalid/'},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(callback){if (++readyFrames <= 2) queueMicrotask(()=>callback(0));},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:key=>filterStore[key],setItem:(key,value)=>filterStore[key]=value},fakeModule:{default:async()=>{},Game:{create:async()=>engine}}});
  const bootSource = source.replace("const { default: init, Game } = await import('./pkg/fantasy_land.js');", 'const { default: init, Game } = fakeModule;');
  vm.runInContext(bootSource,filterContext);
  return {ids:filterIds,get destinationCalls(){return destinationCalls;},calls:filterCalls,teleports,asciiCalls,resolutionCalls,qualityCalls,groundCoverCalls,rendererEvents,run:code=>vm.runInContext(code,filterContext),saved:()=>JSON.parse(filterStore['wayfarer.exploration.v4'])};
}

async function verifyFilterSettings() {
  const existing = {seed:1337,x:637,z:222,quality:2,sensitivity:1.2,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
  const first = filterHarness(existing);
  assert.equal(first.run('filterMode'),1); assert.equal(first.run('filterStrength'),1);
  assert.equal(first.ids['filter-select'].value,'1'); assert.equal(first.ids['filter-strength'].value,'100');
  assert.equal(first.ids['filter-strength'].disabled,false);
  assert.equal(first.run('renderResolution'),0); assert.equal(first.run('asciiScale'),2); assert.equal(first.run('asciiPalette'),0);
  assert.equal(first.ids['render-resolution'].value,'0'); assert.equal(first.ids['ascii-scale'].value,'2'); assert.equal(first.ids['ascii-palette'].value,'0');
  assert.equal(first.ids['ascii-options'].classList.contains('hidden'),true);
  await first.run('boot()');
  assert.deepEqual(first.calls,[[1,1]],'Boot must apply Bloom at 100% to the renderer.');
  assert.deepEqual(first.asciiCalls,[[2,0]]); assert.deepEqual(first.resolutionCalls,[0]);
  assert.equal(first.ids['render-dimensions'].textContent,'Actual 720 × 450');
  assert.deepEqual(first.rendererEvents.map(event=>event[0]),['quality','resolution','ascii','filter','groundCover','shadows','resize'],'Preferences must apply after world quality and before resize.');
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
  console.log('PASS: Bloom defaults; renderer initialization; filter selection and amount API calls; v4 position/waypoint/atlas preservation; reload persistence; Clean disabled state; zero strength; maximum clamp.');
}

async function verifyAsciiResolutionSettings() {
  const existing = {seed:1337,x:810,z:-160,quality:2,sensitivity:1,filterMode:3,filterStrength:0,asciiScale:3,asciiPalette:1,renderResolution:360,waypoint:{x:900,z:-210,name:'The Pass'},atlas:{x:850,z:-180,span:8000}};
  const selected = filterHarness(existing); await selected.run('boot()'); selected.run('initialReady=true;');
  assert.deepEqual(selected.calls,[[3,0]],'ASCII must remain selected even with a saved zero filter strength.');
  assert.deepEqual(selected.asciiCalls,[[3,1]]); assert.deepEqual(selected.resolutionCalls,[360]);
  assert.equal(selected.ids['ascii-options'].classList.contains('hidden'),false);
  assert.equal(selected.ids['filter-strength'].disabled,true);
  assert.equal(selected.ids['render-dimensions'].textContent,'Actual 576 × 360');
  const before=selected.calls.length;
  selected.ids['filter-strength'].value='120'; selected.ids['filter-strength'].fire('input');
  assert.equal(selected.calls.length,before,'ASCII must ignore its disabled blend strength.');
  selected.ids['ascii-scale'].value='1'; selected.ids['ascii-scale'].fire('change'); assert.deepEqual(selected.asciiCalls.at(-1),[1,1]);
  selected.ids['ascii-palette'].value='2'; selected.ids['ascii-palette'].fire('change'); assert.deepEqual(selected.asciiCalls.at(-1),[1,2]);
  selected.ids['render-resolution'].value='720'; selected.ids['render-resolution'].fire('change');
  assert.equal(selected.resolutionCalls.at(-1),720); assert.equal(selected.ids['render-dimensions'].textContent,'Actual 1152 × 720');
  const resolutionCallCount=selected.resolutionCalls.length;
  selected.ids['quality-select'].value='0'; selected.ids['quality-select'].fire('change');
  assert.equal(selected.qualityCalls.at(-1),0); assert.equal(selected.resolutionCalls.length,resolutionCallCount,'Changing world quality must not overwrite an explicit resolution.');
  assert.equal(selected.ids['render-dimensions'].textContent,'Actual 1152 × 720');
  const persisted=selected.saved();
  assert.equal(persisted.asciiScale,1); assert.equal(persisted.asciiPalette,2); assert.equal(persisted.renderResolution,720); assert.equal(persisted.filterMode,3);
  assert.equal(persisted.x,existing.x); assert.equal(persisted.z,existing.z); assert.deepEqual(persisted.waypoint,existing.waypoint); assert.deepEqual(persisted.atlas,existing.atlas);
  const restored=filterHarness(persisted); await restored.run('boot()'); restored.run('initialReady=true;');
  assert.deepEqual(restored.asciiCalls,[[1,2]]); assert.deepEqual(restored.resolutionCalls,[720]); assert.deepEqual(restored.calls,[[3,0]]);
  restored.ids['render-resolution'].value='1'; restored.ids['render-resolution'].fire('change');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 800 × 500');
  restored.ids.world.clientWidth=900;restored.ids.world.clientHeight=600;restored.run('resize()');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 900 × 600','Native actual dimensions must follow viewport resize.');
  restored.ids['render-resolution'].value='0'; restored.ids['render-resolution'].fire('change');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 360 × 240');
  restored.ids['quality-select'].value='1'; restored.ids['quality-select'].fire('change');
  assert.equal(restored.ids['render-dimensions'].textContent,'Actual 540 × 360','Auto must follow world quality.');
  restored.ids['filter-select'].value='1'; restored.ids['filter-select'].fire('change');
  assert.equal(restored.ids['ascii-options'].classList.contains('hidden'),true); assert.equal(restored.ids['filter-strength'].disabled,false);
  restored.ids['filter-select'].value='3'; restored.ids['filter-select'].fire('change');
  assert.equal(restored.ids['ascii-options'].classList.contains('hidden'),false); assert.equal(restored.ids['filter-strength'].disabled,true);
  assert.equal(restored.ids['ascii-scale'].value,'1'); assert.equal(restored.ids['ascii-palette'].value,'2');
  const invalid=filterHarness({...existing,renderResolution:999,asciiScale:99,asciiPalette:99});
  assert.equal(invalid.run('renderResolution'),0);assert.equal(invalid.run('asciiScale'),2);assert.equal(invalid.run('asciiPalette'),0);
  console.log('PASS: ASCII full-mode preference; size/palette API calls; disabled ASCII strength; resolution defaults/boot ordering; explicit resolution independent of quality; Native resize; Auto quality; actual-size labels; v4 persistence.');
}

async function verifyGroundCoverSettings() {
  const existing = {seed:1337,x:637,z:222,quality:2,sensitivity:1.2,filterMode:3,filterStrength:0.8,renderResolution:720,asciiScale:3,asciiPalette:2,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
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

async function verifyLandscapeDestinations() {
  const existing={seed:1337,x:637,z:222,quality:2,sunShadows:false,groundCoverDensity:4,sensitivity:1.2,filterMode:2,filterStrength:1.1,renderResolution:720,asciiScale:3,asciiPalette:2,atlas:{x:640,z:225,span:6000}};
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
  const tabEvent=key('Tab');assert.equal(tabEvent.prevented,undefined);assert.equal(run('modal'),'map');
  const before=run('JSON.stringify(screenToWorld(140,180))');run('zoomMap(.7,140,180)');const after=run('JSON.stringify(screenToWorld(140,180))');assert.deepEqual(JSON.parse(before),JSON.parse(after));
  key('KeyI');assert.equal(run('modal'),'bag');key('KeyC');assert.equal(run('modal'),'character');assert.equal(ids['character-stamina'].textContent,'75 / 100');key('KeyK');assert.equal(run('modal'),'skills');key('Escape');assert.equal(run('modal'),null);
  assert.equal(key('Space').prevented,true);assert.equal(run('jumpQueued'),true);
  verifyAtlasRoutes();
  await verifyFilterSettings();
  await verifyAsciiResolutionSettings();
  await verifyGroundCoverSettings();
  await verifyLandscapeDestinations();
  console.log('PASS: save migration; synchronous click capture; captured look; rejected-capture focused look; Escape; late rejection/success; retained atlas; modal Tab accessibility; cursor-anchored zoom; I/C/K panels; Space jump.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
