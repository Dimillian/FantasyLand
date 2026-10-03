import {rigEquipment} from './equipment-rig.mjs';
import {norm,cross,sub,add,mul} from './equipment-geometry.mjs';
export {equipmentRecipe,registerEquipment} from './equipment-items.mjs';
export {weaponPose} from './equipment-rig.mjs';
export function paintEquipment(ctx,recipe,pose={}){
  const options={width:746,height:420,light:1,...pose};
  return rasterEquipment(ctx,rigEquipment(recipe,{...pose,hand:pose.hand||(recipe.rig==='shield'?'left':'right')}),recipe,options);
}
// One-time software bake: a real depth buffer prevents fingers, straps and the
// forearm from drawing through each other. Textures are stable in object space.
const hashPixel=(x,y,seed)=>{let n=Math.imul(x|0,374761393)^Math.imul(y|0,668265263)^seed;n=Math.imul(n^(n>>>13),1274126177);return (n^(n>>>16))>>>0;};
function rasterEquipment(ctx,faces,r,{width,height,light}){
  const image=ctx.createImageData?.(width,height),pixels=image?.data;
  const zbuffer=pixels?new Float32Array(width*height).fill(Infinity):null;
  const bounds=[width,height,0,0],focal=height*.70;
  const project=p=>[width*.5+p[0]/Math.max(.13,p[2])*focal,height*.5-p[1]/Math.max(.13,p[2])*focal,1/p[2]];
  const lamp=norm([-.45,.75,-.65]),half=norm(add(lamp,[0,0,-1]));
  for(const face of faces){
    const {points,color,kind}=face;
    let normal=norm(cross(sub(points[1],points[0]),sub(points[2],points[0])));
    if(normal[2]>0)normal=mul(normal,-1);
    const metal=kind==='steel'||kind==='brass';
    const diffuse=Math.max(0,normal[0]*lamp[0]+normal[1]*lamp[1]+normal[2]*lamp[2]);
    const spec=metal?Math.pow(Math.max(0,normal[0]*half[0]+normal[1]*half[1]+normal[2]*half[2]),18)*.65:0;
    const shade=(.40+diffuse*.67)*light;
    for(let tri=1;tri<points.length-1;tri++){
      const verts=[points[0],points[tri],points[tri+1]],uv=[face.texcoords[0],face.texcoords[tri],face.texcoords[tri+1]],p=verts.map(project);
      const [a,b,c]=p,den=(b[1]-c[1])*(a[0]-c[0])+(c[0]-b[0])*(a[1]-c[1]);
      if(Math.abs(den)<.01)continue;
      const x0=Math.max(0,Math.floor(Math.min(a[0],b[0],c[0]))),x1=Math.min(width-1,Math.ceil(Math.max(a[0],b[0],c[0])));
      const y0=Math.max(0,Math.floor(Math.min(a[1],b[1],c[1]))),y1=Math.min(height-1,Math.ceil(Math.max(a[1],b[1],c[1])));
      if(x0>x1||y0>y1)continue;
      bounds[0]=Math.min(bounds[0],x0);bounds[1]=Math.min(bounds[1],y0);bounds[2]=Math.max(bounds[2],x1+1);bounds[3]=Math.max(bounds[3],y1+1);
      if(!pixels){ctx.fillStyle=`rgb(${color.map(v=>Math.round(v*shade)).join(',')})`;ctx.beginPath();ctx.moveTo(...a.slice(0,2));ctx.lineTo(...b.slice(0,2));ctx.lineTo(...c.slice(0,2));ctx.closePath();ctx.fill();continue;}
      for(let y=y0;y<=y1;y++)for(let x=x0;x<=x1;x++){
        let u=((b[1]-c[1])*(x+.5-c[0])+(c[0]-b[0])*(y+.5-c[1]))/den;
        let v=((c[1]-a[1])*(x+.5-c[0])+(a[0]-c[0])*(y+.5-c[1]))/den,w=1-u-v;
        if(u<-.00001||v<-.00001||w<-.00001)continue;
        const inv=u*a[2]+v*b[2]+w*c[2],depth=1/inv,index=y*width+x;
        if(depth>=zbuffer[index])continue;
        zbuffer[index]=depth;u*=a[2]*depth;v*=b[2]*depth;w*=c[2]*depth;
        const px=uv[0][0]*u+uv[1][0]*v+uv[2][0]*w;
        const py=uv[0][1]*u+uv[1][1]*v+uv[2][1]*w;
        const pz=uv[0][2]*u+uv[1][2]*v+uv[2][2]*w;
        const tx=px+pz*.37,ty=py;
        const fine=hashPixel(Math.floor(tx*420),Math.floor(ty*420),r.seed),coarse=hashPixel(Math.floor(tx*35),Math.floor(ty*35),r.seed);
        let texture=.94+(fine%101)/800,highlight=spec,red=0;
        if(kind==='wood'){
          const grain=hashPixel(Math.floor(tx*670+(coarse%3)),Math.floor(ty*9),r.seed);
          texture=.74+(grain%91)/260+(fine%17)/150;
          if(grain%29===0)texture*=.55;
        }else if(kind==='leather'||kind==='cloth'){
          texture=.80+(coarse%67)/310+(fine%43)/390;
          if(kind==='cloth')texture*=((Math.floor(tx*550)+Math.floor(ty*550))&1)?.96:1.04;
          highlight=0;
        }else{
          const scratch=hashPixel(Math.floor(tx*720),Math.floor(ty*32),r.seed);
          texture=.90+(coarse%29)/180+(fine%41)/410;
          if(scratch%37===0)highlight+=.18;
          if(coarse%31===0){texture*=.78;red=8;}
        }
        const o=index*4;
        for(let k=0;k<3;k++)pixels[o+k]=Math.min(255,Math.max(0,color[k]*shade*texture+highlight*light*(k===2?140:125)+(k===0?red:0)));
        pixels[o+3]=255;
      }
    }
  }
  if(image)ctx.putImageData(image,0,0);
  return bounds;
}

export function createEquipmentSprite(recipe,createCanvas=()=>document.createElement('canvas'),pose={}) {
  const canvas=createCanvas();canvas.width=pose.width||746;canvas.height=420;
  const bounds=paintEquipment(canvas.getContext('2d'),recipe,{...pose,width:canvas.width,height:canvas.height});
  if(pose.crop){
    const x=Math.max(0,Math.floor(bounds[0])-2),y=Math.max(0,Math.floor(bounds[1])-2);
    const cropped=createCanvas();cropped.width=Math.max(1,Math.min(canvas.width-x,Math.ceil(bounds[2])-x+2));cropped.height=Math.max(1,Math.min(canvas.height-y,Math.ceil(bounds[3])-y+2));
    cropped.getContext('2d').drawImage(canvas,-x,-y);cropped.originX=x;cropped.originY=y;return cropped;
  }
  return canvas;
}
