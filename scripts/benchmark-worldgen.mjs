import fs from 'node:fs';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
const modulePath=process.argv[2]||'dist/pkg/fantasy_land.js';
const wasmPath=process.argv[3]||'dist/pkg/fantasy_land_bg.wasm';
const engine=await import(pathToFileURL(path.resolve(modulePath)));
await engine.default({module_or_path:fs.readFileSync(wasmPath)});
const results=[];
for(const seed of [0,42,1337,4294967295]){
  let start=performance.now();const g=new engine.StreamGenerator(seed);const construction=performance.now()-start;
  const row={seed,constructionMs:Math.round(construction)};
  if(g.map_layer_data){
    start=performance.now();const pixels=g.map_layer_data(0,0,384000,384,1);row.atlasMs=Math.round(performance.now()-start);
    fs.mkdirSync('output/worldgen-hardening',{recursive:true});
    if(seed===1337)fs.writeFileSync('output/worldgen-hardening/elevation.rgba',pixels);
    start=performance.now();g.map_features(0,0,384000);row.featuresMs=Math.round(performance.now()-start);
  }
  g.free();results.push(row);
}
console.log(JSON.stringify(results,null,2));
