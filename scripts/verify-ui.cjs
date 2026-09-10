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
const fakeGame={look:(x,y)=>looks.push([x,y]),state:()=>({x:100,z:200,stamina:75}),map_data(){return new Uint8Array(320*320*4);},features(){return {};},set_time(){},set_quality(){},return_to_spawn(){},teleport(){}};
const context=vm.createContext({document,window:new Element('window'),navigator:{gpu:{}},location:{href:'https://test.invalid/'},URL,console,Map,Set,Math,Number,JSON,Promise,Uint8Array,Uint8ClampedArray,ImageData:function(){},devicePixelRatio:1,performance:{now:()=>0},requestAnimationFrame(){},setTimeout(){return 1;},clearTimeout(){},matchMedia:()=>({matches:false}),localStorage:{getItem:k=>stored[k],setItem:(k,v)=>stored[k]=v},fakeGame});
vm.runInContext(source,context);
const run=code=>vm.runInContext(code,context);
const key=code=>document.fire('keydown',{code,target:ids.world});
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
  console.log('PASS: save migration; synchronous click capture; captured look; rejected-capture focused look; Escape; late rejection/success; retained atlas; modal Tab accessibility; cursor-anchored zoom; I/C/K panels; Space jump.');
}
main().catch(error=>{console.error(error);process.exitCode=1;});
