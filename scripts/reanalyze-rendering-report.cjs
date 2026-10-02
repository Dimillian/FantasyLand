#!/usr/bin/env node
// CPU-only reanalysis with the browser's actual statistics and gate functions.
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const crypto=require('node:crypto');
const gatePath=path.join(__dirname,'../dist/rendering-review.js');
const gateSource=fs.readFileSync(gatePath,'utf8');
const sha256=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');

function reanalyzeReports(report,control) {
  const elements=new Map();
  const context=vm.createContext({document:{hidden:false,getElementById(id){
    if(!elements.has(id))elements.set(id,{value:id==='scene'?'forest':'full',style:{setProperty(){}},textContent:''});
    return elements.get(id);
  }},fetch:async()=>({ok:false}),requestAnimationFrame(){},console,Math,JSON,Number,Array,Map,Set,Error,
  performance:{now:()=>0},savedReport:report,savedControl:control});
  // Loading the UI defines the exact production functions; all I/O is stubbed
  // and no measurement, worker, renderer or GPU API is invoked.
  vm.runInContext(gateSource,context,{filename:gatePath});
  const result=vm.runInContext(`(()=>{
    const report=savedReport;
    const hasRaw=report.runs?.length>0;
    const blocks=hasRaw?blockComparisons(report.runs,report.options.blocks)
      :report.blocks.map(block=>({...block,ratios:resolvedBlockRatios(block)}));
    const noise=matchingNoiseReport(savedControl,report.options);
    const validation=performanceGate(report.options,blocks,noise);
    return {options:report.options,summary:hasRaw?[summarize(report.runs,'A'),summarize(report.runs,'B')]:report.summary,
      blocks,validation,gate:validation.gate,compatibleNoiseControl:Boolean(noise)};
  })()`,context);
  return JSON.parse(JSON.stringify(result));
}

function main(args) {
  if(args.length!==2 && !(args.length===4 && args[2]==='--out')) {
    throw Error('Usage: node scripts/reanalyze-rendering-report.cjs REPORT.json CONTROL.json [--out ANALYSIS.json]');
  }
  const reportPath=fs.realpathSync(args[0]),controlPath=fs.realpathSync(args[1]);
  const reportBytes=fs.readFileSync(reportPath),controlBytes=fs.readFileSync(controlPath);
  const analysis={
    sources:{report:{path:reportPath,sha256:sha256(reportBytes)},control:{path:controlPath,sha256:sha256(controlBytes)},
      gate:{path:gatePath,sha256:sha256(gateSource)}},
    ...reanalyzeReports(JSON.parse(reportBytes.toString('utf8')),JSON.parse(controlBytes.toString('utf8')))
  };
  const output=JSON.stringify(analysis,null,2)+'\n';
  if(args[2]==='--out') {
    const requested=path.resolve(args[3]);
    const resolved=fs.existsSync(requested)?fs.realpathSync(requested):requested;
    if(resolved===reportPath || resolved===controlPath)throw Error('The analysis output must differ from both source reports; raw samples are read-only.');
    fs.mkdirSync(path.dirname(requested),{recursive:true});fs.writeFileSync(requested,output);
  } else process.stdout.write(output);
}

module.exports={reanalyzeReports,main};
if(require.main===module) {
  try{main(process.argv.slice(2));}catch(error){console.error(error.message);process.exitCode=1;}
}
