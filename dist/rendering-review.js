const $ = id => document.getElementById(id);
const stages = {full:'All three upgrades', shadows:'AO + cascaded shadows', ao:'Ambient occlusion', off:'Legacy rendering'};
const masks = {off:0, ao:1, shadows:3, full:7};
let cancelled = false;
let lastReport = null;

function reportName(options) {
  const stage = options.control === 'main-main' ? 'off' : options.stage;
  return 'browser-' + options.scene + '-' + stage + '-' + options.resolution + '-' + options.workload + '-' + options.control + '.json';
}
function reportContext(report) {
  const o = report.options;
  return o.scene + ' · ' + o.resolution + 'p · ' + o.workload + ' · ' + o.control + ' · ' + (o.control === 'main-main' ? 'baseline' : stages[o.stage]);
}
function displayGate(report) {
  $('gate').textContent = report.gate + ' [' + reportContext(report) + ']';
}
function adapterMetadata(info) {
  if (!info) return null;
  return Object.fromEntries(['vendor','architecture','device','description']
    .map(key=>[key,typeof info[key]==='string'?info[key]:'']));
}
async function hashBytes(bytes) {
  if (!globalThis.crypto?.subtle) throw Error('SHA-256 is unavailable; open the local review on localhost or a secure browser origin');
  const digest=await crypto.subtle.digest('SHA-256',bytes);
  return Array.from(new Uint8Array(digest),value=>value.toString(16).padStart(2,'0')).join('');
}
async function assetFingerprint(url) {
  const response=await fetch(url,{cache:'no-store'});
  if (!response.ok) throw Error('Benchmark build is unavailable: '+url);
  const bytes=await response.arrayBuffer();
  return {url,bytes:bytes.byteLength,sha256:await hashBytes(bytes)};
}
async function runtimeFingerprints() {
  const [main,candidate]=await Promise.all([
    assetFingerprint('/comparison/baseline/source/dist/pkg/fantasy_land_bg.wasm'),
    assetFingerprint('./pkg/fantasy_land_bg.wasm')
  ]);
  return {main,candidate};
}
async function show() {
  const scene = $('scene').value, stage = $('stage').value;
  const a = '/comparison/baseline/captures/' + scene + '.png';
  const b = '/comparison/candidate/' + stage + '/' + scene + '.png';
  $('before').src = a; $('after').src = b;
  $('before-link').href = a; $('after-link').href = b;
  $('after-label').textContent = 'After · ' + stages[stage];
  try {
    const response = await fetch('/comparison/baseline/captures/' + scene + '.json');
    if (!response.ok) throw Error('Fixture metadata missing');
    const m = await response.json();
    if ($('scene').value !== scene) return;
    $('metadata').textContent = 'Seed ' + m.seed + ' · 1280 × 720 · ' + m.internal[1] + 'p internal · Balanced · FXAA · ' +
      m.fixture.hour.toFixed(1) + 'h · frozen camera [' + m.fixture.eye.map(v=>v.toFixed(2)).join(', ') + ']';
  } catch (error) {
    $('metadata').textContent = error.message;
  }
  if (lastReport) displayGate(lastReport);
}
$('scene').onchange = show;
$('stage').onchange = show;
$('split').oninput = () => $('comparison').style.setProperty('--split', $('split').value + '%');
show();

async function previous() {
  try {
    const response = await fetch('/comparison/browser-report.json');
    if (!response.ok) return;
    lastReport = await response.json();
    $('results').textContent = JSON.stringify(lastReport.summary, null, 2);
    displayGate(lastReport);
  } catch {}
}
previous();

const frame = () => new Promise(resolve=>requestAnimationFrame(resolve));
function checkRun() {
  if (cancelled) throw Error('Stopped; incomplete measurements were not saved');
  if (document.hidden) throw Error('Tab hidden; benchmark invalid');
}
function requireGpuDrain(game,version) {
  if (typeof game.begin_gpu_drain !== 'function' || typeof game.gpu_drained !== 'function') {
    throw Error(version+' GPU drain API missing; rebuild both preserved baseline and candidate benchmark harnesses');
  }
}
async function drainGpu(game,version) {
  requireGpuDrain(game,version); checkRun();
  game.begin_gpu_drain();
  const deadline=performance.now()+120000;
  while (!game.gpu_drained()) {
    await frame(); checkRun();
    if (performance.now()>deadline) throw Error(version+' GPU queue did not drain within 120 seconds; comparison invalid');
  }
}
function stats(values) {
  const finite = values.filter(Number.isFinite);
  if (!finite.length) return null;
  const sorted = [...finite].sort((a,b)=>a-b);
  return {
    meanMs:finite.reduce((a,b)=>a+b,0)/finite.length,
    medianMs:sorted[Math.floor(sorted.length*.5)],
    p95Ms:sorted[Math.floor(sorted.length*.95)],
    p99Ms:sorted[Math.floor(sorted.length*.99)],
    hitchesOver33ms:finite.filter(t=>t>33.4).length,
    frames:finite.length
  };
}
function pose(state) {
  return {eye:Array.from(state.cameraPosition ?? [state.x,state.altitude+1.72,state.z]),
    eyeSource:state.cameraPosition ? 'rendered camera' : 'feet plus standing height',
    yaw:state.yaw,pitch:state.pitch,hour:state.dayTime};
}
function sceneState(game) {
  const state = game.state();
  return {
    pose:pose(state),
    internal:Array.from(game.render_resolution()),
    triangles:state.triangleCount, meshMB:state.meshMegabytes,
    people:state.nearbyPeople, pending:state.streamingPending,
    coverDensity:state.groundCoverDensity, coverInstances:state.coverInstances,
    lightingMode:state.lightingMode ?? 0, lightingBytes:state.lightingBytes ?? null
  };
}
function near(a,b,tolerance=0.0001) {
  return Number.isFinite(a) && Number.isFinite(b) && Math.abs(a-b) <= tolerance;
}
function stateMismatches(a,b,includeGeometry=true) {
  const errors = [];
  for (let i=0;i<3;i++) if (!near(a.pose.eye[i],b.pose.eye[i])) errors.push('eye['+i+']');
  for (const key of ['yaw','pitch','hour']) if (!near(a.pose[key],b.pose[key])) errors.push(key);
  if (a.internal.join(',') !== b.internal.join(',')) errors.push('internal resolution');
  if (a.coverDensity !== b.coverDensity) errors.push('cover density');
  if (includeGeometry) {
    for (const key of ['triangles','people','coverInstances']) if (a[key] !== b[key]) errors.push(key);
    if (!near(a.meshMB,b.meshMB,0.001)) errors.push('mesh bytes');
    if (a.pending || b.pending) errors.push('pending geometry');
  }
  return errors;
}
function makeWorker(game,baseline) {
  const worker = new Worker(baseline ? '/comparison/baseline/source/dist/world-worker.js' : './world-worker.js', {type:'module'});
  worker.results = []; worker.ready = false; worker.outstanding = 0; worker.gi = false;
  worker.onmessage = ({data}) => {
    if (data.type === 'ready') worker.ready = true;
    else if (data.type === 'error') worker.error = data.message;
    else worker.results.push(data);
  };
  worker.onerror = event => {worker.error = event.message || 'Worker failed';};
  worker.postMessage({type:'init', seed:1337});
  // Queue generation immediately; setup never depends on synchronous mesh work.
  game.set_async_streaming(true);
  return worker;
}
function pump(game,worker) {
  if (worker.error) throw Error(worker.error);
  for (const result of worker.results.splice(0)) {
    if (result.type === 'mesh') {
      if (!game.accept_stream_result(result.ticket,result.bytes)) throw Error('Invalid streamed mesh packet');
      worker.outstanding--;
    } else if (result.type === 'gi') {
      if (!game.accept_gi_result(result.ticket,result.bytes)) throw Error('Invalid indirect-light packet');
      worker.gi = false;
    }
  }
  if (!worker.ready) return;
  while (worker.outstanding < 2) {
    const job = game.next_stream_job();
    if (!job.length) break;
    worker.postMessage({type:'generate',job});
    worker.outstanding++;
  }
  if (!worker.gi && game.next_gi_job) {
    const job = game.next_gi_job();
    if (job) {worker.gi = true; worker.postMessage({type:'generateGi',...job});}
  }
}
async function idleCadence() {
  const gaps = []; let previous = await frame();
  for (let i=0;i<30;i++) {
    const now = await frame(); checkRun();
    gaps.push(now-previous); previous = now;
  }
  return stats(gaps);
}
function advanceWorkload(game,fixture,workload,index,count) {
  if (workload === 'sweep') game.face(fixture.yaw+Math.sin(index/Math.max(count-1,1)*Math.PI*2)*.2,fixture.pitch);
  if (workload === 'transition') {
    const first = Math.floor(count/3), second = Math.floor(count*2/3);
    if (index === first || index === second) {
      game.teleport(fixture.eye[0]+(index === first ? 25 : 0),fixture.eye[2]);
      game.face(fixture.yaw,fixture.pitch);
    }
  }
  game.tick(workload === 'walk' ? 1/60 : 0,workload === 'walk' ? 1 : 0,0,false,false);
}
async function measure(version,role,block,fixture,options,mask) {
  const baseline = version === 'main';
  const fingerprint=options.runtimeWasm?.[version];
  const revision=fingerprint?'?sha256='+fingerprint.sha256:'';
  const pkg = (baseline ? '/comparison/baseline/source/dist/pkg/fantasy_land.js' : './pkg/fantasy_land.js')+revision;
  const mod = await import(pkg);
  await mod.default({module_or_path:new URL((baseline ? '/comparison/baseline/source/dist/pkg/fantasy_land_bg.wasm' : './pkg/fantasy_land_bg.wasm')+revision,location.href)});
  checkRun();
  const cadence = await idleCadence();
  let game = await mod.Game.create($('measure'),1337);
  let worker = null;
  let canDrain = false;
  try {
    requireGpuDrain(game,version); canDrain=true;
    game.set_quality(1); game.set_render_resolution(options.resolution);
    game.set_antialiasing(1); game.set_filter(1,.85); game.set_ground_cover_density(4);
    game.set_shadows(true); game.set_reflections(true); game.set_enclosure(true);
    if (!baseline && typeof game.set_lighting_mode !== 'function') throw Error('Candidate renderer API missing; rebuild or reload the local preview');
    if (game.set_lighting_mode) game.set_lighting_mode(mask);
    worker = makeWorker(game,baseline);
    game.teleport(fixture.eye[0],fixture.eye[2]); game.face(fixture.yaw,fixture.pitch);
    game.set_time(fixture.hour); game.set_weather_mode(fixture.weather);
    // Thirty simulated seconds at the engine's supported 50ms step. Batch GPU
    // submission during setup, then wait for streamed geometry and settle.
    for (let i=0;i<600;i++) {
      game.tick(.05,0,0,false,false);
      if (i%30 === 29) {await frame(); checkRun(); pump(game,worker);}
    }
    await drainGpu(game,version);
    game.set_time(fixture.hour); game.set_weather_paused(true);
    let frames=0, settled=0;
    while (settled<180) {
      await frame(); checkRun(); pump(game,worker); game.tick(0,0,0,false,false);
      const pending = game.pending_chunks() || worker.outstanding || worker.gi || !worker.ready;
      settled = pending ? 0 : settled+1;
      if (++frames > 7200) throw Error('Streaming did not finish');
      if (frames%120 === 0) $('progress').textContent = version+' · warming '+frames+' frames';
    }
    await drainGpu(game,version);
    const initialState = sceneState(game);
    const expectedInternal = [Math.round(options.resolution*1280/720), options.resolution];
    if (initialState.internal.join(',') !== expectedInternal.join(',')) throw Error('Actual internal resolution does not match the requested fixed resolution');
    if (!baseline && initialState.lightingMode !== mask) throw Error('Candidate lighting mode did not apply');

    const gaps=[], cpu=[], gpu=[], gpuFresh=[], samples=[];
    let previous = await frame(), lastGpuSampleId = game.state().gpuSampleId ?? null;
    for (let i=0;i<options.frames;i++) {
      if (i) {
        const now=await frame(); checkRun(); gaps.push(now-previous); previous=now;
      }
      const start=performance.now();
      pump(game,worker);
      advanceWorkload(game,fixture,options.workload,i,options.frames);
      cpu.push(performance.now()-start);
      if (i%30 === 0) {
        const state=game.state(), sampleId=state.gpuSampleId ?? null;
        if (Number.isFinite(state.gpuRenderMs)) {
          gpu.push(state.gpuRenderMs);
          if (sampleId !== null && sampleId !== lastGpuSampleId) gpuFresh.push(state.gpuRenderMs);
        }
        lastGpuSampleId=sampleId;
        samples.push({frame:i,gpuSampleId:sampleId,gpuLatestMs:state.gpuRenderMs ?? null,
          pending:state.streamingPending,lightingBytes:state.lightingBytes ?? null,
          meshMB:state.meshMegabytes,passes:state.gpuTimings});
      }
      if (i%120 === 0) $('progress').textContent=version+' · '+options.workload+' · '+i+'/'+options.frames+' frames';
    }
    const finalTimestamp=await frame(); checkRun(); gaps.push(finalTimestamp-previous);
    const finalState=sceneState(game);
    return {version,role,block,resolution:options.resolution,count:options.frames,workload:options.workload,mask,fixture,
      initialState,finalState,nativeEyeDifference:initialState.pose.eye.map((value,index)=>value-fixture.eye[index]),
      cadence,frame:stats(gaps),cpu:stats(cpu),gpu:stats(gpu),gpuFresh:stats(gpuFresh),
      gpuNote:'Latest asynchronous snapshots may repeat. Cross-version GPU spans have different pass coverage and are diagnostic.',
      setup:{weatherSeconds:30,weatherStepSeconds:.05,settledFrames:180,totalSettleFrames:frames,frozenAnimationSeconds:30,gpuQueueDrained:true},
      raw:{gaps,cpu,gpu,gpuFresh,samples}};
  } finally {
    try {
      if (canDrain) await drainGpu(game,version);
    } finally {
      if (worker) {worker.onmessage=null; worker.onerror=null; worker.terminate();}
      game.free(); game=null;
      // Release the previous surface before another device configures this canvas.
      await frame();
    }
  }
}
function summarize(runs,role) {
  const selected=runs.filter(run=>run.role===role);
  return {role,version:selected[0].version,
    frame:stats(selected.flatMap(run=>run.raw.gaps)),
    cpu:stats(selected.flatMap(run=>run.raw.cpu)),
    gpu:stats(selected.flatMap(run=>run.raw.gpu)),
    gpuFresh:stats(selected.flatMap(run=>run.raw.gpuFresh))};
}
const metricPaths={frameMean:['frame','meanMs'],frameMedian:['frame','medianMs'],frameP95:['frame','p95Ms'],frameP99:['frame','p99Ms'],cpuMean:['cpu','meanMs'],cpuP95:['cpu','p95Ms']};
function resolvedBlockRatios(block) {
  return Object.fromEntries(Object.entries(metricPaths).map(([name,[group,statistic]])=>{
    const stored=block.ratios?.[name];
    if (Number.isFinite(stored)) return [name,stored];
    const a=block.a?.[group]?.[statistic],b=block.b?.[group]?.[statistic];
    return [name,Number.isFinite(a) && a>0 && Number.isFinite(b)?b/a-1:null];
  }));
}
function resolvedNoiseEnvelope(noise) {
  const envelope={...noise?.validation?.noiseEnvelope};
  if (!Number.isFinite(envelope.frameMean) && noise?.blocks?.length) {
    const ratios=noise.blocks.map(block=>resolvedBlockRatios(block).frameMean);
    if (ratios.every(Number.isFinite)) envelope.frameMean=Math.max(...ratios.map(Math.abs));
  }
  return envelope;
}
function blockComparisons(runs,blocks) {
  const output=[];
  for (let block=0;block<blocks;block++) {
    const selected=runs.filter(run=>run.block===block);
    const a=summarize(selected,'A'),b=summarize(selected,'B'),ratios={};
    for (const [name,[group,statistic]] of Object.entries(metricPaths)) {
      ratios[name]=a[group][statistic]>0 ? b[group][statistic]/a[group][statistic]-1 : null;
    }
    const initialErrors=selected.slice(1).flatMap(run=>stateMismatches(selected[0].initialState,run.initialState));
    const finalErrors=selected.slice(1).flatMap(run=>stateMismatches(selected[0].finalState,run.finalState,false));
    output.push({block,a,b,ratios,initialStateMismatches:[...new Set(initialErrors)],finalPoseMismatches:[...new Set(finalErrors)]});
  }
  return output;
}
function matchingNoiseReport(report,options) {
  if (!report?.options || report.options.control!=='main-main') return null;
  const match=['scene','resolution','workload','frames'].every(key=>report.options[key]===options[key]);
  const drained=report.runs?.length>0 && report.runs.every(run=>run.setup?.gpuQueueDrained===true);
  return match && drained && report.options.blocks>=2 && report.validation?.pairedStateMatch ? report : null;
}
async function fetchNoise(options) {
  const controlOptions={...options,control:'main-main',stage:'off'};
  try {
    const response=await fetch('/comparison/'+reportName(controlOptions));
    if (!response.ok) return null;
    const report=await response.json();
    return matchingNoiseReport(report,options);
  } catch {return null;}
}
function performanceGate(options,blocks,noise) {
  const pairedStateMatch=blocks.every(block=>!block.initialStateMismatches.length && !block.finalPoseMismatches.length);
  if (!pairedStateMatch) return {status:'invalid',pairedStateMatch,gate:'Performance comparison invalid: browser A/B poses or scene state differ.'};
  blocks=blocks.map(block=>({...block,ratios:resolvedBlockRatios(block)}));
  if (blocks.some(block=>Object.values(block.ratios).some(value=>!Number.isFinite(value)))) {
    return {status:'inconclusive',pairedStateMatch,gate:'Performance gate incomplete: per-block frame/CPU metrics are missing.'};
  }
  if (options.control !== 'main-candidate') {
    const envelope={};
    for (const name of Object.keys(metricPaths)) envelope[name]=Math.max(...blocks.map(block=>Math.abs(block.ratios[name])));
    return {status:'control',pairedStateMatch,noiseEnvelope:envelope,
      gate:'A/A control recorded. ' + (options.blocks>=2 ? 'Per-block variation is available for matching paired comparisons.' : 'At least two blocks are needed to establish repeatability.')};
  }
  if (!noise || options.blocks<2) return {status:'inconclusive',pairedStateMatch,
    gate:'Performance gate incomplete: run at least two matching baseline A/A blocks and two paired blocks.'};
  const envelope=resolvedNoiseEnvelope(noise),regressions={};
  if (!envelope || Object.keys(metricPaths).some(name=>!Number.isFinite(envelope[name]))) {
    return {status:'inconclusive',pairedStateMatch,gate:'Performance gate incomplete: A/A noise metrics are missing.'};
  }
  for (const name of Object.keys(metricPaths)) {
    const threshold=envelope[name];
    const floatingTolerance=1e-9*Math.max(1,Math.abs(threshold));
    // The baseline control defines this run's noise floor. Require the excess
    // in at least two blocks and a majority of blocks; never infer it by pooling.
    // Ignore arithmetic roundoff only, including a zero A/A median envelope.
    const slower=blocks.filter(block=>Number.isFinite(block.ratios[name]) && block.ratios[name]-threshold>floatingTolerance).map(block=>block.block);
    if (slower.length>=2 && slower.length>blocks.length/2) regressions[name]={noiseFloorPercent:threshold*100,slowerBlocks:slower};
  }
  const regression=Object.keys(regressions).length>0;
  return {status:regression?'regression':'provisional',pairedStateMatch,noiseEnvelope:envelope,regressions,
    gate:regression ? 'Performance gate: repeatable slowdown above measured A/A variation; optimization required.' :
      'No repeatable browser frame/CPU slowdown above this A/A control. GPU headroom and other scenes/workloads remain unverified.'};
}
function disableControls(disabled) {
  for (const id of ['scene','stage','resolution','frames','blocks','workload','benchmark-control','run']) if ($(id)) $(id).disabled=disabled;
  $('stop').disabled=!disabled;
}
$('stop').onclick=()=>{cancelled=true;};
$('run').onclick=async()=>{
  cancelled=false; disableControls(true); $('measure').style.display='block';
  $('gate').textContent='Performance comparison running; no pass result yet.';
  const options={resolution:Number($('resolution').value),frames:Number($('frames').value),
    blocks:Number($('blocks').value),workload:$('workload').value,scene:$('scene').value,
    stage:$('stage').value,control:$('benchmark-control')?.value || 'main-candidate',mask:masks[$('stage').value]};
  const runs=[];
  try {
    const manifestUrl='/comparison/baseline/captures/'+options.scene+'.json';
    const response=await fetch(manifestUrl,{cache:'no-store'});
    if (!response.ok) throw Error('Baseline fixture metadata missing');
    const manifestBytes=await response.arrayBuffer();
    const report=JSON.parse(new TextDecoder().decode(manifestBytes));
    const fixtureManifest={url:manifestUrl,bytes:manifestBytes.byteLength,sha256:await hashBytes(manifestBytes)};
    options.runtimeWasm=await runtimeFingerprints();
    checkRun();
    let adapter=null;
    try {const a=await navigator.gpu.requestAdapter({powerPreference:'high-performance'}); adapter=adapterMetadata(a?.info);} catch {}
    for (let block=0;block<options.blocks;block++) {
      for (const role of ['A','B','B','A']) {
        const version=options.control==='main-main'?'main':options.control==='candidate-candidate'?'candidate':role==='A'?'main':'candidate';
        runs.push(await measure(version,role,block,report.fixture,options,version==='candidate'?options.mask:0));
        $('results').textContent=JSON.stringify(runs.map(({version,role,block,frame,cpu,gpu})=>({version,role,block,frame,cpu,gpu})),null,2);
      }
    }
    const blocks=blockComparisons(runs,options.blocks);
    const noise=options.control==='main-candidate'?await fetchNoise(options):null;
    const validation=performanceGate(options,blocks,noise);
    const result={baselineCommit:'76f77ea',browser:navigator.userAgent,adapter,fixtureManifest,options,
      summary:[summarize(runs,'A'),summarize(runs,'B')],blocks,validation,runs,
      noiseReport:noise?reportName({...options,control:'main-main',stage:'off'}):null,
      gpuComparableAcrossVersions:false,gate:validation.gate};
    const saved=await fetch('/comparison/report?name='+encodeURIComponent(reportName(options)),
      {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(result)});
    if (!saved.ok) throw Error('Comparison finished but the local server did not save the report');
    lastReport=result; displayGate(result);
    $('results').textContent=JSON.stringify({summary:result.summary,blocks:result.blocks,validation},null,2);
    $('progress').textContent='Comparison saved locally: '+reportName(options);
  } catch (error) {
    $('progress').textContent=error.message;
    $('gate').textContent='Comparison incomplete; no new performance pass is available.';
  } finally {
    disableControls(false); $('measure').style.display='none';
  }
};
