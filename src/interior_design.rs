//! Purpose-built rooms assembled from reproducible furniture recipes. The same
//! solid bounds are consumed by collision; ornaments never block a doorway.
use crate::{
    geometry::MeshData,
    settlement_mesh::block,
    settlements::{Building, Use},
};
const WOOD: [f32; 3] = [0.43, 0.28, 0.15];
const DARK: [f32; 3] = [0.25, 0.15, 0.085];
const IRON: [f32; 3] = [0.24, 0.25, 0.24];
const BRASS: [f32; 3] = [0.69, 0.49, 0.23];
const LINEN: [f32; 3] = [0.82, 0.74, 0.55];
const JUG: &[(f32, f32)] = &[
    (0.02, 0.62),
    (0.12, 0.82),
    (0.40, 1.0),
    (0.65, 0.91),
    (0.79, 0.48),
    (0.95, 0.43),
    (1., 0.53),
    (1., 0.33),
    (0.84, 0.30),
];
const BOTTLE: &[(f32, f32)] = &[
    (0., 0.65),
    (0.08, 0.8),
    (0.6, 0.8),
    (0.72, 0.34),
    (0.94, 0.29),
    (1., 0.4),
    (1., 0.23),
    (0.92, 0.21),
];
const CUP: &[(f32, f32)] = &[
    (0., 0.67),
    (0.08, 0.80),
    (0.90, 1.0),
    (1., 1.0),
    (1., 0.79),
    (0.17, 0.56),
];
const PLATE: &[(f32, f32)] = &[
    (0., 0.),
    (0., 0.72),
    (0.35, 1.),
    (0.75, 1.),
    (0.8, 0.87),
    (0.3, 0.56),
    (0.3, 0.),
];
const BARREL: &[(f32, f32)] = &[
    (0., 0.77),
    (0.10, 0.88),
    (0.38, 1.),
    (0.65, 1.),
    (0.92, 0.86),
    (1., 0.77),
];
const RING: &[(f32, f32)] = &[(0., 1.), (1., 1.), (1., 0.88), (0., 0.88), (0., 1.)];
const CANDLE: &[(f32, f32)] = &[(0., 0.94), (0.92, 0.95), (1., 0.7)];
#[derive(Clone, Copy)]
pub enum Shape {
    Box,
    Turned {
        profile: &'static [(f32, f32)],
        at: [f32; 3],
        radius: f32,
        height: f32,
    },
}
#[derive(Clone, Copy)]
pub struct Part {
    pub lo: [f32; 3],
    pub hi: [f32; 3],
    pub color: [f32; 3],
    pub texture: f32,
    pub solid: bool,
    pub shape: Shape,
}
struct Room<'a> {
    out: &'a mut dyn FnMut(Part),
    seed: u32,
    cloth: [f32; 3],
}
impl Room<'_> {
    fn cuboid(&mut self, lo: [f32; 3], hi: [f32; 3], color: [f32; 3], texture: f32, solid: bool) {
        (self.out)(Part {
            lo,
            hi,
            color,
            texture,
            solid,
            shape: Shape::Box,
        });
    }
    fn wood(&mut self, lo: [f32; 3], hi: [f32; 3]) {
        self.cuboid(lo, hi, WOOD, 4., true);
    }
    fn turned(
        &mut self,
        at: [f32; 3],
        radius: f32,
        height: f32,
        profile: &'static [(f32, f32)],
        color: [f32; 3],
        texture: f32,
        solid: bool,
    ) {
        (self.out)(Part {
            lo: [at[0] - radius, at[1], at[2] - radius],
            hi: [at[0] + radius, at[1] + height, at[2] + radius],
            color,
            texture,
            solid,
            shape: Shape::Turned {
                profile,
                at,
                radius,
                height,
            },
        });
    }
    fn mug(&mut self, x: f32, y: f32, z: f32) {
        self.turned([x, y, z], 0.09, 0.17, CUP, [0.63, 0.45, 0.26], 26., false);
        for (yy, zz) in [(y + 0.035, z - 0.1), (y + 0.125, z - 0.1)] {
            self.cuboid(
                [x - 0.026, yy, zz - 0.08],
                [x + 0.026, yy + 0.025, zz],
                BRASS,
                12.,
                false,
            );
        }
        self.cuboid(
            [x - 0.025, y + 0.04, z - 0.18],
            [x + 0.025, y + 0.15, z - 0.15],
            BRASS,
            12.,
            false,
        );
    }
    fn candle(&mut self, x: f32, y: f32, z: f32) {
        self.turned([x, y, z], 0.11, 0.025, PLATE, BRASS, 12., false);
        self.turned([x, y + 0.025, z], 0.033, 0.18, CANDLE, LINEN, 11., false);
    }
    fn book(&mut self, x: f32, y: f32, z: f32, standing: bool, n: u32) {
        let c = [
            [0.40, 0.17, 0.12],
            [0.17, 0.28, 0.25],
            [0.30, 0.26, 0.40],
            [0.43, 0.33, 0.17],
        ][(n % 4) as usize];
        let (w, h, d) = if standing {
            (0.11, 0.25 + (n % 3) as f32 * 0.04, 0.23)
        } else {
            (0.30, 0.065, 0.25)
        };
        self.cuboid([x, y, z], [x + w, y + h, z + d], c, 25., false);
        self.cuboid(
            [x + 0.012, y + 0.013, z + 0.015],
            [x + w - 0.012, y + h - 0.013, z + d + 0.004],
            LINEN,
            11.,
            false,
        );
        if standing {
            for yy in [y + 0.035, y + h - 0.055] {
                self.cuboid(
                    [x - 0.002, yy, z + d],
                    [x + w + 0.002, yy + 0.015, z + d + 0.012],
                    BRASS,
                    12.,
                    false,
                );
            }
        }
    }
    fn barrel(&mut self, x: f32, z: f32, scale: f32) {
        self.turned(
            [x, 0., z],
            0.40 * scale,
            0.94 * scale,
            BARREL,
            WOOD,
            21.,
            true,
        );
        for y in [0.12, 0.45, 0.80] {
            let rad = if y == 0.45 { 0.407 } else { 0.371 };
            self.turned(
                [x, y * scale, z],
                rad * scale,
                0.047 * scale,
                RING,
                IRON,
                12.,
                false,
            );
        }
        self.turned(
            [x, 0.92 * scale, z],
            0.315 * scale,
            0.02,
            CANDLE,
            DARK,
            4.,
            false,
        );
    }
    fn chair(&mut self, x: f32, z: f32, face: f32) {
        self.wood([x - 0.30, 0.43, z - 0.29], [x + 0.30, 0.52, z + 0.29]);
        for dx in [-0.26, 0.19] {
            for dz in [-0.25, 0.18] {
                self.wood([x + dx, 0., z + dz], [x + dx + 0.07, 0.46, z + dz + 0.07]);
            }
        }
        let zz = z + face * 0.26;
        for dx in [-0.28, 0.21] {
            self.wood([x + dx, 0.48, zz - 0.04], [x + dx + 0.07, 1.13, zz + 0.04]);
        }
        for y in [0.70, 0.96] {
            self.wood([x - 0.28, y, zz - 0.04], [x + 0.28, y + 0.10, zz + 0.04]);
        }
    }
    fn table(&mut self, x: f32, z: f32, long: bool) {
        let d = if long { 1.05 } else { 0.65 };
        self.wood([x - 0.66, 0.78, z - d], [x + 0.66, 0.9, z + d]);
        for dx in [-0.51, 0.41] {
            for dz in [-d + 0.13, d - 0.23] {
                self.wood([x + dx, 0., z + dz], [x + dx + 0.10, 0.8, z + dz + 0.10]);
            }
        }
        self.wood([x - 0.45, 0.22, z - 0.07], [x + 0.45, 0.32, z + 0.07]);
        self.cuboid(
            [x - 0.28, 0.902, z - d + 0.08],
            [x + 0.28, 0.915, z + d - 0.08],
            self.cloth,
            24.,
            false,
        );
        for dz in [-d * 0.62, d * 0.62] {
            self.turned(
                [x - 0.23, 0.919, z + dz],
                0.19,
                0.028,
                PLATE,
                [0.71, 0.66, 0.49],
                26.,
                false,
            );
            self.mug(x + 0.31, 0.915, z + dz + 0.09);
        }
        self.turned(
            [x + 0.1, 0.915, z],
            0.13,
            0.28,
            JUG,
            [0.29, 0.43, 0.34],
            26.,
            false,
        );
        self.chair(x, z - d - 0.50, -1.);
        self.chair(x, z + d + 0.50, 1.);
    }
    fn shelf(&mut self, x: f32, z: f32, width: f32, books: bool) {
        // Open-backed shelves: objects have real silhouettes between uprights.
        for dx in [0., width - 0.09] {
            self.wood([x + dx, 0., z], [x + dx + 0.09, 2.3, z + 0.43]);
        }
        for (row, y) in [0.20, 0.85, 1.5, 2.20].into_iter().enumerate() {
            self.wood(
                [x - 0.025, y, z - 0.025],
                [x + width + 0.025, y + 0.08, z + 0.47],
            );
            if row == 3 {
                continue;
            }
            let count = (width / 0.24) as u32;
            for n in 0..count {
                let xx = x + 0.16 + n as f32 * (width - 0.24) / count as f32;
                if books {
                    self.book(xx, y + 0.08, z + 0.11, true, n + self.seed + row as u32);
                } else {
                    let c = [[0.20, 0.39, 0.28], [0.48, 0.28, 0.16], [0.28, 0.38, 0.49]]
                        [((n + row as u32) % 3) as usize];
                    self.turned(
                        [xx, y + 0.08, z + 0.24],
                        0.065,
                        0.27 + (n % 3) as f32 * 0.04,
                        BOTTLE,
                        c,
                        26.,
                        false,
                    );
                }
            }
        }
    }
    fn bed(&mut self, x: f32, z: f32, canopy: bool) {
        let c = self.cloth;
        self.wood([x - 0.64, 0.18, z], [x + 0.64, 0.39, z + 2.05]);
        self.cuboid(
            [x - 0.60, 0.39, z + 0.06],
            [x + 0.60, 0.59, z + 2.0],
            LINEN,
            11.,
            true,
        );
        self.cuboid(
            [x - 0.61, 0.59, z + 0.05],
            [x + 0.61, 0.64, z + 1.53],
            c,
            24.,
            false,
        );
        for dx in [-0.63, 0.56] {
            for zz in [z, z + 1.96] {
                self.wood(
                    [x + dx, 0., zz],
                    [x + dx + 0.07, if canopy { 2.55 } else { 1.0 }, zz + 0.08],
                );
                self.turned(
                    [x + dx + 0.035, if canopy { 2.55 } else { 1.0 }, zz + 0.04],
                    0.065,
                    0.09,
                    CANDLE,
                    BRASS,
                    12.,
                    false,
                );
            }
        }
        self.wood([x - 0.58, 0.43, z + 1.98], [x + 0.58, 0.96, z + 2.06]);
        self.wood([x - 0.60, 0.21, z], [x + 0.60, 0.57, z + 0.08]);
        self.cuboid(
            [x - 0.46, 0.60, z + 1.62],
            [x + 0.46, 0.76, z + 1.96],
            LINEN,
            11.,
            false,
        );
        if canopy {
            self.cuboid(
                [x - 0.69, 2.51, z - 0.04],
                [x + 0.69, 2.59, z + 2.10],
                c,
                24.,
                false,
            );
            for side in [-1., 1.] {
                self.cuboid(
                    [x + side * 0.62 - 0.035, 1.15, z + 1.52],
                    [x + side * 0.62 + 0.035, 2.52, z + 2.05],
                    c,
                    24.,
                    false,
                );
            }
        }
    }
    fn chest(&mut self, x: f32, z: f32) {
        self.wood([x - 0.47, 0.05, z - 0.27], [x + 0.47, 0.54, z + 0.27]);
        for dx in [-0.31, 0.27] {
            self.cuboid(
                [x + dx, 0.07, z - 0.285],
                [x + dx + 0.045, 0.56, z + 0.285],
                IRON,
                12.,
                false,
            );
        }
        self.cuboid(
            [x - 0.065, 0.34, z - 0.30],
            [x + 0.065, 0.48, z - 0.275],
            BRASS,
            12.,
            false,
        );
    }
}
pub fn generate(b: &Building, mut emit: impl FnMut(Part)) {
    let [w, d] = b.half;
    let cloth = [
        [0.43, 0.17, 0.13],
        [0.22, 0.35, 0.32],
        [0.27, 0.28, 0.44],
        [0.52, 0.36, 0.17],
    ][(b.id % 4) as usize];
    let mut r = Room {
        out: &mut emit,
        seed: b.id,
        cloth,
    };
    if b.usage == Use::Tent {
        for side in [-1., 1.] {
            let x = side * w * 0.53;
            r.cuboid(
                [x - 0.42, 0.03, -d + 0.9],
                [x + 0.42, 0.16, d - 1.1],
                cloth,
                24.,
                true,
            );
            r.cuboid(
                [x - 0.34, 0.16, d - 1.5],
                [x + 0.34, 0.28, d - 1.16],
                LINEN,
                11.,
                false,
            );
        }
        r.chest(w - 0.85, d - 0.65);
        r.cuboid(
            [-0.42, 0., d + 0.44],
            [0.42, 0.18, d + 1.27],
            [0.38, 0.36, 0.30],
            17.,
            true,
        );
        r.wood([-0.29, 0.18, d + 0.66], [0.29, 0.31, d + 1.05]);
        return;
    }
    let open = matches!(b.usage, Use::Stable | Use::Market);
    if !open {
        // Low wall panelling, plaster above, cornice and closely spaced ceiling joists.
        for sign in [-1., 1.] {
            r.cuboid(
                [sign * w - 0.13, 0.1, -d + 0.13],
                [sign * w + 0.13, 0.91, d - 0.13],
                DARK,
                25.,
                true,
            );
            r.wood(
                [sign * w - 0.17, 0.89, -d + 0.10],
                [sign * w + 0.17, 1.0, d - 0.10],
            );
            for (a, c) in if sign < 0. {
                vec![(-w, -0.95), (0.95, w)]
            } else {
                vec![(-w, w)]
            } {
                r.cuboid(
                    [a, 0.10, sign * d - 0.15],
                    [c, 0.91, sign * d + 0.15],
                    DARK,
                    25.,
                    true,
                );
                r.wood([a, 0.89, sign * d - 0.18], [c, 1.0, sign * d + 0.18]);
            }
            r.wood(
                [-w, b.height - 0.29, sign * d - 0.14],
                [w, b.height - 0.11, sign * d + 0.14],
            );
            r.wood(
                [sign * w - 0.18, b.height - 0.29, -d],
                [sign * w + 0.18, b.height - 0.11, d],
            );
        }
        let plaster = if b.id % 3 == 0 {
            [0.69, 0.61, 0.45]
        } else {
            [0.76, 0.69, 0.54]
        };
        for sign in [-1., 1.] {
            let x = sign * (w - 0.14);
            let wz = b.window_z(sign);
            for (z0, z1, y0, y1) in [
                (-d, wz - 0.89, 1., b.height),
                (wz + 0.89, d, 1., b.height),
                (wz - 0.89, wz + 0.89, 2.58, b.height),
            ] {
                r.cuboid(
                    [x - 0.012, y0, z0],
                    [x + 0.012, y1, z1],
                    plaster,
                    18.,
                    false,
                );
            }
            let z = sign * (d - 0.14);
            for (x0, x1) in [(-w, -0.90), (0.90, w)] {
                r.cuboid(
                    [x0, 1., z - 0.012],
                    [x1, b.height, z + 0.012],
                    plaster,
                    18.,
                    false,
                );
            }
            r.cuboid(
                [-0.90, 2.60, z - 0.012],
                [0.90, b.height, z + 0.012],
                plaster,
                18.,
                false,
            );
        }
        let count = (d * 2. / 1.45).ceil() as usize;
        for i in 0..=count {
            let z = -d + 0.18 + (2. * d - 0.36) * i as f32 / count as f32;
            r.wood(
                [-w, b.height - 0.25, z - 0.075],
                [w, b.height - 0.12, z + 0.075],
            );
        }
        for sign in [-1., 1.] {
            let wz = b.window_z(sign);
            for z in [-d + 0.22, wz - 1.1, wz + 1.1, d - 0.22] {
                if z < -d + 0.1 || z > d - 0.1 {
                    continue;
                }
                r.wood(
                    [sign * (w - 0.11) - 0.09, 0.9, z - 0.07],
                    [sign * (w - 0.11) + 0.09, b.height - 0.1, z + 0.07],
                );
            }
        }
    }
    if let Some(z) = b.partition() {
        for (a, c) in [(-w, -0.94), (0.94, w)] {
            r.cuboid(
                [a, 0., z - 0.10],
                [c, b.height, z + 0.10],
                [0.73, 0.65, 0.49],
                18.,
                true,
            );
            r.cuboid([a, 0.03, z - 0.145], [c, 0.91, z + 0.145], DARK, 25., true);
            r.wood([a, 0.89, z - 0.17], [c, 1.0, z + 0.17]);
            // Framed woven wall hanging, leaving the central arch completely open.
            let mid = (a + c) * 0.5;
            let tw = ((c - a) * 0.46).min(1.2);
            r.cuboid(
                [mid - tw, 1.40, z - 0.145],
                [mid + tw, 2.72_f32.min(b.height - 0.3), z - 0.115],
                cloth,
                27.,
                false,
            );
            r.wood(
                [mid - tw - 0.06, 2.69_f32.min(b.height - 0.33), z - 0.20],
                [mid + tw + 0.06, 2.77_f32.min(b.height - 0.25), z - 0.1],
            );
            for x in [a + 0.05, c - 0.14] {
                r.wood([x, 0., z - 0.18], [x + 0.09, b.height, z + 0.18]);
            }
        }
        r.wood([-0.94, 2.55, z - 0.17], [0.94, b.height, z + 0.17]);
    }
    if !open {
        // A masonry firebox with a dark recess, mantel, fender and raised hearth.
        let hz = b.hearth_z();
        let hx = -w + 1.;
        r.cuboid(
            [hx - 0.83, 0., hz - 0.68],
            [hx + 0.83, 0.16, hz + 0.58],
            [0.46, 0.43, 0.36],
            23.,
            true,
        );
        r.cuboid(
            [hx - 0.65, 0.16, hz + 0.24],
            [hx + 0.65, 1.40, hz + 0.43],
            [0.11, 0.09, 0.07],
            17.,
            true,
        );
        for dx in [-0.75, 0.52] {
            r.cuboid(
                [hx + dx, 0.16, hz - 0.26],
                [hx + dx + 0.23, 1.42, hz + 0.47],
                [0.47, 0.41, 0.32],
                17.,
                true,
            );
        }
        r.cuboid(
            [hx - 0.78, 1.28, hz - 0.28],
            [hx + 0.78, 1.53, hz + 0.49],
            [0.46, 0.40, 0.32],
            17.,
            true,
        );
        r.wood([hx - 0.87, 1.51, hz - 0.38], [hx + 0.87, 1.67, hz + 0.52]);
        r.cuboid(
            [hx - 0.6, 1.67, hz + 0.06],
            [hx + 0.6, b.height, hz + 0.49],
            [0.46, 0.41, 0.33],
            20.,
            true,
        );
        for dz in [-0.15, 0.07] {
            r.wood(
                [hx - 0.42, 0.19, hz + dz],
                [hx + 0.40, 0.29, hz + dz + 0.10],
            );
        }
        for dx in [-0.48, 0.45] {
            r.cuboid(
                [hx + dx, 0.17, hz - 0.37],
                [hx + dx + 0.04, 0.41, hz + 0.14],
                IRON,
                12.,
                false,
            );
        }
        r.candle(hx - 0.61, 1.67, hz - 0.18);
        r.turned(
            [hx + 0.52, 1.67, hz - 0.08],
            0.10,
            0.26,
            JUG,
            [0.39, 0.47, 0.36],
            26.,
            false,
        );
        // Hanging brass candle crown, entirely above a player's head.
        let ly = b.lamp_local()[1];
        let lz = b.lamp_local()[2];
        r.turned([0., ly - 0.16, lz], 0.43, 0.07, RING, IRON, 12., false);
        r.cuboid(
            [-0.014, ly + 0.13, lz - 0.014],
            [0.014, b.height, lz + 0.014],
            IRON,
            12.,
            false,
        );
        for i in 0..4 {
            let a = i as f32 * std::f32::consts::FRAC_PI_2;
            let x = a.cos() * 0.4;
            let z = lz + a.sin() * 0.4;
            r.candle(x, ly - 0.10, z);
        }
    }
    let tz = -d + 2.5;
    match b.usage {
        Use::Inn => {
            // The bar runs along the left wall. The central street-to-bedroom
            // aisle is kept wider than a person plus collision margin.
            r.wood([-w + 0.85, 0., -d + 1.35], [-w + 2.05, 1.04, -d + 4.65]);
            r.wood([-w + 0.75, 1.04, -d + 1.24], [-w + 2.18, 1.17, -d + 4.78]);
            r.cuboid(
                [-w + 2.06, 0.18, -d + 1.42],
                [-w + 2.12, 0.87, -d + 4.6],
                DARK,
                25.,
                false,
            );
            r.shelf(-w + 0.26, -d + 0.28, 2.0, false);
            for n in 0..4 {
                r.mug(-w + 1.82, 1.17, -d + 1.7 + n as f32 * 0.73);
            }
            r.turned(
                [-w + 1.32, 1.17, -d + 2.7],
                0.15,
                0.36,
                JUG,
                [0.39, 0.48, 0.32],
                26.,
                false,
            );
            r.table(w - 1.65, tz, true);
            r.barrel(-w + 0.54, -d + 3.6, 1.0);
            r.barrel(-w + 0.54, -d + 4.55, 0.83);
        }
        Use::Arcane | Use::Guild => {
            r.shelf(-w + 0.32, -d + 0.30, w - 1.15, true);
            r.table(w - 1.65, tz, true);
            r.book(w - 1.80, 0.93, tz - 0.1, false, b.id);
            for n in 0..4 {
                r.turned(
                    [w - 1.95 + n as f32 * 0.2, 0.93, tz + 0.5],
                    0.065,
                    0.3,
                    BOTTLE,
                    [0.30, 0.42, 0.52],
                    26.,
                    false,
                );
            }
            r.chest(-w + 1., -d + 2.);
        }
        Use::Temple | Use::Hall => {
            // A pair of side altars keeps the principal aisle and window clear.
            for side in [-1., 1.] {
                let x = side * (w - 1.2);
                r.wood([x - 0.65, 0., d - 1.3], [x + 0.65, 1.0, d - 0.5]);
                r.cuboid(
                    [x - 0.68, 1.0, d - 1.34],
                    [x + 0.68, 1.04, d - 0.46],
                    cloth,
                    27.,
                    false,
                );
                r.candle(x - 0.4, 1.04, d - 0.9);
                r.candle(x + 0.4, 1.04, d - 0.9);
                for z in [-d + 2., -d + 3.5] {
                    r.wood([x - 0.7, 0.44, z - 0.25], [x + 0.7, 0.57, z + 0.25]);
                    for dx in [-0.6, 0.5] {
                        r.wood([x + dx, 0., z - 0.18], [x + dx + 0.10, 0.44, z + 0.18]);
                    }
                }
            }
        }
        Use::Smithy => {
            r.wood([w - 2.0, 0., -d + 0.65], [w - 0.25, 0.83, -d + 2.1]);
            r.cuboid(
                [w - 2.05, 0.83, -d + 0.6],
                [w - 0.2, 0.95, -d + 2.15],
                DARK,
                25.,
                true,
            );
            r.turned([w - 1.2, 0., -d + 3.3], 0.39, 0.65, BARREL, WOOD, 4., true);
            r.cuboid(
                [w - 1.5, 0.65, -d + 3.1],
                [w - 0.9, 0.91, -d + 3.5],
                IRON,
                12.,
                true,
            );
            r.cuboid(
                [w - 1.8, 0.90, -d + 3.12],
                [w - 0.72, 1.08, -d + 3.48],
                IRON,
                12.,
                true,
            );
            for n in 0..4 {
                let x = w - 1.8 + n as f32 * 0.33;
                r.wood([x, 0.95, -d + 0.70], [x + 0.045, 1.0, -d + 1.15]);
                r.cuboid(
                    [x - 0.08, 1., -d + 0.68],
                    [x + 0.12, 1.06, -d + 0.79],
                    IRON,
                    12.,
                    false,
                );
            }
            r.barrel(-w + 0.6, -d + 1., 0.9);
        }
        Use::Market | Use::Stable => {
            for side in [-1., 1.] {
                let x = side * (w - 1.1);
                r.chest(x, -d + 1.);
                r.barrel(x, d - 1.0, 1.1);
            }
            if b.usage == Use::Market {
                r.table(w - 1.5, 0., true);
            } else {
                for side in [-1., 1.] {
                    let x = side * (w - 1.0);
                    r.cuboid(
                        [x - 0.6, 0.04, -1.8],
                        [x + 0.6, 0.3, 1.8],
                        [0.64, 0.52, 0.26],
                        11.,
                        true,
                    );
                }
            }
        }
        Use::Barracks | Use::Watchtower => {
            r.table(w - 1.5, tz, false);
            r.chest(-w + 1., -d + 1.1);
            r.shelf(-w + 0.28, d - 0.75, 1.5, false);
        }
        _ => {
            r.table(w - 1.5, tz, false);
            r.shelf(-w + 0.30, -d + 0.28, 1.65, b.id % 3 == 0);
            r.barrel(-w + 0.65, -d + 1.65, 0.8);
        }
    }
    if b.partition().is_some() {
        for side in [-1., 1.] {
            let x = side * (w - 1.12);
            let z = d - 2.65;
            r.bed(
                x,
                z,
                matches!(b.usage, Use::Inn | Use::Home) && b.id % 3 != 0,
            );
            let nx = x - side * 1.12;
            r.wood([nx - 0.26, 0., d - 1.0], [nx + 0.26, 0.67, d - 0.46]);
            r.candle(nx, 0.67, d - 0.7);
            r.book(nx - 0.13, 0.67, d - 0.98, false, b.id);
        }
        if w > 4. {
            r.chest(w - 1.2, b.partition().unwrap() + 0.48);
        }
    }
    if !open {
        r.cuboid(
            [-0.73, 0.006, -d + 0.8],
            [0.73, 0.016, d - 0.5],
            cloth,
            24.,
            false,
        );
        // A few ceramics on rear window sill, below its opening.
        r.turned(
            [1.14, 1.0, d - 0.22],
            0.075,
            0.19,
            JUG,
            [0.38, 0.49, 0.36],
            26.,
            false,
        );
    }
}
pub fn render(mesh: &mut MeshData, b: &Building, lod: u32) {
    generate(b, |p| {
        if lod > 0 && !p.solid && (p.hi[1] - p.lo[1]).max(p.hi[0] - p.lo[0]) < 0.65 {
            return;
        }
        match p.shape {
            Shape::Box => block(mesh, b, p.lo, p.hi, p.color, p.texture),
            Shape::Turned {
                profile,
                at,
                radius,
                height,
            } => {
                let sides = if lod == 0 { 10 } else { 6 };
                for pair in profile.windows(2) {
                    for i in 0..sides {
                        let a = i as f32 * std::f32::consts::TAU / sides as f32;
                        let c = (i + 1) as f32 * std::f32::consts::TAU / sides as f32;
                        let point = |t: f32, q: (f32, f32)| {
                            b.point(
                                at[0] + t.cos() * q.1 * radius,
                                at[1] + q.0 * height,
                                at[2] + t.sin() * q.1 * radius,
                            )
                        };
                        let start = mesh.vertices.len();
                        mesh.quad(
                            point(a, pair[0]),
                            point(a, pair[1]),
                            point(c, pair[1]),
                            point(c, pair[0]),
                            p.color,
                            2.,
                        );
                        for v in &mut mesh.vertices[start..] {
                            let q = b.local(v.position[0], v.position[2]);
                            v.uv = [q[0] + q[1], v.position[1] - b.floor];
                            v.texture = 100. + p.texture;
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_room_uses_keep_doors_and_windows_clear_with_bounded_geometry() {
        for usage in [
            Use::Home,
            Use::Inn,
            Use::Smithy,
            Use::Market,
            Use::Guild,
            Use::Arcane,
            Use::Barracks,
            Use::Temple,
            Use::Hall,
            Use::Stable,
            Use::Tent,
            Use::Watchtower,
        ] {
            for id in 0..12 {
                let b = Building {
                    id,
                    name: "Room audit".into(),
                    usage,
                    x: 0.,
                    z: 0.,
                    floor: 0.,
                    yaw: 0.4,
                    half: if usage == Use::Tent {
                        [2.8, 3.3]
                    } else {
                        [3.4, 4.2]
                    },
                    height: 3.3,
                    door: true,
                    district: String::new(),
                    street_node: 0,
                    porch_node: 1,
                    room_node: 2,
                };
                let mut count = 0;
                generate(&b, |p| {
                    count += 1;
                    assert!(p.lo.into_iter().chain(p.hi).all(f32::is_finite));
                    assert!(
                        p.lo.iter().zip(p.hi).all(|(a, b)| *a <= b),
                        "inverted prop {usage:?}: {:?} .. {:?}",
                        p.lo,
                        p.hi
                    );
                    if p.lo[1] < 2.49 && p.hi[1] > 1.2 {
                        for side in [-1., 1.] {
                            let wx = side * b.half[0];
                            let wz = b.window_z(side);
                            assert!(
                                p.hi[0] < wx - 0.20
                                    || p.lo[0] > wx + 0.20
                                    || p.hi[2] < wz - 0.79
                                    || p.lo[2] > wz + 0.79,
                                "window obstructed {usage:?} side {side}: {:?} {:?}",
                                p.lo,
                                p.hi
                            );
                        }
                    }
                    if p.solid
                        && p.lo[1] < 1.8
                        && p.hi[1] > 0.
                        && p.lo[2] < b.half[1] - 0.5
                        && p.hi[2] > -b.half[1]
                    {
                        assert!(
                            p.hi[0] < -0.54 || p.lo[0] > 0.54,
                            "blocked central route {usage:?}: {:?} {:?}",
                            p.lo,
                            p.hi
                        );
                    }
                });
                assert!(count < 650, "unbounded room detail: {count}");
                let mut mesh = MeshData::default();
                render(&mut mesh, &b, 0);
                assert!(
                    mesh.vertices.len() < 60_000,
                    "room vertex budget exceeded {usage:?}: {}",
                    mesh.vertices.len()
                );
                assert!(mesh.vertices.iter().all(|v| v
                    .position
                    .into_iter()
                    .chain(v.normal)
                    .all(f32::is_finite)));
                let mut distant = MeshData::default();
                render(&mut distant, &b, 1);
                assert!(distant.vertices.len() <= mesh.vertices.len());
            }
        }
    }
}
