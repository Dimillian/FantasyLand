//! Bounded encounter simulation shared by world guardians and sparring. Rules live in Rust, presentation
//! consumes a small packet and discrete events. No civilian AI is commandeered.
use crate::{
    geometry::{self, MeshData, Vertex},
    player::Player,
    world::World,
};
use glam::{Vec2, Vec3};
use serde::Serialize;
use std::collections::{BinaryHeap, HashMap};

pub trait Space {
    fn floor(&self, p: Vec2) -> f32;
    fn blocked(&self, p: Vec2, feet: f32) -> bool;
}
impl Space for World {
    fn floor(&self, p: Vec2) -> f32 {
        geometry::walk_height(self, p.x, p.y)
    }
    fn blocked(&self, p: Vec2, feet: f32) -> bool {
        geometry::blocks_body(self, p.x, feet + 0.06, p.y)
            || self.sample(p.x, p.y).water_height > feet + 0.55
    }
}
fn xz(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}
fn facing(yaw: f32) -> Vec2 {
    Vec2::new(yaw.sin(), -yaw.cos())
}
fn stand(s: &impl Space, p: Vec2) -> Option<Vec3> {
    let y = s.floor(p);
    if !y.is_finite() || s.blocked(p, y) {
        None
    } else {
        Some(Vec3::new(p.x, y, p.y))
    }
}
fn passage(s: &impl Space, a: Vec3, b: Vec3) -> bool {
    let n = ((xz(b) - xz(a)).length() / 0.22).ceil().max(1.) as usize;
    if n > 160 {
        return false;
    }
    let mut prev = a;
    for i in 1..=n {
        let p = xz(a).lerp(xz(b), i as f32 / n as f32);
        let Some(next) = stand(s, p) else {
            return false;
        };
        if (next.y - prev.y).abs() > 0.16 + (p - xz(prev)).length() * 0.85 {
            return false;
        }
        prev = next;
    }
    true
}
fn clear_strike(s: &impl Space, a: Vec3, b: Vec3) -> bool {
    let n = (a.distance(b) / 0.18).ceil().max(1.) as usize;
    if n > 160 {
        return false;
    }
    (1..n).all(|i| {
        let p = a.lerp(b, i as f32 / n as f32);
        !s.blocked(xz(p), p.y - 0.8) && s.floor(xz(p)) < p.y - 0.15
    })
}
/// Small eight-neighbour A*: bounded work, no global navmesh or per-frame world scan.
fn route(s: &impl Space, start: Vec3, target: Vec3) -> Vec<Vec3> {
    if passage(s, start, target) {
        return vec![target];
    }
    let origin = xz(start);
    let cell = 0.85;
    let goal = (
        ((target.x - start.x) / cell).round() as i32,
        ((target.z - start.z) / cell).round() as i32,
    );
    let point = |k: (i32, i32)| origin + Vec2::new(k.0 as f32, k.1 as f32) * cell;
    let heuristic =
        |k: (i32, i32)| (((k.0 - goal.0).pow(2) + (k.1 - goal.1).pow(2)) as f32).sqrt() * 100.;
    let mut open = BinaryHeap::new();
    let mut costs = HashMap::new();
    let mut parents = HashMap::new();
    let mut positions = HashMap::new();
    open.push((-(heuristic((0, 0)) as i32), (0, 0)));
    costs.insert((0, 0), 0.);
    positions.insert((0, 0), start);
    for _ in 0..384 {
        let Some((_, k)) = open.pop() else { break };
        if k == goal {
            let mut out = vec![positions[&k]];
            let mut at = k;
            while let Some(&p) = parents.get(&at) {
                if p == (0, 0) {
                    break;
                }
                out.push(positions[&p]);
                at = p;
            }
            out.reverse();
            return out;
        }
        let a = positions[&k];
        let cost = costs[&k];
        for d in [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let q = (k.0 + d.0, k.1 + d.1);
            if q.0.abs() > 24 || q.1.abs() > 24 {
                continue;
            }
            let p = if let Some(p) = positions.get(&q) {
                *p
            } else {
                let Some(p) = stand(s, point(q)) else {
                    continue;
                };
                positions.insert(q, p);
                p
            };
            let next = cost + a.distance(p) * 100.;
            if costs.get(&q).is_some_and(|&v| v <= next) || !passage(s, a, p) {
                continue;
            }
            costs.insert(q, next);
            parents.insert(q, k);
            open.push((-(next + heuristic(q)) as i32, q));
        }
    }
    vec![]
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Approach,
    Windup,
    Strike,
    Recover,
    Stagger,
    Dead,
}
impl Phase {
    fn label(self) -> &'static str {
        match self {
            Self::Approach => "Closing in",
            Self::Windup => "Winding up",
            Self::Strike => "Striking",
            Self::Recover => "Recovering",
            Self::Stagger => "Staggered",
            Self::Dead => "Defeated",
        }
    }
}
#[derive(Serialize)]
pub struct Event {
    pub kind: &'static str,
    pub amount: f32,
    pub position: [f32; 3],
}
pub struct Combat {
    pub active: bool,
    pub armed: bool,
    pub enemy: Vec3,
    pub health: f32,
    pub kind: u32,
    pub phase: Phase,
    timer: f32,
    pub swing: f32,
    pub block: bool,
    attack_requested: bool,
    block_requested: bool,
    path: Vec<Vec3>,
    repath: f32,
    spawn: Vec3,
    origin: Vec3,
    yaw: f32,
    elapsed: f32,
    walking: bool,
    pub feedback: f32,
    pub hit_flash: f32,
    hit_pause: f32,
    recoil: Vec2,
    recoil_time: f32,
    visible: bool,
    visibility_timer: f32,
    events: Vec<Event>,
    pub result: u8,
}
impl Default for Combat {
    fn default() -> Self {
        Self {
            active: false,
            armed: false,
            enemy: Vec3::ZERO,
            health: 100.,
            kind: 0,
            phase: Phase::Approach,
            timer: 0.,
            swing: 0.,
            block: false,
            attack_requested: false,
            block_requested: false,
            path: vec![],
            repath: 0.,
            spawn: Vec3::ZERO,
            origin: Vec3::ZERO,
            yaw: 0.,
            elapsed: 0.,
            walking:false,
            feedback: 0.,
            hit_flash: 0.,
            hit_pause: 0.,
            recoil: Vec2::ZERO,
            recoil_time: 0.,
            visible: true,
            visibility_timer: 0.,
            events: vec![],
            result: 0,
        }
    }
}
impl Combat {
    pub fn max_health(&self)->f32 {[100.,72.,85.][self.kind.min(2) as usize]}
    pub fn spawn_at(&mut self,position:Vec3,kind:u32){
        let armed=self.armed;*self=Self{active:true,armed,enemy:position,spawn:position,kind:kind.min(2),..Self::default()};self.health=self.max_health();
    }
    pub fn input(&mut self, attack: bool, block: bool) {
        if attack {
            self.armed = true;
        }
        self.attack_requested |= attack;
        self.block_requested = block;
    }
    pub fn stop(&mut self) {
        let armed = self.armed;
        *self = Self::default();
        self.armed = armed;
    }
    pub fn toggle_weapon(&mut self) {
        self.armed = !self.armed;
        self.swing = 0.;
        self.block = false;
        self.attack_requested = false;
        self.block_requested = false;
    }
    pub fn begin(&mut self, s: &impl Space, origin: Vec3, yaw: f32) -> bool {
        // Search a safe nearby pair with a clear walkable lane. Never spawn in walls/water.
        let forward = facing(yaw);
        for radius in [0., 6., 12., 22., 36.] {
            for angle in 0..16 {
                let t = angle as f32 * std::f32::consts::TAU / 16.;
                let center = xz(origin) + Vec2::new(t.cos(), t.sin()) * radius;
                let Some(a) = stand(s, center) else { continue };
                let Some(b) = stand(s, center + forward * 5.5) else {
                    continue;
                };
                if !passage(s, a, b) {
                    continue;
                }
                // Enough room to strafe around the opponent rather than a narrow ledge.
                if [-1., 1.].into_iter().any(|side| {
                    stand(s, center + Vec2::new(forward.y, -forward.x) * 2.5 * side).is_none()
                }) {
                    continue;
                }
                *self = Self {
                    active: true,
                    armed: true,
                    enemy: b,
                    spawn: b,
                    origin: a,
                    yaw: yaw + std::f32::consts::PI,
                    ..Self::default()
                };
                return true;
            }
        }
        false
    }
    pub fn origin(&self) -> Vec3 {
        self.origin
    }
    pub(crate) fn emit(&mut self, kind: &'static str, amount: f32, p: Vec3) {
        if self.events.len() < 24 {
            self.events.push(Event {
                kind,
                amount,
                position: p.to_array(),
            });
        }
    }
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
    pub fn frame(&self, player: &Player) -> Vec<f32> {
        vec![
            self.armed as u8 as f32,
            self.swing,
            self.block as u8 as f32,
            self.health,
            self.enemy.x,
            self.enemy.y,
            self.enemy.z,
            self.phase as u8 as f32,
            self.feedback,
            self.hit_flash,
            self.result as f32,
            self.elapsed,
            player.position.x,
            player.eye().y,
            player.position.z,
            player.yaw,
            player.pitch,
            player.health,
            player.stamina,
            self.timer,
            self.visible as u8 as f32,
        ]
    }
    pub fn label(&self) -> &'static str {
        self.phase.label()
    }
    pub fn update(&mut self, s: &impl Space, p: &mut Player, dt: f32) {
        if p.health<=0. {self.attack_requested=false;self.block_requested=false;return;}
        if (!self.armed && !self.active) || !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(0.05);
        self.elapsed += dt;
        let enemy_before=self.enemy;self.walking=false;
        let motion_dt = if self.hit_pause > 0. {
            self.hit_pause = (self.hit_pause - dt).max(0.);
            0.
        } else {
            dt
        };
        self.feedback = (self.feedback - dt).max(0.);
        self.hit_flash = (self.hit_flash - dt).max(0.);
        let stats=p.character.derived();
        self.block = p.character.shield_equipped && self.armed && self.block_requested && self.swing == 0. && p.stamina > 1.;
        // Player locomotion normally regenerates 14/s. Holding guard/attacking suspends it.
        if self.block || self.swing > 0. {
            p.stamina = (p.stamina - dt * stats.stamina_regen).max(0.);
        }
        if self.attack_requested && p.character.sword_equipped && self.swing == 0. && !self.block && p.stamina >= stats.attack_cost {
            p.stamina -= stats.attack_cost;
            self.swing = 0.001;
        }
        self.attack_requested = false;
        if self.swing > 0. {
            let before = self.swing;
            self.swing += motion_dt;
            if before < 0.20 && self.swing >= 0.20 {
                self.emit("swing", 0., p.eye());
            }
            if before < 0.28 && self.swing >= 0.28 {
                let target = self.enemy + Vec3::Y * 1.0;
                let ray = target - p.eye();
                let look = Vec3::new(
                    p.yaw.sin() * p.pitch.cos(),
                    p.pitch.sin(),
                    -p.yaw.cos() * p.pitch.cos(),
                );
                if self.active
                    && self.result == 0
                    && ray.length() < 2.65
                    && ray.normalize_or_zero().dot(look) > 0.70
                    && clear_strike(s, p.eye(), target)
                {
                    let damage = if self.phase == Phase::Recover {
                        stats.damage * 1.36
                    } else {
                        stats.damage
                    };
                    self.health = (self.health - damage).max(0.);
                    self.hit_flash = 0.24;
                    self.hit_pause = 0.05;
                    self.recoil = (xz(self.enemy) - xz(p.position)).normalize_or_zero();
                    self.recoil_time = 0.18;
                    self.emit("hit", damage, target);
                    let ranked=p.character.blades.award(12);
                    self.emit(if ranked {"blades-rank"} else {"blades-xp"},if ranked {p.character.blades.rank as f32}else{12.},target);
                    // Committed attacks have poise: repeated clicks cannot keep
                    // the opponent permanently stun-locked. Recoil still draws.
                    if self.health == 0. {
                        self.phase = Phase::Dead;
                        self.timer = 0.;
                    } else if self.phase == Phase::Approach {
                        self.phase = Phase::Stagger;
                        self.timer = 0.;
                    }
                    if self.health == 0. {
                        self.result = 1;
                        self.emit("victory", 0., target);
                    }
                }
            }
            if self.swing >= 0.58 {
                self.swing = 0.;
            }
        }
        if !self.active || self.result != 0 {
            self.timer += motion_dt;
            return;
        }
        if self.recoil_time > 0. && motion_dt > 0. {
            let q = xz(self.enemy) + self.recoil * motion_dt * 1.8;
            if let Some(next) = stand(s, q) {
                if passage(s, self.enemy, next) {
                    self.enemy = next;
                }
            }
            self.recoil_time = (self.recoil_time - motion_dt).max(0.);
        }
        // Dynamic body separation: enemy never occupies the player's capsule.
        let delta = xz(p.position) - xz(self.enemy);
        let distance = delta.length();
        if distance < 0.85 && distance > 0.001 {
            let q = xz(p.position) - delta / distance * 0.85;
            if let Some(next) = stand(s, q) {
                if passage(s, self.enemy, next) {
                    self.enemy = next;
                }
            }
        }
        if matches!(self.phase, Phase::Approach | Phase::Recover) {
            self.yaw = delta.x.atan2(-delta.y);
        }
        self.visibility_timer -= dt;
        if self.visibility_timer <= 0. {
            self.visible = distance < 22. && clear_strike(s, p.eye(), self.enemy + Vec3::Y * 1.4);
            self.visibility_timer = 0.15;
        }
        self.timer += motion_dt;
        match self.phase {
            Phase::Approach => {
                if distance < 1.95
                    && (p.position.y - self.enemy.y).abs() < 1.0
                    && clear_strike(s, self.enemy + Vec3::Y * 1.2, p.position + Vec3::Y)
                {
                    self.phase = Phase::Windup;
                    self.timer = 0.;
                } else if distance < 22. && self.enemy.distance(self.spawn) < 30. {
                    self.repath -= motion_dt;
                    if self.repath <= 0. {
                        self.path = route(s, self.enemy, p.position);
                        self.repath = 0.65;
                    }
                    if let Some(target) = self.path.first().copied() {
                        let d = xz(target) - xz(self.enemy);
                        if d.length() < 0.16 {
                            self.path.remove(0);
                        } else if distance > 1.45 {
                            let q = xz(self.enemy)
                                + d.normalize() * ((motion_dt * [2.6,3.15,2.15][self.kind as usize]).min(d.length()));
                            if let Some(next) = stand(s, q) {
                                if passage(s, self.enemy, next) {
                                    self.enemy = next;
                                } else {
                                    self.path.clear();
                                }
                            }
                        }
                    }
                } else if distance>=22. && self.enemy.distance(self.spawn)>1. {
                    self.repath-=motion_dt;
                    if self.repath<=0.{self.path=route(s,self.enemy,self.spawn);self.repath=1.;}
                    if let Some(target)=self.path.first().copied(){let d=xz(target)-xz(self.enemy);if d.length()<0.2{self.path.remove(0);}else if let Some(next)=stand(s,xz(self.enemy)+d.normalize()*motion_dt*2.){if passage(s,self.enemy,next){self.enemy=next;}}}
                }
            }
            Phase::Windup => {
                if self.timer >= 0.68 {
                    self.phase = Phase::Strike;
                    self.timer = 0.;
                    self.emit("enemy-swing", 0., self.enemy);
                }
            }
            Phase::Strike => {
                if self.timer >= 0.10 {
                    if distance < 2.35
                        && facing(self.yaw).dot(delta.normalize_or_zero()) > 0.45
                        && (p.position.y - self.enemy.y).abs() < 1.2
                        && clear_strike(s, self.enemy + Vec3::Y * 1.2, p.position + Vec3::Y)
                    {
                        let toward = (xz(self.enemy) - xz(p.position)).normalize_or_zero();
                        if self.block && facing(p.yaw).dot(toward) > 0.35 && p.stamina >= stats.block_cost {
                            p.stamina -= stats.block_cost;
                            self.feedback = 0.20;
                            self.emit("block", 0., p.eye());
                            let ranked=p.character.blocking.award(16);
                            self.emit(if ranked {"blocking-rank"}else{"blocking-xp"},if ranked{p.character.blocking.rank as f32}else{16.},p.eye());
                        } else {
                            let broken = self.block;
                            let damage=([20.,15.,22.][self.kind as usize]-stats.protection).max(1.);
                            p.health = (p.health - damage).max(0.);
                            self.feedback = 0.32;
                            self.emit(if broken { "guard-break" } else { "hurt" }, damage, p.eye());
                            if broken {
                                p.stamina = 0.;
                                self.block = false;
                            }
                            if p.health == 0. {
                                self.result = 2;
                                self.emit("defeat", 0., p.eye());
                            }
                        }
                    }
                    self.phase = Phase::Recover;
                    self.timer = 0.;
                }
            }
            Phase::Recover => {
                if self.timer >= 0.85 {
                    self.phase = Phase::Approach;
                    self.timer = 0.;
                }
            }
            Phase::Stagger => {
                if self.timer >= 0.42 {
                    self.phase = Phase::Recover;
                    self.timer = 0.;
                }
            }
            Phase::Dead => {}
        }
        self.walking=self.enemy.distance_squared(enemy_before)>0.00001;
    }
    pub fn append_corpse(mesh:&mut MeshData,c:&crate::loot::Corpse,eye:Vec3,yaw:f32){
        let mut dead=Self::default();dead.active=true;dead.enemy=Vec3::from_array(c.position);dead.kind=c.kind;dead.phase=Phase::Dead;dead.timer=2.;dead.append_mesh(mesh,eye,yaw);
    }
    pub fn append_mesh(&self, mesh: &mut MeshData, eye: Vec3, yaw: f32) {
        if !self.active {
            return;
        }
        let dir = ((((eye.x - self.enemy.x).atan2(-(eye.z - self.enemy.z)) - self.yaw
            + std::f32::consts::FRAC_PI_4)
            .rem_euclid(std::f32::consts::TAU)
            / std::f32::consts::FRAC_PI_2)
            .floor() as u32)
            % 4;
        let pose = if self.phase == Phase::Dead {
            20 + ((self.timer / 0.14) as u32).min(3)
        } else if self.hit_flash > 0. {
            if self.hit_flash > 0.12 { 18 } else { 19 }
        } else {
            match self.phase {
                Phase::Approach => if self.walking {((self.elapsed * 10.) as u32) % 8} else {0},
                Phase::Windup => 8 + ((self.timer / 0.68 * 4.) as u32).min(3),
                Phase::Strike => 12 + ((self.timer / 0.10 * 3.) as u32).min(2),
                Phase::Recover => 15 + ((self.timer / 0.85 * 3.) as u32).min(2),
                Phase::Stagger => 18 + ((self.timer / 0.21) as u32).min(1),
                Phase::Dead => 23,
            }
        };
        let packed = 65536
            + self.kind
            + dir * 16
            + (pose % 4) * 64
            + (pose / 4) * 262144
            + 2 * 256
            + 3 * 2048
            + if self.hit_flash > 0.12 { 131072 } else { 0 };
        let start = mesh.vertices.len() as u32;
        let h = [1.94,1.40,2.15][self.kind as usize];
        let sway = if self.phase == Phase::Approach {
            (self.elapsed * 9.).sin() * 0.025
        } else {
            0.
        };
        for (x, y, uv) in [
            (-0.5, 0., [0., 1.]),
            (-0.5, 1., [0., 0.]),
            (0.5, 1., [1., 0.]),
            (0.5, 0., [1., 1.]),
        ] {
            mesh.vertices.push(Vertex {
                wind: 0.,
                position: [
                    self.enemy.x
                        + yaw.cos() * x * 1.3
                        + sway
                        + self.recoil.x * y * self.recoil_time,
                    self.enemy.y + y * h - 0.03 + if self.kind==2{0.12+(self.elapsed*2.).sin()*0.08}else{0.},
                    self.enemy.z
                        + yaw.sin() * x * 1.3
                        + self.recoil.y * y * self.recoil_time,
                ],
                normal: [-yaw.sin(), 0.12, yaw.cos()],
                color: if self.hit_flash > 0. {
                    [0.85, 0.24, 0.16]
                } else {
                    [0.46, 0.13, 0.10]
                },
                material: 11.,
                uv,
                texture: 1000. + packed as f32,
            });
        }
        mesh.indices
            .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Room {
        wall: bool,
    }
    impl Space for Room {
        fn floor(&self, _: Vec2) -> f32 {
            0.
        }
        fn blocked(&self, p: Vec2, _: f32) -> bool {
            self.wall && p.y.abs() < 0.2 && p.x.abs() < 2.
        }
    }
    fn player() -> Player {
        Player::combat_fixture(Vec3::new(0., 0., 1.), 0.)
    }
    fn duel() -> Combat {
        Combat {
            active: true,
            armed: true,
            yaw: std::f32::consts::PI,
            enemy: Vec3::new(0., 0., -1.),
            spawn: Vec3::new(0., 0., -1.),
            ..Combat::default()
        }
    }
    #[test]
    fn unequipped_items_cannot_attack_or_guard_and_creatures_have_distinct_vitals() {
        let mut c=duel();let mut p=player();p.character.sword_equipped=false;p.character.shield_equipped=false;
        c.input(true,true);for _ in 0..20{c.update(&Room{wall:false},&mut p,0.02);}
        assert_eq!(c.swing,0.);assert!(!c.block);assert_eq!(p.character.blades.xp,0);assert_eq!(p.character.blocking.xp,0);
        for (kind,hp) in [(0,100.),(1,72.),(2,85.)]{c.spawn_at(Vec3::ZERO,kind);assert_eq!(c.health,hp);assert_eq!(c.max_health(),hp);}
    }
    #[test]
    fn attacks_cannot_cross_walls() {
        let mut c = duel();
        let mut p = player();
        c.input(true, false);
        for _ in 0..14 {
            c.update(&Room { wall: true }, &mut p, 0.02);
        }
        assert_eq!(c.health, 100.);
        assert_eq!(p.character.blades.xp,0);
    }
    #[test]
    fn one_swing_hits_once_and_requires_facing() {
        let mut c = duel();
        let mut p = player();
        c.input(true, false);
        for _ in 0..40 {
            c.update(&Room { wall: false }, &mut p, 0.02);
        }
        assert_eq!(c.health, 75.);
        assert_eq!(p.character.blades.xp,12);
        c.health = 100.;
        p.yaw = 3.14;
        c.input(true, false);
        for _ in 0..40 {
            c.update(&Room { wall: false }, &mut p, 0.02);
        }
        assert_eq!(c.health, 100.);
    }
    #[test]
    fn guard_has_direction_stamina_and_attack_recovery() {
        let mut c = duel();
        let mut p = player();
        c.phase = Phase::Strike;
        c.input(false, true);
        c.update(&Room { wall: false }, &mut p, 0.05);
        c.update(&Room { wall: false }, &mut p, 0.05);
        assert_eq!(p.health, 100.);
        assert!(p.stamina < 81.);
        assert_eq!(c.phase, Phase::Recover);
        assert_eq!(p.character.blocking.xp,16);
        c.phase = Phase::Strike;
        c.timer = 0.;
        p.yaw = 3.14;
        c.update(&Room { wall: false }, &mut p, 0.05);
        c.update(&Room { wall: false }, &mut p, 0.05);
        assert_eq!(p.health, 82.);
    }
    #[test]
    fn weapons_work_without_an_encounter_and_sheathing_cancels_inputs() {
        let mut c = Combat::default();
        let mut p = player();
        c.input(true, false);
        for _ in 0..40 {
            c.update(&Room { wall: false }, &mut p, 0.02);
        }
        assert!(c.armed);
        assert!(!c.active);
        assert_eq!(c.health, 100.);
        assert_eq!(c.events.iter().filter(|e| e.kind == "swing").count(), 1);
        c.input(true, true);
        c.toggle_weapon();
        assert!(!c.armed);
        assert!(!c.attack_requested);
        assert!(!c.block);
    }
    #[test]
    fn impact_holds_the_swing_then_recoils_without_repeating_damage() {
        let mut c = duel();
        let mut p = player();
        c.input(true, false);
        for _ in 0..14 {
            c.update(&Room { wall: false }, &mut p, 0.02);
        }
        assert_eq!(c.health, 75.);
        let contact = c.swing;
        let before = c.enemy;
        c.update(&Room { wall: false }, &mut p, 0.02);
        assert_eq!(c.swing, contact);
        for _ in 0..10 {
            c.update(&Room { wall: false }, &mut p, 0.02);
        }
        assert!(c.enemy.distance(before) > 0.1);
        assert_eq!(c.health, 75.);
        assert_eq!(c.events.iter().filter(|e| e.kind == "hit").count(), 1);
    }
    #[test]
    fn sheathing_does_not_pause_the_opponent() {
        let mut c = duel();
        let mut p = player();
        c.phase = Phase::Strike;
        c.toggle_weapon();
        c.update(&Room { wall: false }, &mut p, 0.05);
        c.update(&Room { wall: false }, &mut p, 0.05);
        assert_eq!(p.health, 82.);
    }
    #[test]
    fn committed_attacks_are_not_stun_locked_and_exhausted_guards_break() {
        let mut c = duel();
        let mut p = player();
        c.phase = Phase::Windup;
        c.input(true, false);
        for _ in 0..16 {
            c.update(&Room { wall: false }, &mut p, 0.02);
        }
        assert_eq!(c.phase, Phase::Windup);
        assert_eq!(c.health, 75.);
        c = duel();
        c.phase = Phase::Strike;
        c.timer = 0.;
        p.stamina = 8.;
        c.input(false, true);
        c.update(&Room { wall: false }, &mut p, 0.05);
        c.update(&Room { wall: false }, &mut p, 0.05);
        assert_eq!(p.health, 82.);
        assert_eq!(p.stamina, 0.);
    }
    #[test]
    fn navigation_detours_and_never_cuts_corners() {
        let s = Room { wall: true };
        let a = Vec3::new(0., 0., 2.);
        let b = Vec3::new(0., 0., -2.);
        let r = route(&s, a, b);
        assert!(!r.is_empty());
        let mut prev = a;
        for p in r {
            assert!(passage(&s, prev, p));
            prev = p;
        }
        assert!(prev.distance(b) < 0.7);
    }
    #[test]
    fn paused_and_finished_encounters_do_not_damage_player() {
        let mut c = duel();
        let mut p = player();
        c.phase = Phase::Strike;
        c.update(&Room { wall: false }, &mut p, 0.);
        assert_eq!(p.health, 100.);
        c.result = 1;
        for _ in 0..100 {
            c.update(&Room { wall: false }, &mut p, 0.05);
        }
        assert_eq!(p.health, 100.);
    }
}
