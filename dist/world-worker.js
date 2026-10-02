// One worker owns a deterministic Rust world and packs meshes away from input
// and rendering. Transfer buffers rather than cloning thousands of JS objects.
import init, { StreamGenerator } from './pkg/fantasy_land.js?v=thunder-sync-1';
let generator;
self.onmessage = async ({data}) => {
  try {
    if (data.type === 'init') {
      await init({module_or_path:new URL('./pkg/fantasy_land_bg.wasm?v=thunder-sync-1', import.meta.url)});
      generator = new StreamGenerator(data.seed);
      self.postMessage({type:'ready'});
    } else if (data.type === 'generate' && generator) {
      const [ticket,kind,x,z,lod,detail] = data.job;
      const bytes = generator.generate(kind,x,z,lod,detail);
      self.postMessage({type:'mesh',ticket:ticket>>>0,bytes},[bytes.buffer]);
    } else if (data.type === 'generateGi' && generator) {
      const [x,y,z] = data.origin;
      const bytes = generator.generate_gi(x,y,z,new Uint32Array(data.doorIds),new Float32Array(data.doorAngles));
      self.postMessage({type:'gi',ticket:data.ticket>>>0,bytes},[bytes.buffer]);
    }
  } catch (error) {
    self.postMessage({type:'error',message:String(error?.message || error)});
  }
};
