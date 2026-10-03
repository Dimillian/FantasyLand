use crate::{
    geometry,
    world::{World, WORLD_SIZE},
};
use glam::Vec3;

pub struct Player {
    pub character: crate::progression::Character,
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub grounded: bool,
    pub stamina: f32,
    pub health: f32,
    pub mana: f32,
    pub walked: f32,
    pub speed: f32,
    velocity_y: f32,
    bob: f32,
    jump_was_down: bool,
    precise_x: f64,
    precise_z: f64,
}

impl Player {
    #[cfg(test)]
    pub(crate) fn combat_fixture(position: Vec3, yaw: f32) -> Self {
        Self {
            character: Default::default(),
            position,
            yaw,
            pitch: 0.,
            grounded: true,
            stamina: 100.,
            health: 100.,
            mana: 100.,
            walked: 0.,
            speed: 0.,
            velocity_y: 0.,
            bob: 0.,
            jump_was_down: false,
            precise_x: position.x as f64,
            precise_z: position.z as f64,
        }
    }
    pub fn new(world: &World, x: f32, z: f32) -> Self {
        Self {
            character: Default::default(),
            position: Vec3::new(x, Self::floor(world, x, z), z),
            yaw: 0.0,
            pitch: -0.035,
            grounded: true,
            stamina: 100.0,
            health: 100.0,
            mana: 100.0,
            walked: 0.0,
            speed: 0.0,
            velocity_y: 0.0,
            bob: 0.0,
            jump_was_down: false,
            precise_x: x as f64,
            precise_z: z as f64,
        }
    }
    fn floor(world: &World, x: f32, z: f32) -> f32 {
        let sample = world.sample(x, z);
        geometry::walk_height(world, x, z).max(sample.water_height - 1.0)
    }
    pub fn look(&mut self, dx: f32, dy: f32) {
        if dx.is_finite() && dy.is_finite() {
            self.yaw = (self.yaw + dx * 0.0022).rem_euclid(std::f32::consts::TAU);
            self.pitch = (self.pitch - dy * 0.0022).clamp(-1.42, 1.42);
        }
    }
    pub(crate) fn block_combat_overlap(&mut self, before: Vec3, enemy: Vec3) {
        let delta = glam::Vec2::new(self.position.x - enemy.x, self.position.z - enemy.z);
        if delta.length() < 0.72 && (self.position.y - enemy.y).abs() < 1.65 {
            self.walked = (self.walked
                - glam::Vec2::new(self.position.x - before.x, self.position.z - before.z).length())
            .max(0.);
            self.position.x = before.x;
            self.position.z = before.z;
            self.precise_x = before.x as f64;
            self.precise_z = before.z as f64;
            self.speed = 0.;
        }
    }
    pub fn eye(&self) -> Vec3 {
        self.position + Vec3::Y * (1.72 + self.bob.sin() * 0.025 * (self.speed / 5.5).min(1.0))
    }
    pub fn teleport(&mut self, world: &World, x: f32, z: f32) {
        let bound = WORLD_SIZE * 0.5 - 12.0;
        let mut destination = [x.clamp(-bound, bound), z.clamp(-bound, bound)];
        if geometry::blocks_player(world, destination[0], destination[1]) {
            'search: for radius in [1.0, 2.0, 4.0, 8.0, 12.0, 20.0, 40.0, 80.0, 120.0] {
                for i in 0..16 {
                    let angle = i as f32 * std::f32::consts::TAU / 16.0;
                    let p = [
                        (x + angle.sin() * radius).clamp(-bound, bound),
                        (z + angle.cos() * radius).clamp(-bound, bound),
                    ];
                    if !geometry::blocks_player(world, p[0], p[1]) {
                        destination = p;
                        break 'search;
                    }
                }
            }
        }
        if geometry::blocks_player(world, destination[0], destination[1]) {
            destination = world.spawn();
        }
        let [x, z] = destination;
        self.position = Vec3::new(x, Self::floor(world, x, z), z);
        self.precise_x = x as f64;
        self.precise_z = z as f64;
        self.velocity_y = 0.0;
        self.grounded = true;
        self.speed = 0.0;
    }
    pub fn update(
        &mut self,
        world: &World,
        dt: f32,
        forward: f32,
        strafe: f32,
        sprint: bool,
        jump: bool,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let forward = if forward.is_finite() {
            forward.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let strafe = if strafe.is_finite() {
            strafe.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let mut direction = Vec3::new(
            self.yaw.sin() * forward + self.yaw.cos() * strafe,
            0.0,
            -self.yaw.cos() * forward + self.yaw.sin() * strafe,
        );
        if direction.length_squared() > 1.0 {
            direction = direction.normalize();
        }
        let moving = direction.length_squared() > 0.01;
        let sprinting = sprint && moving && self.stamina > 1.0;
        self.stamina =
            (self.stamina + if sprinting { -11.0 * dt } else { self.character.derived().stamina_regen * dt }).clamp(0.0, self.character.derived().max_stamina);
        let sample = world.sample(self.position.x, self.position.z);
        let swimming = sample.water_height
            > geometry::walk_height(world, self.position.x, self.position.z) + 1.1;
        let pace = if swimming {
            2.5
        } else if sprinting {
            9.0
        } else {
            5.5
        };
        let delta = direction * pace * dt;
        let before = (self.precise_x, self.precise_z);
        let bound = WORLD_SIZE * 0.5 - 12.0;
        // Axis-separated movement slides naturally along trunks, rocks and steep rises.
        for (dx, dz) in [(delta.x, 0.0), (0.0, delta.z)] {
            let px = (self.precise_x + dx as f64).clamp(-bound as f64, bound as f64);
            let pz = (self.precise_z + dz as f64).clamp(-bound as f64, bound as f64);
            let x = px as f32;
            let z = pz as f32;
            let h = Self::floor(world, x, z);
            let rise = h - self.position.y;
            let can_climb = rise < 0.10 + 1.45 * (dx * dx + dz * dz).sqrt()
                || (!self.grounded && h < self.position.y + 0.22);
            if can_climb && !geometry::blocks_body(world, x, self.position.y.max(h), z) {
                self.position.x = x;
                self.position.z = z;
                self.precise_x = px;
                self.precise_z = pz;
            }
        }
        let traveled = ((before.0 - self.precise_x).powi(2) + (before.1 - self.precise_z).powi(2))
            .sqrt() as f32;
        self.walked += traveled;
        self.speed = traveled / dt;
        self.bob += traveled * 2.3;
        if jump && !self.jump_was_down && self.grounded {
            self.velocity_y = 5.5;
            self.grounded = false;
        }
        self.jump_was_down = jump;
        let ground = Self::floor(world, self.position.x, self.position.z);
        if self.grounded && self.position.y - ground < 0.65 && self.velocity_y <= 0.0 {
            self.position.y = ground;
            self.velocity_y = 0.0;
        } else {
            self.velocity_y -= 18.0 * dt;
            self.position.y += self.velocity_y * dt;
            self.grounded = false;
            if self.position.y <= ground {
                self.position.y = ground;
                self.velocity_y = 0.0;
                self.grounded = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fast_travel_avoids_solid_props() {
        let world = World::new(1337);
        let s = world.spawn();
        let mut p = Player::new(&world, s[0], s[1]);
        p.teleport(&world, -199.52692, 5580.3286);
        assert!(!geometry::blocks_player(&world, p.position.x, p.position.z));
    }
    #[test]
    fn movement_jump_and_recovery() {
        let world = World::new(1337);
        let (s, yaw) = world.spawn_view();
        let mut p = Player::new(&world, s[0], s[1]);
        p.yaw = yaw;
        let initial = p.position;
        for _ in 0..120 {
            p.update(&world, 1.0 / 60.0, 1.0, 0.0, false, false);
        }
        assert!(
            (p.position - initial).length() > 2.0,
            "spawn road must be walkable"
        );
        let y = p.position.y;
        p.update(&world, 1.0 / 60.0, 0.0, 0.0, false, true);
        for _ in 0..8 {
            p.update(&world, 1.0 / 60.0, 0.0, 0.0, false, false);
        }
        assert!(p.position.y > y + 0.1);
        for _ in 0..120 {
            p.update(&world, 1.0 / 60.0, 0.0, 0.0, false, false);
        }
        assert!(p.grounded);
        assert!((p.position.y - Player::floor(&world, p.position.x, p.position.z)).abs() < 0.01);
    }
    #[test]
    fn diagonal_speed_is_normalized() {
        let w = World::new(1337);
        let s = w.spawn();
        let mut p = Player::new(&w, s[0], s[1]);
        p.update(&w, 0.01, 1.0, 1.0, false, false);
        assert!(p.speed < 5.51);
    }
}
