// Reconstructible map data: bounded LRU RAM cache plus optional IndexedDB cache.
// Whole viewport packets include raster AND static features, not player markers.
export class AtlasCache {
  constructor(namespace, {memoryBytes=16*1024*1024, diskBytes=32*1024*1024,databaseName='fantasyland.atlas'}={}) {
    this.databaseName=databaseName;this.namespace=namespace;this.memoryBytes=memoryBytes;this.diskBytes=diskBytes;
    this.entries=new Map();this.bytes=0;this.db=null;this.stats={memoryHits:0,diskHits:0,misses:0};
  }
  key(q) {return `${this.namespace}:${q.layer}:${q.res}:${q.x}:${q.z}:${q.span}`;}
  remember(key,record) {
    if(this.entries.has(key)){this.bytes-=this.entries.get(key).size;this.entries.delete(key);}
    if(record.size>this.memoryBytes)return;
    this.entries.set(key,record);this.bytes+=record.size;
    while(this.bytes>this.memoryBytes){const k=this.entries.keys().next().value;this.bytes-=this.entries.get(k).size;this.entries.delete(k);}
  }
  async database() {
    if(typeof indexedDB==='undefined')return null;
    if(!this.db)this.db=new Promise(resolve=>{
      const request=indexedDB.open(this.databaseName,2);
      request.onupgradeneeded=()=>{const db=request.result;if(db.objectStoreNames.contains('views'))db.deleteObjectStore('views');db.createObjectStore('views',{keyPath:'key'});db.createObjectStore('metadata',{keyPath:'key'});};
      request.onsuccess=()=>resolve(request.result);request.onerror=()=>resolve(null);request.onblocked=()=>resolve(null);
    });
    return this.db;
  }
  async get(q) {
    const key=this.key(q),memory=this.entries.get(key);
    if(memory){this.remember(key,memory);this.stats.memoryHits++;return {...memory,source:'memory'};}
    try {
      const db=await this.database();
      if(db){const record=await new Promise(resolve=>{const r=db.transaction('views').objectStore('views').get(key);r.onsuccess=()=>resolve(r.result);r.onerror=()=>resolve(null);});
        if(record && record.pixels?.byteLength===q.res*q.res*4){this.remember(key,record);this.stats.diskHits++;return {...record,source:'disk'};}}
    }catch(_){} // Cache availability must never affect the world save.
    this.stats.misses++;return null;
  }
  put(q,pixels,features) {
    const record={source:'generated',key:this.key(q),pixels:new Uint8Array(pixels),features,used:Date.now()};
    record.size=record.pixels.byteLength+JSON.stringify(features).length*2;
    this.remember(record.key,record);
    void this.persist(record);
    return record;
  }
  async persist(record) {
    try {
      if(record.size>this.diskBytes)return;
      const db=await this.database();if(!db)return;
      const tx=db.transaction(['views','metadata'],'readwrite'),store=tx.objectStore('views'),meta=tx.objectStore('metadata');store.put(record);meta.put({key:record.key,size:record.size,used:record.used});
      // Small bounded store; pruning all namespaces also expires old generators.
      const request=meta.getAll();
      request.onsuccess=()=>{
        const records=request.result.sort((a,b)=>b.used-a.used);let bytes=0;
        for(const item of records){bytes+=item.size;if(bytes>this.diskBytes){store.delete(item.key);meta.delete(item.key);}}
      };
      tx.onerror=()=>{};
    }catch(_){}
  }
}
