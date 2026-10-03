//! Sparse, seed-stable landmark guardians. Only the nearby encounter simulates.
#[cfg(test)]
use crate::combat::Space;
use crate::{combat::Combat, world::World};
use glam::{Vec2, Vec3};
use serde::Serialize;
#[derive(Clone, Serialize)]
pub struct Encounter {
    pub id: String,
    pub name: String,
    pub kind: u32,
    pub x: f32,
    pub z: f32,
    pub y: f32,
}
pub fn locations(world: &World, x: f32, z: f32, radius: f32) -> Vec<Encounter> {
    let mut out = vec![];
    for l in world.landmarks_near(x, z, radius) {
        let kind = match l.kind.as_str() {
            "crypt" | "cemetery" => 0,
            "enemy_camp" | "mini_dungeon" => 1,
            "haunted_grove" => 2,
            _ => continue,
        };
        let layout = world.hostile.layout(world, &l);
        for (slot, p) in layout.spawns.iter().enumerate() {
            out.push(Encounter {
                id: format!("monster:{}:{}", l.stable_id.0, slot),
                name: format!(
                    "{} · {}",
                    ["Barrow skeleton", "Thorn goblin", "Hollow ghost"][kind as usize],
                    l.name
                ),
                kind,
                x: p[0],
                y: p[1],
                z: p[2],
            });
        }
    }
    out.sort_by(|a, b| ((a.x - x).hypot(a.z - z)).total_cmp(&(b.x - x).hypot(b.z - z)));
    out
}
#[derive(Default)]
pub struct Encounters {
    pub defeated: std::collections::BTreeSet<String>,
    pub corpses: Vec<crate::loot::Corpse>,
    pub current: Option<String>,
    cached: Vec<Encounter>,
    cell: Option<(i32, i32)>,
    cooldown: f32,
    dead_time: f32,
}
impl Encounters {
    pub fn complete(&mut self, seed: u32, c: &Combat) {
        if c.result != 1 {
            return;
        }
        if let Some(id) = &self.current {
            if self.defeated.insert(id.clone()) {
                self.corpses.push(crate::loot::Corpse {
                    id: id.clone(),
                    kind: c.kind,
                    position: c.enemy.to_array(),
                    items: crate::loot::roll(seed, id, c.kind),
                });
            }
        }
    }
    pub fn reset_nearby(&mut self) {
        self.cell = None;
        self.current = None;
        self.cached.clear();
    }
    pub fn tick(&mut self, world: &World, p: Vec3, c: &mut Combat, dt: f32) {
        if dt <= 0. {
            return;
        }
        self.cooldown = (self.cooldown - dt).max(0.);
        self.complete(world.seed, c);
        if self.current.is_some() {
            if c.result == 1 {
                self.dead_time += dt;
            } else {
                self.dead_time = 0.;
            }
            if c.result != 2
                && (c.enemy.distance(p) > 95. || (c.result == 1 && self.dead_time > 1.2))
            {
                c.stop();
                self.current = None;
                self.cooldown = 2.;
                self.dead_time = 0.;
            }
            return;
        }
        if c.active || self.cooldown > 0. {
            return;
        }
        let cell = ((p.x / 128.).floor() as i32, (p.z / 128.).floor() as i32);
        if self.cell != Some(cell) {
            self.cached = locations(world, p.x, p.z, 240.);
            self.cell = Some(cell);
        }
        if let Some(e) = self
            .cached
            .iter()
            .filter(|e| {
                !self.defeated.contains(&e.id) && Vec2::new(e.x - p.x, e.z - p.z).length() < 65.
            })
            .min_by(|a, b| {
                (a.x - p.x)
                    .hypot(a.z - p.z)
                    .total_cmp(&(b.x - p.x).hypot(b.z - p.z))
            })
        {
            c.spawn_at(Vec3::new(e.x, e.y, e.z), e.kind);
            self.current = Some(e.id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guardians_are_stable_dry_and_stay_cleared() {
        let w = World::new(1337);
        let (p, _) = w.spawn_view();
        let a = locations(&w, p[0], p[1], 2500.);
        let b = locations(&w, p[0], p[1], 2500.);
        assert!(!a.is_empty());
        assert_eq!(
            a.iter().map(|e| &e.id).collect::<Vec<_>>(),
            b.iter().map(|e| &e.id).collect::<Vec<_>>()
        );
        let e = &a[0];
        assert!(!w.blocked(Vec2::new(e.x, e.z), e.y));
        assert!(w.sites_near(e.x, e.z, 140.).is_empty());
        let mut area = Encounters::default();
        let mut c = Combat::default();
        let pos = Vec3::new(e.x, e.y, e.z + 4.);
        area.tick(&w, pos, &mut c, 0.02);
        assert_eq!(area.current.as_ref(), Some(&e.id));
        assert!(c.active);
        c.result = 1;
        area.tick(&w, pos, &mut c, 0.02);
        assert!(area.defeated.contains(&e.id));
        assert_eq!(area.corpses.len(), 1);
        assert!(!area.corpses[0].items.is_empty());
        area.complete(w.seed, &c);
        assert_eq!(area.corpses.len(), 1);
        c.stop();
        area.reset_nearby();
        area.tick(&w, pos, &mut c, 0.02);
        assert_ne!(area.current.as_ref(), Some(&e.id));
    }
}
