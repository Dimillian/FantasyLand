//! Small procedural plant communities. Curved ribbons, serrated fronds and
//! low leaf masses retain readable silhouettes without textures or alpha cards.
use crate::{
    geometry::MeshData,
    world::{hash, rand01},
};
use std::f32::consts::TAU;

pub const KINDS: u32 = 8;
pub const TRIANGLES: u32 = 24;
fn r(seed: u32, salt: u32) -> f32 {
    rand01(hash(seed, salt as i32, 197))
}
fn tone(c: [f32; 3], f: f32) -> [f32; 3] {
    c.map(|v| v * f)
}
fn point(base: [f32; 3], a: f32, along: f32, side: f32, y: f32) -> [f32; 3] {
    [
        base[0] + a.sin() * along + a.cos() * side,
        base[1] + y,
        base[2] + a.cos() * along - a.sin() * side,
    ]
}
fn blade(m: &mut MeshData, base: [f32; 3], a: f32, h: f32, w: f32, bend: f32, color: [f32; 3]) {
    let l = point(base, a, 0., -w, 0.);
    let rr = point(base, a, 0., w, 0.);
    let ml = point(base, a, bend * 0.30, -w * 0.54, h * 0.56);
    let mr = point(base, a, bend * 0.30, w * 0.54, h * 0.56);
    let tip = point(base, a, bend, 0., h);
    m.quad(l, rr, mr, ml, tone(color, 0.83), 6.);
    m.triangle(ml, mr, tip, color, 6.);
}
fn turf(m: &mut MeshData, seed: u32) {
    for i in 0..7 {
        let a = r(seed, 10 + i) * TAU;
        let radius = r(seed, 30 + i) * 0.47;
        let base = [a.sin() * radius, 0., a.cos() * radius];
        blade(
            m,
            base,
            a + 0.2,
            0.25 + r(seed, 50 + i) * 0.38,
            0.045 + r(seed, 70 + i) * 0.035,
            0.22 + r(seed, 90 + i) * 0.26,
            [0.85 + r(seed, 110 + i) * 0.15; 3],
        );
    }
}
fn reed_bed(m: &mut MeshData, seed: u32) {
    for i in 0..4 {
        let a = r(seed, 120 + i) * TAU;
        let base = [a.sin() * 0.38, 0., a.cos() * 0.38];
        blade(
            m,
            base,
            a,
            0.70 + r(seed, 130 + i) * 0.60,
            0.035,
            0.22,
            [0.88; 3],
        );
    }
    for i in 0..3 {
        let a = r(seed, 140 + i) * TAU;
        let base = [a.sin() * 0.24, 0., a.cos() * 0.24];
        let y = 1.05 + r(seed, 150 + i) * 0.40;
        m.triangle(
            point(base, a, 0., -0.015, 0.),
            point(base, a, 0., 0.015, 0.),
            point(base, a, 0.04, 0., y),
            [0.83; 3],
            6.,
        );
        for side in 0..3 {
            let aa = side as f32 * TAU / 3.;
            let bb = (side + 1) as f32 * TAU / 3.;
            m.triangle(
                point(base, aa, 0.065, 0., y - 0.26),
                point(base, bb, 0.065, 0., y - 0.26),
                [base[0] + 0.04, y + 0.035, base[2]],
                [0.35, 0.25, 0.13],
                6.,
            );
        }
    }
}
fn fern_bed(m: &mut MeshData, seed: u32) {
    for i in 0..4 {
        let a = i as f32 * TAU / 4. + r(seed, 160) * TAU;
        let len = 0.74 + r(seed, 170 + i) * 0.31;
        let base = [0., 0., 0.];
        let mid = point(base, a, len * 0.46, 0., 0.46);
        let tip = point(base, a, len, 0., 0.20);
        let c = [0.82 + r(seed, 180 + i) * 0.18; 3];
        m.triangle(
            base,
            point(base, a, len * 0.46, -0.05, 0.46),
            tip,
            tone(c, 0.88),
            6.,
        );
        m.triangle(base, tip, point(base, a, len * 0.46, 0.05, 0.46), c, 6.);
        for part in 0..2 {
            let t = 0.32 + part as f32 * 0.28;
            let y = if part == 0 { 0.38 } else { 0.39 };
            for side in [-1., 1.] {
                let start = point(base, a, len * (t - 0.17), 0., y - 0.12);
                let edge = point(
                    base,
                    a,
                    len * t,
                    side * (0.25 - part as f32 * 0.06),
                    y - 0.045,
                );
                let end = if part == 0 {
                    mid
                } else {
                    point(base, a, len * (t + 0.24), 0., y - 0.065)
                };
                if side > 0. {
                    m.triangle(start, end, edge, c, 6.);
                } else {
                    m.triangle(start, edge, end, tone(c, 0.84), 6.);
                }
            }
        }
    }
}
fn flower_bed(m: &mut MeshData, variant: u32, seed: u32) {
    for i in 0..3 {
        let a = r(seed, 190 + i) * TAU;
        let base = [a.sin() * 0.46, 0., a.cos() * 0.46];
        let blue = variant >= 6;
        let y = if blue {
            0.75 + r(seed, 200 + i) * 0.28
        } else {
            0.32 + r(seed, 200 + i) * 0.27
        };
        let top = point(base, a, 0.07, 0., y);
        m.triangle(
            point(base, a, 0., -0.018, 0.),
            point(base, a, 0., 0.018, 0.),
            top,
            [0.88; 3],
            6.,
        );
        for side in [-1., 1.] {
            m.triangle(
                point(base, a, 0.02, 0., y * 0.23),
                point(base, a, 0.06, side * 0.23, y * 0.47),
                point(base, a, 0.05, 0., y * 0.58),
                [0.90; 3],
                6.,
            );
        }
        if blue {
            let c = [0.38, 0.43, 0.81];
            for side in 0..4 {
                let aa = side as f32 * TAU / 4.;
                let bb = (side + 1) as f32 * TAU / 4.;
                m.triangle(
                    point(top, aa, 0.10, 0., -0.11),
                    point(top, bb, 0.10, 0., -0.11),
                    [top[0] + 0.03, top[1] + 0.24, top[2]],
                    tone(c, 0.88 + side as f32 * 0.04),
                    6.,
                );
            }
            m.triangle(
                [top[0] - 0.10, y - 0.12, top[2]],
                [top[0] + 0.10, y - 0.12, top[2]],
                [top[0], y + 0.10, top[2] + 0.11],
                c,
                6.,
            );
        } else {
            let c = if variant < 3 {
                [0.95, 0.93, 0.79]
            } else {
                [0.97, 0.74, 0.16]
            };
            let radius = 0.12 + r(seed, 210 + i) * 0.07;
            for side in 0..4 {
                let aa = side as f32 * TAU / 4.;
                m.triangle(
                    [top[0], y + 0.015, top[2]],
                    point(top, aa, radius, -radius * 0.45, 0.025),
                    point(top, aa, radius, radius * 0.45, 0.025),
                    tone(c, 0.94 + side as f32 * 0.015),
                    6.,
                );
            }
            m.triangle(
                [top[0] - 0.05, y + 0.03, top[2] - 0.04],
                [top[0], y + 0.031, top[2] + 0.05],
                [top[0] + 0.05, y + 0.03, top[2] - 0.04],
                [0.76, 0.49, 0.12],
                6.,
            );
        }
    }
}
fn bush(m: &mut MeshData, seed: u32, heather: bool) {
    for i in 0..3 {
        let a = r(seed, 220 + i) * TAU;
        let base = [a.sin() * 0.36, 0., a.cos() * 0.36];
        let h = 0.27 + r(seed, 230 + i) * 0.29;
        let radius = 0.32 + r(seed, 240 + i) * 0.16;
        for side in 0..4 {
            let aa = a + side as f32 * TAU / 4.;
            let bb = a + (side + 1) as f32 * TAU / 4.;
            let l = point(base, aa, radius, 0., h * 0.42);
            let rr = point(base, bb, radius, 0., h * 0.42);
            m.triangle(base, rr, l, [0.67; 3], 6.);
            let c = if heather && (i + side) % 3 != 0 {
                [0.52, 0.32, 0.43]
            } else {
                [0.87 + side as f32 * 0.025; 3]
            };
            m.triangle(l, rr, [base[0] + 0.05, h, base[2]], c, 6.);
        }
    }
}
fn seed_bed(m: &mut MeshData, seed: u32) {
    for i in 0..4 {
        let a = r(seed, 250 + i) * TAU;
        blade(
            m,
            [a.sin() * 0.25, 0., a.cos() * 0.25],
            a,
            0.32 + r(seed, 260 + i) * 0.32,
            0.035,
            0.36,
            [0.90; 3],
        );
    }
    for i in 0..3 {
        let a = r(seed, 270 + i) * TAU;
        let base = [a.sin() * 0.32, 0., a.cos() * 0.32];
        let y = 0.81 + r(seed, 280 + i) * 0.27;
        m.triangle(
            point(base, a, 0., -0.012, 0.),
            point(base, a, 0., 0.012, 0.),
            point(base, a, 0.13, 0., y),
            [0.90; 3],
            6.,
        );
        for j in 0..3 {
            let h = y - 0.09 + j as f32 * 0.085;
            m.triangle(
                point(base, a, 0.11, -0.055, h - 0.05),
                point(base, a, 0.20, 0.075, h),
                point(base, a, 0.13, 0., h + 0.075),
                [0.73, 0.59, 0.30],
                6.,
            );
        }
    }
}
fn litter(m: &mut MeshData, seed: u32) {
    for i in 0..8 {
        let a = r(seed, 290 + i) * TAU;
        let radius = r(seed, 310 + i) * 0.70;
        let base = [a.sin() * radius, 0., a.cos() * radius];
        let c = if i % 3 == 0 {
            [0.36, 0.39, 0.18]
        } else {
            [0.42, 0.30, 0.15]
        };
        m.triangle(
            point(base, a, -0.13, 0., 0.025),
            point(base, a, 0., -0.08, 0.035),
            point(base, a, 0.16, 0., 0.055),
            c,
            6.,
        );
        m.triangle(
            point(base, a, -0.13, 0., 0.025),
            point(base, a, 0.16, 0., 0.055),
            point(base, a, 0., 0.08, 0.028),
            tone(c, 0.8),
            6.,
        );
    }
    for i in 0..3 {
        let a = r(seed, 330 + i) * TAU;
        let base = [
            (r(seed, 340 + i) - 0.5) * 0.6,
            0.,
            (r(seed, 350 + i) - 0.5) * 0.6,
        ];
        m.quad(
            point(base, a, -0.40, -0.022, 0.04),
            point(base, a, 0.33, -0.017, 0.07),
            point(base, a, 0.33, 0.017, 0.075),
            point(base, a, -0.40, 0.022, 0.045),
            [0.30, 0.23, 0.14],
            6.,
        );
    }
}
pub fn template(kind: u32, variant: u32) -> MeshData {
    let seed = hash(0x434f5645, kind as i32, variant as i32);
    let mut mesh = MeshData::default();
    match kind {
        1 => reed_bed(&mut mesh, seed),
        2 => fern_bed(&mut mesh, seed),
        3 => flower_bed(&mut mesh, variant, seed),
        4 => bush(&mut mesh, seed, true),
        5 => seed_bed(&mut mesh, seed),
        6 => bush(&mut mesh, seed, false),
        7 => litter(&mut mesh, seed),
        _ => turf(&mut mesh, seed),
    }
    mesh
}
