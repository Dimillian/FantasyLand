import {meshBuilder,add,sub,mul,mix,ease} from './equipment-geometry.mjs';
export function weaponPose(swing=0) {
  const idle={hand:[.38,-.43,.57],roll:-.16,tilt:.30};
  const raised={hand:[.57,-.24,.56],roll:-.48,tilt:.20};
  const follow={hand:[-.16,-.40,.63],roll:1.52,tilt:.32};
  let a=idle,b=idle,t=0;
  if(swing>0&&swing<.20){a=idle;b=raised;t=ease(swing/.20);}
  else if(swing>=.20&&swing<.36){a=raised;b=follow;t=ease((swing-.20)/.16);}
  else if(swing>=.36){a=follow;b=idle;t=ease(Math.min(1,(swing-.36)/.22));}
  return {hand:mix(a.hand,b.hand,t),roll:a.roll+(b.roll-a.roll)*t,tilt:a.tilt+(b.tilt-a.tilt)*t};
}

export function shieldPose(guard=0){return {center:mix([-.46,-.60,.94],[-.21,-.18,.72],guard),yaw:-.40+guard*.14};}
export function rigEquipment(r,{swing=0,guard=0,hand='right'}={}){
  const mesh=meshBuilder(r),{face,box,limb}=mesh;
  // A fist around a VERTICAL handle: four fingers stack down its axis.
  // Each finger curls in the XZ plane, with the thumb crossing the index.
  const grasp=(tr,handed=1)=>{
    const point=p=>tr([p[0]*handed,p[1],p[2]]);
    limb(point([.035,-.085,-.008]),point([.036,.050,-.008]),.032,.040,r.leather,14);
    for(let i=0;i<4;i++){
      const y=.054-i*.035,size=.0165-i*.0007;
      const joints=[[.058,y,-.013],[.043,y,-.047],[.010,y,-.062],[-.027,y,-.042],[-.029,y,-.007],[-.007,y,.014]];
      for(let j=0;j<joints.length-1;j++)limb(point(joints[j]),point(joints[j+1]),size,size*.96,r.leather,10);
      // Worn seam over the knuckle follows the finger, not a row of studs.
      limb(point([.031,y+.008,-.056]),point([.006,y+.008,-.065]),.003,.0025,r.leather.map(v=>v*1.32),6);
    }
    limb(point([.056,-.010,-.045]),point([.022,.059,-.078]),.025,.021,r.leather,12);
    limb(point([.022,.059,-.078]),point([-.013,.044,-.069]),.021,.017,r.leather,12);
  };
  const pose=weaponPose(swing);
  const rotate=p=>{
    let [x,y,z]=p;[x,z]=[x*Math.cos(.40)+z*Math.sin(.40),-x*Math.sin(.40)+z*Math.cos(.40)];const ct=Math.cos(pose.tilt),st=Math.sin(pose.tilt),cr=Math.cos(pose.roll),sr=Math.sin(pose.roll);
    [y,z]=[y*ct-z*st,y*st+z*ct];return add(pose.hand,[x*cr-y*sr,x*sr+y*cr,z]);
  };

  let modelTransform=rotate;
  if(r.rig==='shield'){
    const {center,yaw}=shieldPose(guard);
    const tr=p=>add(center,[p[0]*Math.cos(yaw)+p[2]*Math.sin(yaw),p[1],-p[0]*Math.sin(yaw)+p[2]*Math.cos(yaw)]);
    modelTransform=tr;
    if(r.showHand!==false){
    const wrist=tr(r.grip.wrist),elbow=[-.73,-1.10,.32];
    limb(elbow,wrist,.14,.052,r.cloth);
    limb(mix(elbow,wrist,.62),mix(elbow,wrist,.98),.086,.054,r.leather);
    // Cuff follows the forearm axis; no upright pillar or floating crossbar.
    limb(mix(elbow,wrist,.82),mix(elbow,wrist,.85),.070,.067,r.leather.map(v=>v*.62));
    grasp(p=>tr(add(p,r.grip.position)),-1);
    }
  }else{
    // Jointed arm enters from well below the near plane: every pose reaches the
    // viewport bottom, including the follow-through. It is never a cutout forearm.
    const wrist=rotate(r.grip.wrist),elbow=[.78,-1.12,.38];
    limb(elbow,wrist,.17,.073,r.cloth);
    limb(mix(elbow,wrist,.51),mix(elbow,wrist,.91),.119,.076,r.leather);
    for(const t of [.57,.85]){const c=mix(elbow,wrist,t);limb(add(c,[0,-.018,0]),add(c,[0,.012,0]),.108-(t-.57)*.07,.108-(t-.57)*.07,r.trim);}
    grasp(p=>rotate(add(p,r.grip.position)));
  }
  const all=[...r.mesh.map(f=>({...f,points:f.points.map(modelTransform)})),...mesh.faces];
  // Mirror the complete rig, including thumb and wrist, for opposite-hand use.
  const natural=r.rig==='shield'?'left':'right';
  return hand===natural?all:all.map(f=>({...f,points:f.points.map(([x,y,z])=>[-x,y,z])}));
}
