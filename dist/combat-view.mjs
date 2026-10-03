import {EquipmentSpriteCache} from './equipment-cache.mjs';
export function projectCombat(position,frame,width,height) {
  const dx=position[0]-frame[12],dy=position[1]-frame[13],dz=position[2]-frame[14];
  const yaw=frame[15],pitch=frame[16];const forward=dx*Math.sin(yaw)-dz*Math.cos(yaw);
  const depth=forward*Math.cos(pitch)+dy*Math.sin(pitch);
  if(depth<.15)return null;
  const scale=height/(2*Math.tan(72*Math.PI/360));
  return {x:width/2+(dx*Math.cos(yaw)+dz*Math.sin(yaw))*scale/depth,y:height/2-(dy*Math.cos(pitch)-forward*Math.sin(pitch))*scale/depth,depth};
}
export function combatBarPosition(projected,width,height) {
  if(!projected||projected.depth<.15||projected.depth>22||projected.x < -width*.35||projected.x>width*1.35)return null;
  return {x:Math.max(66,Math.min(width-66,projected.x)),y:Math.max(70,Math.min(height-100,projected.y))};
}
export class CombatView {
  constructor(canvas){this.canvas=canvas;this.ctx=null;this.sprites=null;this.numbers=[];this.flash=0;this.flashKind='';this.time=0;this.lastFrame=null;this.kick=0;this.guard=0;this.drawAmount=0;this.trailHealth=100;this.bursts=[];this.damagePulse=0;this.equipment=new EquipmentSpriteCache();}
  setEquipment(loadout){this.equipment.setLoadout(loadout);}
  reset(){this.numbers=[];this.trailHealth=100;this.bursts=[];this.flash=0;this.canvas.classList.add('hidden');}
  event(e){
    if(['hit','block','hurt','guard-break'].includes(e.kind)){this.flash=.22;this.flashKind=e.kind;this.kick=1;}
    if(e.kind==='hit'){this.numbers.push({position:e.position,amount:e.amount,life:1.05});this.damagePulse=.5;}
    if(e.kind==='hit'||e.kind==='block'){this.bursts.push({position:e.position,kind:e.kind,life:.38});if(this.bursts.length>6)this.bursts.shift();}
    if(this.numbers.length>12)this.numbers.shift();
  }
  draw(f,dt,{visible=true,moving=false}={}) {
    const armed=!!f?.[0],trial=!!f?.[23];
    this.drawAmount+=(Number(armed)-this.drawAmount)*Math.min(1,dt*15);
    this.canvas.classList.toggle('hidden',!visible||(!trial&&this.drawAmount<.01));
    if(!visible||(!trial&&this.drawAmount<.01))return;
    if(!this.ctx){this.ctx=this.canvas.getContext('2d');}
    const c=this.ctx,w=Math.round(420*this.canvas.clientWidth/Math.max(1,this.canvas.clientHeight)),h=420;
    if(this.canvas.width!==w||this.canvas.height!==h){this.canvas.width=w;this.canvas.height=h;}
    c.clearRect(0,0,w,h);c.imageSmoothingEnabled=false;this.time+=dt;this.flash=Math.max(0,this.flash-dt);this.kick=Math.max(0,this.kick-dt*8);
    const text=(value,x,y,color='#e5d6aa',size=10)=>{c.font=`${size}px monospace`;c.textAlign='center';c.fillStyle='#121b20';c.fillText(value,Math.round(x)+1,Math.round(y)+1);c.fillStyle=color;c.fillText(value,Math.round(x),Math.round(y));};
    const proj=combatBarPosition(projectCombat([f[4],f[5]+[2.22,1.65,2.43][f[24]||0],f[6]],f,w,h),w,h);
    this.trailHealth=f[3]>this.trailHealth?f[3]:Math.max(f[3],this.trailHealth-dt*38);this.damagePulse=Math.max(0,this.damagePulse-dt);
    this.guard+=(f[2]-this.guard)*Math.min(1,dt*16);
    const bob=moving?Math.sin(this.time*9)*3:Math.sin(this.time*1.8)*1.2;
    const light=Math.round((.40+.60*Math.max(0,Math.sin(((f[21]??12)-6)*Math.PI/12))*(1-(f[22]??0)*.26))*8)/8;
    this.equipment.warmSprites(w,light);
    const cached=(slot,pose)=>this.equipment.sprite(slot,pose,w,light);
    // Bake perspective poses once; arms stay attached to the screen edge.
    if(this.drawAmount>.01){
    const lower=(1-this.drawAmount)*h*.95;
    const shield=cached('offHand',this.equipment.loadout.offHand?.rig==='shield'?Math.round(this.guard*12):0),sword=cached('mainHand',this.equipment.loadout.mainHand?.rig==='shield'?Math.round(this.guard*12):Math.min(32,Math.round(f[1]/.58*32)));
    if(shield)c.drawImage(shield,shield.originX+this.kick*2,shield.originY+lower+bob+this.kick*3);
    if(sword)c.drawImage(sword,sword.originX-this.kick*3,sword.originY+lower+bob+this.kick*3);
    }
    if(f[23]&&proj&&f[10]!==1&&f[20]!==0){
      const bw=110,x=proj.x-bw/2,y=proj.y;
      text(['BARROW SKELETON','THORN GOBLIN','HOLLOW GHOST'][f[24]||0],proj.x,y-9,'#eee1b8',10);
      c.fillStyle='#171d22';c.fillRect(x-1,y-1,bw+2,6);c.fillStyle='#452e2b';c.fillRect(x,y,bw,5);c.fillStyle='#e3c393';c.fillRect(x,y,bw*Math.min(this.trailHealth,f[25]||100)/(f[25]||100),5);c.fillStyle=this.damagePulse>.3?'#edac7a':'#b66c55';c.fillRect(x,y,bw*Math.max(0,f[3])/(f[25]||100),5);text(`${Math.ceil(f[3])} / ${f[25]||100}`,proj.x,y+17,'#dbcab0',9);

    }
    // Brief readable contact marks, never a full-screen white flash.
    if(this.flash>0){
      c.globalAlpha=this.flash/.22;
      if(this.flashKind==='hit'||this.flashKind==='block'){
        c.strokeStyle=this.flashKind==='block'?'#f0d894':'#eee9d0';c.lineWidth=1;
        for(let i=0;i<6;i++){const a=i*Math.PI/3,r=8+(1-this.flash/.22)*18;c.beginPath();c.moveTo(w/2+Math.cos(a)*r,h/2+Math.sin(a)*r);c.lineTo(w/2+Math.cos(a)*(r+5),h/2+Math.sin(a)*(r+5));c.stroke();}
        text(this.flashKind==='block'?'BLOCK':'',w/2,h/2+28,'#eedca9',10);
      }else{c.fillStyle='#8f2926';c.fillRect(0,0,w,4);c.fillRect(0,h-4,w,4);c.fillRect(0,0,4,h);c.fillRect(w-4,0,4,h);text(this.flashKind==='guard-break'?'GUARD BROKEN':'',w/2,h/2+32,'#e8a892');}
      c.globalAlpha=1;
    }
    for(const b of this.bursts){
      b.life-=dt;const p=projectCombat(b.position,f,w,h)||{x:w/2,y:h/2};const age=.38-b.life;
      for(let i=0;i<14;i++){const angle=i*2.399,travel=age*(40+(i%5)*16);c.globalAlpha=Math.max(0,b.life/.38);c.fillStyle=i%3===0?'#f3e6c1':b.kind==='block'?'#c8ab68':'#b98666';c.fillRect(Math.round(p.x+Math.cos(angle)*travel),Math.round(p.y+Math.sin(angle)*travel+age*age*80),i%4===0?3:2,2);}
    }
    this.bursts=this.bursts.filter(b=>b.life>0);
    for(const n of this.numbers){n.life-=dt;const p=projectCombat(n.position,f,w,h);if(!p)continue;c.globalAlpha=Math.max(0,Math.min(1,n.life*3));const nx=Math.max(22,Math.min(w-22,p.x+16)),ny=Math.max(90,Math.min(h-85,p.y-20-(1.05-n.life)*45));text(String(n.amount),nx,ny,n.amount>25?'#ffe3a0':'#f4edda',19);if(n.amount>25)text('COUNTER',nx,ny+13,'#dabb7e',8);}
    this.numbers=this.numbers.filter(n=>n.life>0);c.globalAlpha=1;
    if(f[23]&&f[10]===2){text('DEFEATED',w/2,h*.32,'#ebd59b',22);}

  }
}
