import {createEquipmentSprite} from './equipment-sprites.mjs';
self.onmessage=({data:m})=>{
  try{
    const canvas=createEquipmentSprite(m.recipe,()=>new OffscreenCanvas(1,1),m.options);
    const originX=canvas.originX,originY=canvas.originY,image=canvas.transferToImageBitmap();
    self.postMessage({key:m.key,generation:m.generation,meta:m.meta,image,originX,originY},[image]);
  }catch(error){self.postMessage({key:m.key,generation:m.generation,error:String(error)});}
};
