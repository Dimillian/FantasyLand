import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import init, {StreamGenerator, Game} from '../dist/pkg/fantasy_land.js';
await init({module_or_path:await fs.readFile(new URL('../dist/pkg/fantasy_land_bg.wasm',import.meta.url))});
for(const name of ['set_async_streaming','next_stream_job','accept_stream_result','pending_chunks']) assert.equal(typeof Game.prototype[name],'function');
const generator=new StreamGenerator(1337);
for(const kind of [0,1,2,3,4]) {
  const x=kind===4 ? -227 : kind>=2 ? -8 : -57;
  const z=kind===4 ? 1219 : kind>=2 ? 38 : 304;
  const packet=generator.generate(kind,x,z,0,1);
  assert.deepEqual(packet,generator.generate(kind,x,z,0,1),'worker generation is deterministic');
  const view=new DataView(packet.buffer,packet.byteOffset,packet.byteLength);
  assert.equal(view.getUint32(0,true),0x46534c31);
  const count=view.getUint32(4,true);
  if(kind===4) {
    assert.equal(count,0xffffffff);assert.equal(view.getUint32(8,true),1337);
    assert.equal(view.getFloat32(12,true),x*48);assert.equal(view.getFloat32(16,true),z*48);
    const plants=view.getUint32(44,true);assert.ok(plants>0 && plants<=1024);
    assert.equal(packet.length,48+plants*32+121*4);
  } else {
    assert.equal(count,kind<=2?2:1);
    let offset=8;
    for(let m=0;m<count;m++) {
      const nv=view.getUint32(offset+24,true),ni=view.getUint32(offset+28,true);
      assert.equal(nv%40,0);assert.equal(ni%12,0);offset+=32;
      for(let v=0;v<nv;v+=40) {
        for(const f of [0,4,8,16,20,24,28,36]) assert.ok(Number.isFinite(view.getFloat32(offset+v+f,true)));
        const material=view.getFloat32(offset+v+28,true);
        if(kind===2) assert.equal(material,m===0?7:8,'horizon passes contain only their own material');
        if(kind===1 && m===1) {
          const layer=view.getFloat32(offset+v+36,true);
          assert.ok(layer<5 || layer>9,'reflection proxy avoids fine alpha foliage');
        }
      }
      offset+=nv;
      for(let i=0;i<ni;i+=4) assert.ok(view.getUint32(offset+i,true)<nv/40);
      offset+=ni;
    }
    assert.equal(offset,packet.length);
  }
  console.log(`WASM worker packet ${kind}: ${packet.length} bytes, deterministic and valid`);
}
generator.free();
