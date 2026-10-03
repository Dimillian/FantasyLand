import {equipmentRecipe} from './equipment-items.mjs';
import {createEquipmentSprite} from './equipment-sprites.mjs';
export const defaultEquipmentLoadout=()=>({mainHand:equipmentRecipe(41,'sword'),offHand:equipmentRecipe(19,'shield')});
export class EquipmentSpriteCache{
  constructor(loadout=defaultEquipmentLoadout()){
    this.cache=new Map();this.cacheBytes=0;this.pending=new Map();this.baking=false;this.worker=null;this.bakeGroup=null;this.generation=0;this.setLoadout(loadout);
  }
  setLoadout(loadout){
    this.loadout={mainHand:loadout.mainHand||null,offHand:loadout.offHand||null};this.generation++;
    this.pending.clear();this.bakeGroup=null;
    for(const entry of this.cache.values())entry.image.close?.();this.cache.clear();this.cacheBytes=0;
  }
  initBaker(){
    if(this.worker||this.workerFailed)return;
    if(typeof Worker==='undefined'||typeof OffscreenCanvas==='undefined'){this.workerFailed=true;return;}
    this.worker=new Worker(new URL('./equipment-worker.mjs?v=encounters-1',import.meta.url),{type:'module'});
    this.worker.onmessage=({data:m})=>{
      this.baking=false;this.inFlight=null;
      if(m.generation!==this.generation){m.image?.close();this.pumpBakes();return;}
      if(m.error){this.failWorker();return;}
      m.image.originX=m.originX;m.image.originY=m.originY;this.storeSprite(m.key,m.image,m.meta);this.pumpBakes();
    };
    this.worker.onerror=()=>this.failWorker();
  }
  failWorker(){this.workerFailed=true;this.worker?.terminate();this.worker=null;this.pending.clear();this.baking=false;this.inFlight=null;}
  storeSprite(key,image,meta={}){
    const old=this.cache.get(key);if(old){this.cacheBytes-=old.image.width*old.image.height*4;old.image.close?.();}
    this.cache.set(key,{image,...meta});this.cacheBytes+=image.width*image.height*4;
    while(this.cacheBytes>24*1048576&&this.cache.size>2){const first=this.cache.keys().next().value,old=this.cache.get(first).image;this.cache.delete(first);this.cacheBytes-=old.width*old.height*4;old.close?.();}
  }
  request(slot,pose,width,light){
    const recipe=this.loadout[slot];if(!recipe)return null;
    const hand=slot==='offHand'?'left':'right',shield=recipe.rig==='shield';
    pose=Math.max(0,Math.min(shield?12:32,Math.round(pose)));
    const key=`${recipe.key}|${hand}|${pose}|${width}|${light}`;
    return {key,recipe,generation:this.generation,meta:{recipeKey:recipe.key,hand,pose,width,light},options:{crop:true,width,light,hand,swing:shield?0:pose*.58/32,guard:shield?pose/12:0}};
  }
  pumpBakes(){
    if(this.baking||!this.worker||!this.pending.size)return;
    const key=this.pending.keys().next().value,m=this.pending.get(key);this.pending.delete(key);this.baking=true;this.inFlight=key;this.worker.postMessage(m);
  }
  sprite(slot,pose,width,light){
    const job=this.request(slot,pose,width,light);if(!job)return null;
    if(this.cache.has(job.key))return this.cache.get(job.key).image;
    this.initBaker();
    if(this.worker){
      if(this.inFlight!==job.key){this.pending.delete(job.key);this.pending=new Map([[job.key,job],...this.pending]);}
      this.pumpBakes();
      let nearest=null,distance=Infinity;
      for(const entry of this.cache.values()){
        const d=Math.abs(entry.pose-pose)+Math.abs(entry.light-light)*20;
        if(entry.recipeKey===job.recipe.key&&entry.hand===job.meta.hand&&entry.width===width&&d<distance){distance=d;nearest=entry.image;}
      }
      return nearest;
    }
    const image=createEquipmentSprite(job.recipe,undefined,job.options);this.storeSprite(job.key,image,job.meta);return image;
  }
  warmSprites(width,light){
    this.initBaker();if(!this.worker)return;
    const group=`${this.generation}:${width}:${light}`;if(this.bakeGroup===group)return;
    this.bakeGroup=group;this.pending.clear();
    for(let pose=0;pose<=32;pose++)for(const slot of ['mainHand','offHand']){
      const r=this.loadout[slot];if(!r||pose>(r.rig==='shield'?12:32))continue;
      const job=this.request(slot,pose,width,light);if(!this.cache.has(job.key)&&this.inFlight!==job.key)this.pending.set(job.key,job);
    }
    this.pumpBakes();
  }
  dispose(){this.worker?.terminate();this.worker=null;this.setLoadout({});}
}
