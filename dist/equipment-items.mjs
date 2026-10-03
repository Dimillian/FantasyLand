import {meshBuilder,add} from './equipment-geometry.mjs';
const definitions=new Map();
export const EQUIPMENT_ART_VERSION=4;
export function registerEquipment(id,build){
  if(typeof id!=='string'||!id||typeof build!=='function')throw Error('Equipment requires an id and geometry builder');
  if(definitions.has(id))throw Error(`Equipment already registered: ${id}`);
  definitions.set(id,build);
}
const palette={iron:[96,105,112],edge:[166,178,183],leather:[71,48,32],wood:[94,63,36],cloth:[43,47,43],trim:[117,90,49]};
// Builders run on the main thread only once per item definition/seed. Their
// result is ordinary data, so custom recipes work in the bake worker unchanged.
export function equipmentRecipe(seed=1,kind='sword',options={}){
  const build=definitions.get(kind);if(!build)throw Error(`Unknown equipment: ${kind}`);
  const r={seed:seed>>>0,kind,bladeWidth:.035+(seed%5)*.0015,guardSpan:.20+(seed%3)*.009,...palette,...options.materials};
  const mesh=meshBuilder(r),metadata=build({r,...mesh,options})||{};
  const rig=metadata.rig||'one-handed';
  if(!['one-handed','shield'].includes(rig))throw Error(`Unknown equipment rig: ${rig}`);
  if(mesh.faces.length>10000||mesh.faces.some(f=>f.points.length<3||f.points.some(p=>p.length!==3||p.some(v=>!Number.isFinite(v)))))throw Error('Invalid equipment geometry');
  const recipe={...r,version:EQUIPMENT_ART_VERSION,rig,showHand:metadata.showHand??true,grip:metadata.grip||{position:[0,0,0],wrist:[.012,-.065,-.015]},mesh:mesh.faces};
  let hash=2166136261;for(const ch of JSON.stringify(recipe))hash=Math.imul(hash^ch.charCodeAt(0),16777619)>>>0;
  recipe.key=`${kind}-${r.seed}-${hash.toString(16)}`;return recipe;
}
function wrappedGrip({r,limb}){
  limb([0,-.09,0],[0,.10,0],.025,.023,r.leather);
  for(let i=0;i<11;i++){const y=-.085+i*.017;limb([0,y,0],[0,y+.004,0],.026,.026,r.leather.map(v=>v*.60));}
  limb([0,-.096,0],[0,-.13,0],.043,.030,r.iron,12);
}
function sword({r,face,box,limb}){
    const rotate=p=>p;
      const b=r.bladeWidth;
      // Diamond cross section, narrow fuller, chipped cutting edges and a
      // foreshortened point. The guard's top face faces the sky, not the player.
      const base=.142,tip=1.0;
      const v=[[-b,base,0],[0,base,-.016],[b,base,0],[0,base,.016],[-b*.75,.85,0],[0,.85,-.012],[b*.75,.85,0],[0,.85,.012],[0,tip,0]].map(rotate);
      for(const [q,c] of [[[0,4,5,1],r.iron],[[1,5,6,2],r.edge],[[2,6,7,3],r.iron],[[3,7,4,0],r.iron.map(v=>v*.6)],[[4,8,5],r.iron],[[5,8,6],r.edge],[[6,8,7],r.iron],[[7,8,4],r.iron]])face(q.map(i=>v[i]),c,.045);
      face([[-.004,.185,-.017],[.004,.185,-.017],[.003,.81,-.013],[-.003,.81,-.013]].map(rotate),r.iron.map(v=>v*.55),.01,'steel');
      for(const sign of [-1,1]){
        face([[sign*b,.145,0],[sign*(b-.004),.145,-.003],[sign*(b*.75-.003),.85,-.003],[sign*b*.75,.85,0]].map(rotate),r.edge,.015,'steel');
        // Forged quillons curve away from the hand with rounded end caps.
        let last=[0,.126,0];
        for(let j=1;j<=7;j++){
          const t=j/7,next=[sign*r.guardSpan*.55*t,.126-.039*t*t,.016*t*t];
          limb(rotate(last),rotate(next),.018-.006*t,.018-.006*t,r.iron,8);last=next;
        }
        limb(rotate(add(last,[0,-.012,0])),rotate(add(last,[0,.01,0])),.022,.018,r.trim);
      }
      limb(rotate([0,.092,0]),rotate([0,.146,0]),.031,.027,r.iron);

}

function axe({r,face,box,limb}){const rotate=p=>p;

      box([0,.40,0],[.036,.7,.036],r.wood,rotate);
      face([[0,.7,0],[.23,.81,0],[.29,.56,0],[.08,.54,0]].map(rotate),r.iron);face([[.23,.81,0],[.29,.56,0],[.26,.57,-.01],[.21,.78,-.01]].map(rotate),r.edge);

}
function mace({r,face,box,limb}){const rotate=p=>p;

      box([0,.4,0],[.03,.65,.03],r.iron,rotate);for(let i=0;i<5;i++){const a=i*Math.PI*2/5;box([Math.cos(a)*.06,.73,Math.sin(a)*.06],[.045,.19,.045],r.iron,rotate);}

}
function shield({r,face,box,limb}){const tr=p=>p;
    const radius=.43,sides=64;
    // Inner planks lie behind the rear straps. The outward boss and heraldry
    // are deliberately on the other side of the shield.
    for(let i=0;i<sides;i++){
      const a=i*Math.PI*2/sides,b=(i+1)*Math.PI*2/sides;
      const at=(r,z,t)=>tr([Math.cos(t)*r,Math.sin(t)*r,z]);
      face([tr([0,0,.035]),at(radius,.027,a),at(radius,.027,b)],r.wood,.26,'wood');
      face([at(radius,-.011,a),at(radius,-.011,b),at(radius-.027,-.022,b),at(radius-.027,-.022,a)],r.iron,.21);
      face([at(radius,.027,a),at(radius,.027,b),at(radius,-.011,b),at(radius,-.011,a)],r.iron.map(v=>v*.5),.16);
      face([at(radius,.027,a),at(radius,.060,a),at(radius,.060,b),at(radius,.027,b)],r.wood,.2,'wood');
      face([tr([0,0,.060]),at(radius,.060,b),at(radius,.060,a)],r.wood,.2,'wood');
      face([at(radius-.027,-.022,a),at(radius-.027,-.022,b),at(radius-.027,.029,b),at(radius-.027,.029,a)],r.iron,.12);
    }
    // Narrow, broken wood fibres follow each plank in object space.
    for(let i=0;i<66;i++){
      const x=-.385+i*.0117,h=Math.sqrt(Math.max(0,.39*.39-x*x));
      for(let j=0;j<3;j++){
        const n=((r.seed+i*197+j*887)*16807)>>>0,offset=(n%31)/3100;
        const y0=-h+(h*2/3)*j+.006,y1=Math.min(h,y0+h*.55);
        const color=r.wood.map(v=>v*(.75+(n%51)/100));
        face([[x,y0,-.004],[x+.002+offset*.12,y0,-.004],[x+.004,y1,-.004],[x+.001,y1,-.004]].map(tr),color,.02);
      }
    }
    // Plank seams, grain cuts, and fastened steel reinforcing straps.
    for(let x=-.32;x<.39;x+=.105){const half=Math.sqrt(Math.max(0,(radius-.04)**2-x*x));box([x,0,-.005],[.005,half*2,.009],r.wood.map(v=>v*.38),tr);}
    for(const x of [-.29,.27]){
      const strapHeight=2*Math.sqrt((radius-.028)**2-x*x)-.012;
      box([x,0,-.018],[.034,strapHeight,.013],r.iron.map(v=>v*.66),tr);
      for(const y of [-.27,-.12,.13,.28])box([x,y,-.029],[.019,.018,.014],r.iron,tr);
    }
    for(const y of [-.125,.125]){
      box([-.03,y,-.015],[.084,.037,.025],r.iron,tr);
      limb([-.03,y,-.02],[-.03,y,-.13],.021,.018,r.iron);
    }
    limb([-.03,-.125,-.13],[-.03,.125,-.13],.025,.025,r.leather);
    return {rig:'shield',showHand:false,grip:{position:[-.03,.010,-.13],wrist:[-.100,-.045,-.14]}};

}
for(const [id,build] of [['sword',sword],['axe',axe],['mace',mace]])registerEquipment(id,args=>{wrappedGrip(args);build(args);});
registerEquipment('shield',shield);
// Non-weapon proof: a held lantern uses exactly the same grip and bake path.
registerEquipment('lantern',({r,box,limb})=>{
  limb([0,-.08,0],[0,.1,0],.024,.024,r.leather);
  box([0,.24,0],[.16,.035,.13],r.iron);
  box([0,.40,0],[.13,.26,.10],[174,115,43]);
  for(const x of [-.075,.075])for(const z of [-.06,.06])limb([x,.25,z],[x,.55,z],.011,.011,r.iron);
  box([0,.55,0],[.18,.035,.15],r.iron);
});
