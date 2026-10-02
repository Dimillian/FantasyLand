// Review harness checks: absent timings, A/A noise, paired state and workloads.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto').webcrypto;
const {reanalyzeReports,main:reanalyzeMain}=require('./reanalyze-rendering-report.cjs');
const elements = new Map();
function element(id) {
  if (!elements.has(id)) elements.set(id,{value:id==='scene'?'forest':'full',style:{setProperty(){}},textContent:''});
  return elements.get(id);
}
const context=vm.createContext({document:{getElementById:element,hidden:false},
  fetch:async()=>({ok:false}),requestAnimationFrame(){},console,Math,JSON,Number,Array,Map,Set,
  Error,crypto,Uint8Array,TextDecoder,performance:{now:()=>0}});
vm.runInContext(fs.readFileSync(path.join(__dirname,'../dist/rendering-review.js'),'utf8'),context);
const evaluate=source=>vm.runInContext(source,context);
assert.equal(evaluate('stats([])'),null,'Missing GPU samples must remain unavailable');
assert.equal(evaluate('stats([NaN,Infinity])'),null,'Invalid values must not produce measured timings');
assert.equal(evaluate('stats([3,1,2]).medianMs'),2);
evaluate(`var options={control:'main-candidate',blocks:3};
  var ratios={frameMean:.01,frameMedian:.01,frameP95:.01,frameP99:.01,cpuMean:.01,cpuP95:.01};
  var blocks=[0,1,2].map(block=>({block,ratios:{...ratios},initialStateMismatches:[],finalPoseMismatches:[]}));
  var noise={validation:{noiseEnvelope:{frameMean:.02,frameMedian:.02,frameP95:.02,frameP99:.02,cpuMean:.02,cpuP95:.02}}};`);
assert.equal(evaluate('performanceGate(options,blocks,null).status'),'inconclusive','Paired aggregates alone cannot establish a pass');
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'provisional','Parity below noise still leaves GPU headroom unverified');
evaluate('blocks[0].ratios.cpuMean=.5;');
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'provisional','One anomalous block is not repeatable regression');
evaluate('blocks[1].ratios.cpuMean=.07;');
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'regression','Two slow blocks above A/A noise must fail');
evaluate("blocks[2].initialStateMismatches=['eye[1]'];");
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'invalid','Different camera state invalidates A/B timing');
evaluate(`blocks=[0,1,2].map(block=>({block,ratios:Object.fromEntries(Object.keys(metricPaths).map(name=>[name,name==='frameMedian'?5e-10:0])),initialStateMismatches:[],finalPoseMismatches:[]}));
  noise={validation:{noiseEnvelope:Object.fromEntries(Object.keys(metricPaths).map(name=>[name,0]))}};`);
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'provisional','Floating roundoff at a zero noise floor must not create a regression.');
evaluate('blocks.forEach(block=>block.ratios.frameMedian=1e-6);');
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'regression','Even tiny repeatable slowdowns beyond roundoff must remain detectable.');
evaluate(`blocks=[0,1,2].map(block=>({block,ratios:{frameMedian:0,frameP95:0,frameP99:0,cpuMean:0,cpuP95:0},a:{frame:{meanMs:16.7}},b:{frame:{meanMs:17.2}},initialStateMismatches:[],finalPoseMismatches:[]}));
  noise={validation:{noiseEnvelope:{frameMedian:0,frameP95:0,frameP99:0,cpuMean:0,cpuP95:0}},blocks:[0,1].map(block=>({block,a:{frame:{meanMs:16.7}},b:{frame:{meanMs:16.72}}}))};`);
assert.equal(evaluate('performanceGate(options,blocks,noise).status'),'regression','Repeated mean frame slowdown must fail even when every frame percentile is unchanged.');
assert.deepEqual(JSON.parse(evaluate('JSON.stringify(Object.keys(performanceGate(options,blocks,noise).regressions))')),['frameMean']);
assert.ok(Math.abs(evaluate('performanceGate(options,blocks,noise).noiseEnvelope.frameMean')-(16.72/16.7-1))<1e-12,
  'Older A/A reports must derive the mean noise floor from their saved block means.');
assert.equal(evaluate("reportName({scene:'forest',stage:'full',resolution:720,workload:'static',control:'main-main'})"),
  'browser-forest-off-720-static-main-main.json','Baseline control is independent of selected upgrade');
evaluate(`var events=[];var game={face:(...v)=>events.push(['face',...v]),teleport:(...v)=>events.push(['teleport',...v]),tick(){}};
  var fixture={eye:[100,20,200],yaw:1,pitch:.1};
  advanceWorkload(game,fixture,'transition',0,300);`);
assert.equal(evaluate("events.filter(v=>v[0]==='teleport').length"),0,'First measured frame must not clear warm geometry');
evaluate("advanceWorkload(game,fixture,'transition',100,300);advanceWorkload(game,fixture,'transition',200,300);");
assert.equal(evaluate("events.filter(v=>v[0]==='teleport').length"),2,'Every selected duration contains both cold-stream transitions');
evaluate("events=[];advanceWorkload(game,fixture,'sweep',299,300);");
assert.ok(Math.abs(evaluate('events[0][1]')-1)<1e-10,'Sweep follows a full cycle at every selected duration');
assert.throws(()=>evaluate("requireGpuDrain({},'main')"),/main GPU drain API missing; rebuild both/,'Old harness builds must fail explicitly.');

function verifyReanalysis() {
  const options={control:'main-candidate',blocks:2,scene:'forest',resolution:720,workload:'static',frames:1000};
  const state={pose:{eye:[100,20,200],yaw:1,pitch:.1,hour:9},internal:[1280,720],
    triangles:100,meshMB:1,people:0,pending:0,coverDensity:4,coverInstances:20};
  const runs=[];
  for(let block=0;block<2;block++)for(const role of ['A','B','B','A']) {
    const gaps=Array(1000).fill(16.7);
    if(role==='B')gaps[999]=200;
    runs.push({block,role,version:role==='A'?'main':'candidate',initialState:state,finalState:state,
      setup:{gpuQueueDrained:true},raw:{gaps,cpu:Array(1000).fill(1),gpu:[],gpuFresh:[]}});
  }
  const report={options,runs};
  const control={options:{...options,control:'main-main'},runs:runs.map(run=>({...run,version:'main',raw:{...run.raw,gaps:Array(1000).fill(16.7)}})),
    blocks:[0,1].map(block=>({block,a:{frame:{meanMs:16.7}},b:{frame:{meanMs:16.7}}})),
    validation:{pairedStateMatch:true,noiseEnvelope:{frameMedian:0,frameP95:0,frameP99:0,cpuMean:0,cpuP95:0}}};
  const untouched=JSON.stringify([report,control]);
  const analysis=reanalyzeReports(report,control);
  assert.equal(analysis.validation.status,'regression');
  assert.deepEqual(Object.keys(analysis.validation.regressions),['frameMean']);
  for(const block of analysis.blocks) {
    assert.equal(block.ratios.frameMedian,0);assert.equal(block.ratios.frameP95,0);assert.equal(block.ratios.frameP99,0);
    assert.ok(block.ratios.frameMean>0.01,'Rare missed frames must raise the mean while percentiles remain equal.');
  }
  assert.equal(JSON.stringify([report,control]),untouched,'Reanalysis must not modify either source report or its raw samples.');
  const incompatible=reanalyzeReports(report,{...control,options:{...control.options,workload:'walk'}});
  assert.equal(incompatible.validation.status,'inconclusive','A different workload cannot supply its noise envelope.');
  const temporary=fs.mkdtempSync(path.join(require('node:os').tmpdir(),'render-reanalysis-'));
  try {
    const sourcePath=path.join(temporary,'report.json'),controlPath=path.join(temporary,'control.json');
    fs.writeFileSync(sourcePath,JSON.stringify(report));fs.writeFileSync(controlPath,JSON.stringify(control));
    assert.throws(()=>reanalyzeMain([sourcePath,controlPath,'--out',sourcePath]),/raw samples are read-only/);
    assert.equal(fs.readFileSync(sourcePath,'utf8'),JSON.stringify(report),'An output path cannot overwrite raw measurements.');
  } finally {fs.rmSync(temporary,{recursive:true,force:true});}
}
verifyReanalysis();

async function verifyMetadata() {
  const info={};
  for(const [key,value] of Object.entries({vendor:'test vendor',architecture:'test architecture',device:'test device',description:'test adapter'})) {
    Object.defineProperty(info,key,{get:()=>value});
  }
  context.adapterInfo=info;
  assert.equal(JSON.stringify(info),'{}','The mock reproduces nonenumerable browser adapter properties.');
  assert.deepEqual(JSON.parse(evaluate('JSON.stringify(adapterMetadata(adapterInfo))')),
    {vendor:'test vendor',architecture:'test architecture',device:'test device',description:'test adapter'});
  assert.equal(evaluate('adapterMetadata(null)'),null);
  const expected='ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad';
  assert.equal(await evaluate('hashBytes(new Uint8Array([97,98,99]))'),expected,'Hash the bytes actually fetched with SHA-256.');
  const calls=[],previousFetch=context.fetch;
  context.fetch=async(url,options)=>{calls.push([url,options.cache]);return {ok:true,arrayBuffer:async()=>new Uint8Array([97,98,99]).buffer};};
  try {
    const fingerprints=JSON.parse(JSON.stringify(await evaluate('runtimeFingerprints()')));
    assert.deepEqual(Object.keys(fingerprints),['main','candidate']);
    for(const fingerprint of Object.values(fingerprints)) {
      assert.equal(fingerprint.sha256,expected);assert.equal(fingerprint.bytes,3);
    }
    assert.deepEqual(calls,[['/comparison/baseline/source/dist/pkg/fantasy_land_bg.wasm','no-store'],['./pkg/fantasy_land_bg.wasm','no-store']]);
  } finally {context.fetch=previousFetch;}
}

async function verifyGpuDrains() {
  const events=[];
  let ticks=0,clock=0,drainPolls=0;
  const mockGame={
    set_quality(){},set_render_resolution(){},set_antialiasing(){},set_filter(){},set_ground_cover_density(){},
    set_shadows(){},set_reflections(){},set_enclosure(){},set_lighting_mode(){},set_async_streaming(){},
    teleport(){},face(){},set_time(){},set_weather_mode(){},set_weather_paused(){},
    tick(){ticks++;events.push(['tick',ticks]);},pending_chunks:()=>0,
    next_stream_job(){events.push(['pump',ticks]);return [];},
    begin_gpu_drain(){drainPolls=0;events.push(['drain',ticks]);},
    gpu_drained(){if(++drainPolls<3)return false;events.push(['drained',ticks]);return true;},
    state:()=>({cameraPosition:[100,20,200],yaw:1,pitch:.1,dayTime:9,triangleCount:100,meshMegabytes:1.5,
      nearbyPeople:1,streamingPending:0,groundCoverDensity:4,coverInstances:20,lightingMode:0,gpuSampleId:ticks,gpuRenderMs:10}),
    render_resolution:()=>[1280,720],free(){events.push(['free',ticks]);}
  };
  const benchContext=vm.createContext({document:{getElementById:element,hidden:false},
    fetch:async()=>({ok:false}),requestAnimationFrame(callback){clock+=16.667;callback(clock);},
    performance:{now:()=>clock},console,Math,JSON,Number,Array,Map,Set,Error,URL,
    location:{href:'https://test.invalid/'},
    Worker:class {
      postMessage(message){if(message.type==='init')this.onmessage({data:{type:'ready'}});}
      terminate(){}
    },
    mockImport:async url=>{events.push(['import',url]);return {default:async({module_or_path})=>{events.push(['wasm',module_or_path.href]);},Game:{create:async()=>mockGame}};}});
  const source=fs.readFileSync(path.join(__dirname,'../dist/rendering-review.js'),'utf8')
    .replace('await import(pkg)','await mockImport(pkg)');
  vm.runInContext(source,benchContext);
  const report=await vm.runInContext("measure('candidate','B',0,{eye:[100,20,200],yaw:1,pitch:.1,hour:9,weather:1},{resolution:720,frames:3,workload:'static',runtimeWasm:{candidate:{sha256:'abc'}}},0)",benchContext);
  assert.deepEqual(events.slice(0,2),[['import','./pkg/fantasy_land.js?sha256=abc'],['wasm','https://test.invalid/pkg/fantasy_land_bg.wasm?sha256=abc']],
    'Select runtime module URLs using the recorded content hash to avoid stale imported builds.');
  assert.deepEqual(events.filter(event=>event[0]==='drain').map(event=>event[1]),[600,780,783],
    'Drain weather submissions, settled frames and measured frames separately.');
  for(let start=0;start<events.length;start++) {
    if(events[start][0]!=='drain')continue;
    const end=events.findIndex((event,index)=>index>start && event[0]==='drained');
    assert.ok(end>start,'Each drain must observe completion.');
    assert.ok(events.slice(start+1,end).every(event=>!['tick','pump'].includes(event[0])),
      'A drain must wait without submitting frames or pumping worker uploads.');
  }
  assert.deepEqual(events.slice(-2),[['drained',783],['free',783]],'Release the device only after the final queue drain.');
  assert.equal(report.frame.frames,3,'Drain waits must stay outside measured frame gaps.');
  assert.equal(report.cpu.frames,3); assert.equal(report.setup.gpuQueueDrained,true);
}
Promise.all([verifyMetadata(),verifyGpuDrains()]).then(()=>console.log('PASS: unavailable timing, A/A noise, mean/percentile gates, raw-preserving reanalysis, pose parity, workloads, GPU drains and build fingerprints.'))
  .catch(error=>{console.error(error);process.exitCode=1;});
