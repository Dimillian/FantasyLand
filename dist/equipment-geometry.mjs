export const add=(a,b)=>a.map((v,i)=>v+b[i]), sub=(a,b)=>a.map((v,i)=>v-b[i]),mul=(a,s)=>a.map(v=>v*s);
export const cross=(a,b)=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
export const norm=a=>mul(a,1/Math.max(.00001,Math.hypot(...a)));
export const mix=(a,b,t)=>a.map((v,i)=>v+(b[i]-v)*t);
export const ease=t=>t*t*(3-2*t);

export function meshBuilder(r){
  const faces=[];let seq=0;
  const material=color=>color===r.wood?'wood':color===r.leather?'leather':color===r.cloth?'cloth':color===r.trim?'brass':color[2]>=color[0]*.85?'steel':'leather';
  const face=(points,color,rough=.1,kind=material(color))=>faces.push({points,texcoords:points.map(p=>p.slice()),color,rough,kind,id:seq++});
  const box=(c,size,color,transform=p=>p)=>{
    const v=[[-1,-1,-1],[1,-1,-1],[1,1,-1],[-1,1,-1],[-1,-1,1],[1,-1,1],[1,1,1],[-1,1,1]].map(p=>transform(add(c,p.map((v,i)=>v*size[i]/2))));
    for(const q of [[0,3,2,1],[4,5,6,7],[0,4,7,3],[1,2,6,5],[3,7,6,2],[0,1,5,4]])face(q.map(i=>v[i]),color);
  };
  const limb=(a,b,ra,rb,color,sides=12)=>{
    const axis=norm(sub(b,a)),right=norm(cross(axis,Math.abs(axis[2])<.9?[0,0,1]:[0,1,0])),up=norm(cross(right,axis));
    const ring=(p,r)=>Array.from({length:sides},(_,i)=>add(p,add(mul(right,Math.cos(i*Math.PI*2/sides)*r),mul(up,Math.sin(i*Math.PI*2/sides)*r))));
    const aa=ring(a,ra),bb=ring(b,rb);for(let i=0;i<sides;i++){const j=(i+1)%sides;face([aa[i],aa[j],bb[j],bb[i]],color);}face(bb,color);
  };

  return {faces,face,box,limb};
}
