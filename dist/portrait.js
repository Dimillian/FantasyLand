// A miniature is painted once per conversation. Identity colors come from Rust's
// paper-doll palette; no downloaded portraits or extra world-renderer passes.
export function paintPortrait(canvas, person) {
  const context = canvas?.getContext('2d'); if (!context?.createImageData) return;
  const a = person.appearance || {skin:3,hair:0,palette:[.30,.36,.35],body:0};
  const role = (person.role || '').toLowerCase();
  let seed=2166136261; for (const c of person.name || '') seed=Math.imul(seed^c.charCodeAt(0),16777619)>>>0;
  const skins=[[.82,.62,.43],[.65,.43,.27],[.43,.27,.18],[.87,.69,.52],[.56,.36,.22]];
  const hairs=[[.14,.09,.055],[.35,.20,.10],[.63,.45,.22],[.44,.19,.07],[.61,.60,.53],[.20,.16,.13],[.75,.70,.58],[.32,.29,.25]];
  const skin=skins[a.skin%5], hair=hairs[a.hair%8], cloth=a.palette;
  const ink=[.12,.085,.07],gold=[.70,.51,.24],linen=[.67,.61,.45];
  const pixels=context.createImageData(64,80);
  const dot=(x,y,c,shade=1)=>{if(x<0||x>=64||y<0||y>=80)return;const i=(y*64+x)*4;for(let k=0;k<3;k++)pixels.data[i+k]=Math.max(0,Math.min(255,Math.round(c[k]*shade*255)));pixels.data[i+3]=255;};
  const rect=(x,y,w,h,c,shade=1)=>{for(let yy=y;yy<y+h;yy++)for(let xx=x;xx<x+w;xx++)dot(xx,yy,c,shade);};
  const polygon=(points,c,shade=1)=>{
    const minY=Math.max(0,Math.floor(Math.min(...points.map(p=>p[1])))),maxY=Math.min(79,Math.ceil(Math.max(...points.map(p=>p[1]))));
    for(let y=minY;y<=maxY;y++)for(let x=0;x<64;x++){let inside=false;for(let i=0,j=points.length-1;i<points.length;j=i++) {const p=points[i],q=points[j];if((p[1]>y)!==(q[1]>y)&&x<(q[0]-p[0])*(y-p[1])/(q[1]-p[1])+p[0])inside=!inside;}if(inside)dot(x,y,c,shade*(.90+.12*(1-x/64))+(((x*13+y*7+seed)%11===0)? .025:0));}
  };
  const ellipse=(cx,cy,rx,ry,c,shade=1)=>{for(let y=Math.max(0,cy-ry);y<=Math.min(79,cy+ry);y++)for(let x=Math.max(0,cx-rx);x<=Math.min(63,cx+rx);x++){if(((x-cx)/rx)**2+((y-cy)/ry)**2<=1)dot(x,y,c,shade*(1.08-.20*(x-cx)/rx));}};
  for(let y=0;y<80;y++)for(let x=0;x<64;x++){const glow=Math.max(0,1-Math.hypot(x-23,y-31)/48);const joint=(y%12===0||((x+(Math.floor(y/12)%2)*9)%19===0))?.75:1;dot(x,y,[.13+glow*.09,.15+glow*.09,.14+glow*.045],joint);}
  // Mantle and linen collar, below a directional, faceted face.
  const cx=31+(seed%3)-1;
  ellipse(cx,a.body===0?31:25,a.body===2?16:14,a.body===0?24:16,hair,.65);
  polygon([[cx-9,49],[cx+9,49],[57,65],[63,79],[1,79],[7,63]],cloth,.9);
  rect(cx-5,43,10,15,skin,.66);
  polygon([[cx-9,50],[cx-1,60],[cx-6,67],[cx-15,55]],linen);
  polygon([[cx+7,49],[cx-1,60],[cx+3,66],[cx+14,55]],linen,.75);
  polygon([[7,63],[cx-14,53],[cx-5,67],[cx-6,79],[1,79]],cloth,1.25);
  polygon([[cx+14,54],[57,65],[63,79],[cx+3,79],[cx+4,64]],cloth,.62);
  ellipse(cx-11,34,3,5,skin,.78);ellipse(cx+10,34,2,5,skin,.61);
  const jaw=a.body===0?7:a.body===2?11:9;
  polygon([[cx-10,21],[cx+8,21],[cx+11,30],[cx+8,42],[cx+4,48],[cx-2,50],[cx-jaw,44],[cx-11,31]],skin);
  polygon([[cx+2,24],[cx+9,24],[cx+10,35],[cx+6,45],[cx+1,48],[cx+3,37]],skin,.70);
  polygon([[cx-9,33],[cx-4,35],[cx-5,41],[cx-9,39]],skin,1.20);
  polygon([[cx-1,30],[cx+2,31],[cx+4,40],[cx-2,40]],skin,1.17);
  rect(cx+2,37,2,4,skin,.64);rect(cx-1,40,4,1,ink,.9);
  rect(cx-8,30,6,2,hair,.65);rect(cx+3,30,5,2,hair,.56);
  rect(cx-7,33,5,1,linen);rect(cx+3,33,4,1,linen,.76);
  rect(cx-5,32,2,2,ink);rect(cx+4,32,2,2,ink);
  rect(cx-4,44,7,1,[.40,.20,.14]);rect(cx-2,45,4,1,skin,1.12);
  polygon([[cx-14,27],[cx-11,15],[cx-3,11],[cx+8,14],[cx+13,22],[cx+11,30],[cx+7,23],[cx+1,20],[cx-7,24],[cx-11,36]],hair,1.05);
  for(let n=0;n<6;n++){let x=cx-10+n*4;rect(x,17+(n%3),2,3,hair,1.4);}
  if(a.body===1 || a.body===2){polygon([[cx-9,38],[cx-4,45],[cx+3,45],[cx+8,39],[cx+6,49],[cx,54],[cx-7,49]],hair,.84);rect(cx-3,44,6,1,ink);}
  if((person.age||35)>48){rect(cx-8,36,5,1,skin,.69);rect(cx+4,36,4,1,skin,.52);rect(cx-5,27,7,1,skin,.73);}
  if(/mage|arcanist|scholar/.test(role)){
    polygon([[cx-19,27],[cx-10,12],[cx+3,4],[cx+17,27],[cx+12,31],[cx+7,19],[cx-6,19],[cx-13,31]],cloth,.8);
    polygon([[cx-10,17],[cx+2,7],[cx-1,14]],gold,.85);rect(cx-2,11,2,2,linen,1.2);
    rect(cx-1,61,2,17,gold,.8);rect(cx-3,65,6,2,gold);
  } else if(/guard|fighter/.test(role)) {
    polygon([[cx-14,25],[cx-12,14],[cx-2,9],[cx+10,13],[cx+14,25]], [.46,.48,.43]);
    rect(cx-2,10,3,15,[.64,.62,.50]);rect(cx-14,24,29,3,ink);
    polygon([[5,63],[cx-14,55],[cx-9,59],[cx-12,68]], [.52,.54,.48]);
  } else if(/merchant|innkeeper|farmer/.test(role)) {
    polygon([[cx-15,23],[cx-11,13],[cx+9,14],[cx+13,25],[cx-16,27]],cloth,1.2);rect(cx-15,24,29,2,gold,.65);
  }
  // Brooch, mantle seams and hand-stitched trim.
  ellipse(cx+9,60,3,3,gold);dot(cx+8,59,linen,1.35);dot(cx+10,61,ink);
  for(let y=68;y<80;y+=3){dot(cx-8,y,gold,.8);dot(cx+6,y,gold,.65);}
  canvas.width=64;canvas.height=80;context.putImageData(pixels,0,0);
}
