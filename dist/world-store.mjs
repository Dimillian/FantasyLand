// World format is validated again by Rust before any engine state is restored.
export const descriptor = seed => ({seed, generator_version:1, recipe_revision:1});
export const worldKey = w => `g${w.generator_version}:r${w.recipe_revision}:s${w.seed}`;
export function validSeed(value, fallback=1337) {
  if (value === null || value === undefined || value === '') return fallback;
  const n=Number(value); return Number.isInteger(n) && n>=0 && n<=4294967295 ? n : fallback;
}
export function validateSave(save) {
  const s=save?.snapshot, w=s?.world, p=s?.player;
  if (save?.schema!==1 || s?.schema!==1 || !w || validSeed(w.seed,-1)!==w.seed || w.generator_version!==1 || w.recipe_revision!==1)
    throw new Error('Only saves from the current world version can be imported.');
  const exact=(object,fields)=>object && Object.keys(object).every(k=>fields.includes(k)) && fields.every(k=>Object.hasOwn(object,k));
  if(!exact(s,['schema','world','player','clock','doors']) || !exact(w,['seed','generator_version','recipe_revision']) || !exact(p,['x','z','yaw','pitch','health','mana','stamina','walked']))throw new Error('Unrecognized save fields.');
  if (!p || !['x','z','yaw','pitch','health','mana','stamina','walked'].every(k=>Number.isFinite(p[k])) || Math.abs(p.x)>=192000 || Math.abs(p.z)>=192000 || Math.abs(p.pitch)>1.42 || Math.abs(p.yaw)>1e6 || p.walked>1e12 || ['health','mana','stamina'].some(k=>p[k]<0||p[k]>100) || p.walked<0 || !Number.isFinite(s.clock) || s.clock<0 || s.clock>1e12)
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
