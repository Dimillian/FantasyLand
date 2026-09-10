const fs = require('fs');
const vm = require('vm');
const assert = require('assert/strict');
const html = fs.readFileSync(require('path').join(__dirname, '../dist/index.html'), 'utf8');
const source = fs.readFileSync(require('path').join(__dirname, '../dist/app.js'), 'utf8').replace(/boot\(\);\s*$/, '');
class Element {
  constructor(id='') { this.id=id; this.listeners={}; this.attributes={}; this.style={}; this.classList={ add(){}, remove(){}, toggle(){}, contains(){return true;} }; this.clientWidth=800; this.clientHeight=500; this.width=800; this.height=500; this.tagName='DIV'; this.value=''; }
  addEventListener(name, callback) { (this.listeners[name] ||= []).push(callback); }
  fire(name, data={}) { const event={target:this, preventDefault(){this.prevented=true;}, ...data}; for(const cb of this.listeners[name]||[]) cb(event); return event; }
  setAttribute(name,value){this.attributes[name]=value;}
  focus(){document.activeElement=this;}
  querySelector(){return this.child ||= new Element();}
  getBoundingClientRect(){return {width:800,height:500,left:0,top:0};}
  getContext(){return {};}
  setPointerCapture(){}
}
const ids = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map((m)=>[m[1],new Element(m[1])]));
ids.world.tagName='CANVAS'; ids['map-canvas'].tagName='CANVAS';
const document = new Element('document');
document.body=new Element('body'); document.getElementById=id=>ids[id];
document.querySelectorAll=selector=>selector==='[data-close]' ? ['map','bag','character','skills','settings'].map(name=>ids[name+'-modal'].querySelector()) : selector==='.overlay' ? ['map','bag','character','skills','settings'].map(name=>ids[name+'-modal']) : [];
document.pointerLockElement=null;
document.exitPointerLock=()=>{document.pointerLockElement=null;document.fire('pointerlockchange');};
const stored={ 'wayfarer.exploration.v3': JSON.stringify({seed:1337,quality:2,sensitivity:1.4,x:900,z:800,waypoint:{x:5,z:8,name:'Old'},atlas:{x:9,z:8,span:700}}) };
let calls=0, looks=[], rejected;
ids.world.requestPointerLock=()=>{calls++;};
const fakeGame={look:(x,y)=>looks.push([x,y]),state:()=>({x:100,z:200,stamina:75}),map_data(){return new Uint8Array(320*320*4);},features(){return {};},set_time(){},set_quality(){},set_filter(){},return_to_spawn(){},teleport(){}};
const context=vm.createContext({document,window:new Element('window'),navigator:{gpu:{}},location:{href:'https://test.invalid/'},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(){},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:k=>stored[k],setItem:(k,v)=>stored[k]=v},fakeGame});
vm.runInContext(source,context);
const run=code=>vm.runInContext(code,context);
const key=code=>document.fire('keydown',{code,target:ids.world});
// Run the real boot/settings code against a GPU boundary stub. This verifies
// renderer calls and reload behavior, not merely the shape of saved fields.
function filterHarness(snapshot) {
  const filterIds = Object.fromEntries([...html.matchAll(/id="([^"]+)"/g)].map((m) => [m[1], new Element(m[1])]));
  const filterDocument = new Element('document');
  filterDocument.body = new Element('body');
  filterDocument.getElementById = id => filterIds[id];
  filterDocument.querySelectorAll = selector => selector === '[data-close]' ? ['map', 'bag', 'character', 'skills', 'settings'].map(name => filterIds[name + '-modal'].querySelector()) : selector === '.overlay' ? ['map', 'bag', 'character', 'skills', 'settings'].map(name => filterIds[name + '-modal']) : [];
  const filterStore = { 'wayfarer.exploration.v4': JSON.stringify(snapshot) };
  const filterCalls = [], teleports = [];
  const engine = { set_filter:(mode,strength)=>filterCalls.push([mode,strength]), world_size:()=>256000, set_quality(){}, resize(){}, teleport:(x,z)=>teleports.push([x,z]), state:()=>({x:snapshot.x,z:snapshot.z,stamina:100,health:100,mana:100}) };
  let readyFrames = 0;
  const filterContext = vm.createContext({document:filterDocument,window:new Element('window'),navigator:{gpu:{}},location:{href:'https://test.invalid/'},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(callback){if (++readyFrames <= 2) queueMicrotask(()=>callback(0));},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:key=>filterStore[key],setItem:(key,value)=>filterStore[key]=value},fakeModule:{default:async()=>{},Game:{create:async()=>engine}}});
  const bootSource = source.replace("const { default: init, Game } = await import('./pkg/fantasy_land.js');", 'const { default: init, Game } = fakeModule;');
  vm.runInContext(bootSource,filterContext);
  return {ids:filterIds,calls:filterCalls,teleports,run:code=>vm.runInContext(code,filterContext),saved:()=>JSON.parse(filterStore['wayfarer.exploration.v4'])};
}

async function verifyFilterSettings() {
  const existing = {seed:1337,x:637,z:222,quality:2,sensitivity:1.2,waypoint:{x:810,z:390,name:'The Road'},atlas:{x:640,z:225,span:6000}};
  const first = filterHarness(existing);
  assert.equal(first.run('filterMode'),1); assert.equal(first.run('filterStrength'),1);
  assert.equal(first.ids['filter-select'].value,'1'); assert.equal(first.ids['filter-strength'].value,'100');
  assert.equal(first.ids['filter-strength'].disabled,false);
  await first.run('boot()');
  assert.deepEqual(first.calls,[[1,1]],'Boot must apply Bloom at 100% to the renderer.');
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
  await verifyFilterSettings();
  console.log('PASS: save migration; synchronous click capture; captured look; rejected-capture focused look; Escape; late rejection/success; retained atlas; modal Tab accessibility; cursor-anchored zoom; I/C/K panels; Space jump.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
