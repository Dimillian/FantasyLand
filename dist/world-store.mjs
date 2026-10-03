// World format is validated again by Rust before any engine state is restored.
export const descriptor = seed => ({seed, generator_version:3, recipe_revision:1});
export const worldKey = w => `g${w.generator_version}:r${w.recipe_revision}:s${w.seed}`;
export function validSeed(value, fallback=1337) {
  if (value === null || value === undefined || value === '') return fallback;
  const n=Number(value); return Number.isInteger(n) && n>=0 && n<=4294967295 ? n : fallback;
}
export function validateSave(save) {
  const s=save?.snapshot, w=s?.world, p=s?.player;
  if (save?.schema!==1 || s?.schema!==3 || !w || validSeed(w.seed,-1)!==w.seed || w.generator_version!==3 || w.recipe_revision!==1)
    throw new Error('Only saves from the current world version can be imported.');
  const exact=(object,fields)=>object && Object.keys(object).every(k=>fields.includes(k)) && fields.every(k=>Object.hasOwn(object,k));
  if(!exact(s,['schema','world','player','clock','doors','character','defeated','corpses']) || !exact(w,['seed','generator_version','recipe_revision']) || !exact(p,['x','z','yaw','pitch','health','mana','stamina','walked']))throw new Error('Unrecognized save fields.');
  const c=s.character;
  const skill=v=>v&&exact(v,['rank','xp'])&&Number.isInteger(v.rank)&&v.rank>=1&&v.rank<=100&&Number.isInteger(v.xp)&&v.xp>=0&&v.xp<50+v.rank*25&&(v.rank<100||v.xp===0);
  if(!exact(c,['strength','endurance','agility','intellect','willpower','sword_equipped','shield_equipped','blades','blocking','inventory'])||!['strength','endurance','agility','intellect','willpower'].every(k=>Number.isInteger(c[k])&&c[k]>=1&&c[k]<=50)||typeof c.sword_equipped!=='boolean'||typeof c.shield_equipped!=='boolean'||!skill(c.blades)||!skill(c.blocking))throw new Error('Invalid character data.');
  const itemIds=['iron-sword','oak-shield','old-bone','goblin-fang','ectoplasm','grave-essence','wild-essence','spectral-essence'];
  const stacks=v=>Array.isArray(v)&&v.length<=48&&new Set(v.map(i=>i.id)).size===v.length&&v.every(i=>exact(i,['id','quantity'])&&itemIds.includes(i.id)&&Number.isInteger(i.quantity)&&i.quantity>=1&&i.quantity<=1000000);
  if(!stacks(c.inventory)||(c.sword_equipped&&!c.inventory.some(i=>i.id==='iron-sword'))||(c.shield_equipped&&!c.inventory.some(i=>i.id==='oak-shield')))throw new Error('Invalid inventory.');
  if(!Array.isArray(s.defeated)||s.defeated.length>10000||new Set(s.defeated).size!==s.defeated.length||s.defeated.some(id=>typeof id!=='string'||!id.startsWith('monster:')||id.length>100))throw new Error('Invalid encounter state.');
  if(!Array.isArray(s.corpses)||s.corpses.length>10000||new Set(s.corpses.map(c=>c.id)).size!==s.corpses.length||s.corpses.some(c=>!exact(c,['id','kind','position','items'])||!s.defeated.includes(c.id)||!Number.isInteger(c.kind)||c.kind<0||c.kind>2||!Array.isArray(c.position)||c.position.length!==3||c.position.some(v=>!Number.isFinite(v)||Math.abs(v)>=192000)||!stacks(c.items)))throw new Error('Invalid corpse loot.');
  if (!p || !['x','z','yaw','pitch','health','mana','stamina','walked'].every(k=>Number.isFinite(p[k])) || Math.abs(p.x)>=192000 || Math.abs(p.z)>=192000 || Math.abs(p.pitch)>1.42 || Math.abs(p.yaw)>1e6 || p.walked>1e12 || ['health','mana','stamina'].some(k=>p[k]<0||p[k]>60+4*c[k==='mana'?'intellect':'endurance']) || p.walked<0 || !Number.isFinite(s.clock) || s.clock<0 || s.clock>1e12)
    throw new Error('Invalid player data in save.');
  if (!Array.isArray(s.doors) || s.doors.length>10000) throw new Error('Invalid door data in save.');
  const ids=new Set();
  for (const d of s.doors) {
    if (!exact(d,['id','angle','target','hold']) || !Number.isInteger(d.id)||d.id<0||d.id>4294967295||ids.has(d.id)||![d.angle,d.target,d.hold].every(Number.isFinite)||d.angle<0||d.angle>1||d.target<0||d.target>1||d.hold<0||d.hold>3600) throw new Error('Invalid door state in save.');
    ids.add(d.id);
  }
  const point=p=>p && Number.isFinite(p.x)&&Number.isFinite(p.z)&&Math.abs(p.x)<768000&&Math.abs(p.z)<768000;
  if (save.atlas && (!point(save.atlas)||!Number.isFinite(save.atlas.span)||save.atlas.span<128||save.atlas.span>1536000)) throw new Error('Invalid saved atlas.');
  if (save.waypoint && (!point(save.waypoint)||typeof save.waypoint.name!=='string'||save.waypoint.name.length>300)) throw new Error('Invalid saved waypoint.');
  return save;
}
export class WorldStore {
  constructor(storage) {this.storage=storage;}
  key(w) {return `fantasyland.world.${worldKey(w)}`;}
  // Development policy: incompatible/corrupt local state starts a fresh world.
  // There are no old-generator loaders, migration slots or recovery snapshots.
  load(w) {
    const key=this.key(w), raw=this.storage.getItem(key);
    if(!raw)return null;
    try {
      const save=validateSave(JSON.parse(raw));
      if(worldKey(save.snapshot.world)!==worldKey(w))throw new Error('Save belongs to another world.');
      return save;
    } catch {
      this.storage.removeItem(key);
      return null;
    }
  }
  write(save) {
    validateSave(save);
    this.storage.setItem(this.key(save.snapshot.world),JSON.stringify(save));
  }
}
