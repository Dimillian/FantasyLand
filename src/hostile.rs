//! Shared seeded room/furniture recipe: render geometry and collision use the same boxes.
use crate::{
    geometry,
    world::{Landmark, World},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
#[derive(Clone, Debug)]
pub struct Part {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub material: f32,
    pub solid: bool,
}
#[derive(Clone, Debug)]
pub struct Layout {
    pub x: f32,
    pub z: f32,
    pub floor: f32,
    pub low: f32,
    pub half: [f32; 2],
    pub indoor: bool,
    pub parts: Vec<Part>,
    pub spawns: Vec<[f32; 3]>,
}
pub fn is_hostile(kind: &str) -> bool {
    matches!(
        kind,
        "enemy_camp" | "crypt" | "cemetery" | "mini_dungeon" | "haunted_grove"
    )
}
pub fn indoor(kind: &str) -> bool {
    matches!(kind, "crypt" | "mini_dungeon")
}
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    layouts: RefCell<HashMap<u32, Rc<Layout>>>,
    cells: RefCell<HashMap<(i32, i32), Vec<Rc<Layout>>>>,
}
impl Catalog {
    pub fn layout(&self, w: &World, l: &Landmark) -> Rc<Layout> {
        if let Some(v) = self.layouts.borrow().get(&l.id) {
            return v.clone();
        }
        let v = Rc::new(build(w, l));
        let mut c = self.layouts.borrow_mut();
        if c.len() >= 128 {
            c.clear();
        }
        c.insert(l.id, v.clone());
        v
    }
    pub fn near(&self, w: &World, x: f32, z: f32) -> Vec<Rc<Layout>> {
        let key = ((x / 96.).floor() as i32, (z / 96.).floor() as i32);
        if let Some(v) = self.cells.borrow().get(&key) {
            return v.clone();
        }
        let v = w
            .landmarks_near(key.0 as f32 * 96. + 48., key.1 as f32 * 96. + 48., 112.)
            .iter()
            .filter(|l| is_hostile(&l.kind))
            .map(|l| self.layout(w, l))
            .collect::<Vec<_>>();
        let mut c = self.cells.borrow_mut();
        if c.len() >= 64 {
            c.clear();
        }
        c.insert(key, v.clone());
        v
    }
}
impl Layout {
    pub fn contains(&self, x: f32, z: f32, pad: f32) -> bool {
        (x - self.x).abs() < self.half[0] + pad && (z - self.z).abs() < self.half[1] + pad
    }
    pub fn floor_at(&self, x: f32, z: f32, ground: f32) -> f32 {
        if self.contains(x, z, 0.) {
            return ground.max(self.floor);
        }
        // Broad southern stair approach; a smooth collision ramp avoids tiny step catches.
        let dz = self.z - self.half[1] - z;
        if (x - self.x).abs() < 2.2 && (0. ..=8.).contains(&dz) {
            return ground.max(self.floor - (self.floor - self.low + 0.2) * dz / 8.);
        }
        ground
    }
    pub fn blocked(&self, x: f32, y: f32, z: f32) -> bool {
        self.parts.iter().any(|p| {
            p.solid
                && (x - p.center[0]).abs() < p.size[0] * 0.5 + 0.32
                && (z - p.center[2]).abs() < p.size[2] * 0.5 + 0.32
                && y < p.center[1] + p.size[1] * 0.5
                && y + 1.75 > p.center[1] - p.size[1] * 0.5
        })
    }
}
fn build(w: &World, l: &Landmark) -> Layout {
    let inside = indoor(&l.kind);
    let half = if inside { [8., 12.] } else { [13., 13.] };
    let mut top = f32::NEG_INFINITY;
    let mut low = f32::INFINITY;
    for iz in 0..=8 {
        for ix in 0..=8 {
            let x = l.x - half[0] + ix as f32 * half[0] / 4.;
            let z = l.z - half[1] + iz as f32 * half[1] / 4.;
            for lod in 0..=2 {
                let h = geometry::terrain_surface_height_lod(w, x, z, lod);
                top = top.max(h);
                low = low.min(h);
            }
        }
    }
    let floor = top + 0.12;
    let mut out = Layout {
        x: l.x,
        z: l.z,
        floor,
        low,
        half,
        indoor: inside,
        parts: vec![],
        spawns: vec![],
    };
    let stone = if l.kind == "crypt" {
        [0.29, 0.32, 0.32]
    } else {
        [0.38, 0.33, 0.26]
    };
    let mut part = |x: f32, y: f32, z: f32, s: [f32; 3], c: [f32; 3], mat: f32, solid: bool| {
        out.parts.push(Part {
            center: [l.x + x, floor + y, l.z + z],
            size: s,
            color: c,
            material: mat,
            solid,
        })
    };
    part(
        0.,
        -(floor - low + 0.3) * 0.5,
        0.,
        [half[0] * 2., floor - low + 0.3, half[1] * 2.],
        stone,
        2.,
        false,
    );
    for step in 0..24 {
        let d = (step as f32 + 0.5) / 3.;
        let h = -(floor - low + 0.2) * d / 8.;
        part(
            0.,
            h - 0.12,
            -half[1] - d,
            [4.4, 0.24, 0.34],
            stone,
            2.,
            false,
        );
    }
    if inside {
        for x in [-8., 8.] {
            part(x, 2.4, 0., [0.65, 4.8, 24.6], stone, 2., true);
        }
        part(0., 2.4, 12., [16., 4.8, 0.65], stone, 2., true);
        for x in [-4.9, 4.9] {
            part(x, 2.4, -12., [6.2, 4.8, 0.65], stone, 2., true);
        }
        part(0., 4.05, -12., [3.6, 1.5, 0.65], stone, 2., true);
        // Two offset doorways lead through an antechamber, hall and burial chamber.
        let flip = if l.id & 1 == 0 { 1. } else { -1. };
        for (z, sign) in [(-4., flip), (4., -flip)] {
            for (x, width) in [(-5.6 * sign, 4.8), (3.4 * sign, 9.2)] {
                part(x, 2.15, z, [width, 4.3, 0.55], stone, 2., true);
            }
            part(-2.2 * sign, 3.8, z, [2., 1.3, 0.55], stone, 2., true);
        }
        // Stone ceiling has a narrow skylight; enclosure/shadow passes handle shelter.
        for x in [-4.55, 4.55] {
            part(x, 4.75, 0., [6.9, 0.6, 24.], stone, 2., true);
        }
        for z in [-8., 0., 8.] {
            for x in [-6.4, 6.4] {
                part(x, 0.55, z, [1.6, 1.1, 2.6], [0.38, 0.39, 0.36], 2., true);
                part(x, 1.14, z, [1.8, 0.16, 2.8], [0.47, 0.46, 0.40], 2., true);
            }
            part(0., 0.02, z, [2.7, 0.03, 4.], [0.20, 0.16, 0.17], 5., false);
        }
        for z in [-9., 1., 9.] {
            for x in [-7.5, 7.5] {
                part(x, 2.1, z, [0.22, 0.65, 0.22], [0.3, 0.19, 0.09], 3., false);
                part(x, 2.6, z, [0.22, 0.4, 0.22], [1., 0.43, 0.07], 12., false);
            }
        }

        out.spawns = vec![
            [l.x, floor, l.z - 8.],
            [l.x, floor, l.z],
            [l.x, floor, l.z + 8.],
        ];
    } else if l.kind == "cemetery" {
        for x in [-12.5, 12.5] {
            part(x, 0.65, 0., [0.7, 1.3, 26.], stone, 2., true);
        }
        part(0., 0.65, 12.5, [25., 1.3, 0.7], stone, 2., true);
        for row in 0..4 {
            for col in [-1., 1.] {
                let x = col * (4. + (l.id % 3) as f32);
                let z = -8. + row as f32 * 5.;
                part(x, 0.1, z, [2., 0.2, 3.3], [0.30, 0.26, 0.20], 2., false);
                part(x, 0.8, z + 1.5, [1.2, 1.6, 0.4], stone, 2., true);
                part(x, 1.25, z + 1.5, [1.65, 0.3, 0.45], stone, 2., true);
            }
        }
        out.spawns = vec![[l.x, floor, l.z - 7.], [l.x, floor, l.z + 6.]];
    } else if l.kind == "haunted_grove" {
        for i in 0..9 {
            let a = i as f32 * std::f32::consts::TAU / 9.;
            let h = 2.8 + (l.id.wrapping_add(i) % 4) as f32 * 0.5;
            part(
                a.cos() * 10.,
                h * 0.5,
                a.sin() * 10.,
                [1.3, h, 1.2],
                [0.24, 0.31, 0.30],
                2.,
                true,
            );
        }
        part(0., 0.55, 0., [3.2, 1.1, 2.], [0.25, 0.22, 0.29], 2., true);
        part(0., 1.4, 0., [0.3, 0.6, 0.3], [0.5, 0.3, 1.], 12., false);
        out.spawns = vec![[l.x - 3., floor, l.z - 5.], [l.x + 3., floor, l.z + 5.]];
    } else {
        for x in [-8., 8.] {
            for z in [-7., 6.] {
                part(x, 0.45, z, [3.2, 0.9, 2.], [0.30, 0.20, 0.11], 3., true);
                part(x, 0.93, z, [3.3, 0.08, 2.1], [0.38, 0.16, 0.12], 5., false);
            }
        }
        if l.kind == "enemy_camp" {
            for x in [-10., 10.] {
                for z in -5..6 {
                    part(
                        x,
                        1.6,
                        z as f32 * 2.,
                        [0.35, 3.2, 0.35],
                        [0.30, 0.23, 0.13],
                        3.,
                        true,
                    );
                }
            }
        }
        part(0., 0.15, 0., [2., 0.3, 2.], stone, 2., true);
        part(0., 0.65, 0., [0.5, 0.8, 0.5], [1., 0.4, 0.07], 12., false);
        out.spawns = vec![[l.x - 3., floor, l.z - 6.], [l.x + 3., floor, l.z + 6.]];
    }
    out
}
pub fn floor(w: &World, x: f32, z: f32, h: f32) -> f32 {
    w.hostile
        .near(w, x, z)
        .iter()
        .fold(h, |h, l| l.floor_at(x, z, h))
}
pub fn blocked(w: &World, x: f32, y: f32, z: f32) -> bool {
    w.hostile.near(w, x, z).iter().any(|l| l.blocked(x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_rooms_have_accessible_doors_and_safe_guards() {
        let w = World::new(1337);
        let (p, _) = w.spawn_view();
        let places = w.landmarks_near(p[0], p[1], 5500.);
        let hostile: Vec<_> = places.iter().filter(|l| is_hostile(&l.kind)).collect();
        assert!(!hostile.is_empty());
        for l in hostile.iter().take(12) {
            let a = w.hostile.layout(&w, l);
            let b = w.hostile.layout(&w, l);
            assert!(Rc::ptr_eq(&a, &b));
            assert!(a
                .parts
                .iter()
                .all(|p| p.center.iter().chain(&p.size).all(|n| n.is_finite())));
            for p in &a.spawns {
                assert!(
                    !a.blocked(p[0], p[1] + 0.05, p[2]),
                    "blocked guard {}",
                    l.kind
                );
            }
            assert!(
                !a.blocked(a.x, a.floor + 0.05, a.z - a.half[1]),
                "blocked entrance {}",
                l.kind
            );
            if a.indoor {
                assert!(a.blocked(a.x + 8., a.floor + 0.1, a.z));
                assert!(!a.blocked(
                    a.x - 2.2 * if l.id & 1 == 0 { 1. } else { -1. },
                    a.floor + 0.05,
                    a.z - 4.
                ));
            }
        }
    }
}
