//! Small, entirely code-generated pixel materials shared by land, vegetation,
//! props and water. RGB is linear pigment modulation (multiply by 1.45 before
//! the mesh palette); surface RGBA stores normal XY, roughness and emission.
//! Every layer tiles except the five alpha-cutout plant illustrations.

pub const SIZE: u32 = 128;
pub const LAYERS: u32 = 28;
pub const LEVELS: u32 = 8;
pub const ALPHA_CUTOFF: f32 = 0.4;
/// Empty top corners removed from conifer cards; shared with the mask guard.
pub const NEEDLE_CARD_INSET: f32 = 0.24;
const DRAW: usize = 64;
const CUTOUTS: std::ops::RangeInclusive<usize> = 5..=9;

pub struct MaterialPixels {
    /// Each entry contains one mip, with all array layers concatenated.
    pub albedo: Vec<Vec<u8>>,
    pub surface: Vec<Vec<u8>>,
}

#[derive(Clone, Copy)]
struct Pixel {
    pigment: [f32; 3],
    height: f32,
    roughness: f32,
    alpha: f32,
}

impl Pixel {
    fn grey(value: f32, height: f32, roughness: f32, alpha: f32) -> Self {
        Self {
            pigment: [value; 3],
            height,
            roughness,
            alpha,
        }
    }
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut n = (x as u32).wrapping_mul(0x9e37_79b9)
        ^ (y as u32).wrapping_mul(0x85eb_ca6b)
        ^ seed.wrapping_mul(0xc2b2_ae35);
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^= n >> 16;
    (n & 0xffff) as f32 / 65535.0
}

fn noise(x: f32, y: f32, periods: i32, seed: u32) -> f32 {
    let px = x * periods as f32;
    let py = y * periods as f32;
    let ix = px.floor() as i32;
    let iy = py.floor() as i32;
    let fx = px.fract();
    let fy = py.fract();
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let sample = |dx: i32, dy: i32| {
        hash(
            (ix + dx).rem_euclid(periods),
            (iy + dy).rem_euclid(periods),
            seed,
        )
    };
    let a = sample(0, 0) * (1.0 - sx) + sample(1, 0) * sx;
    let b = sample(0, 1) * (1.0 - sx) + sample(1, 1) * sx;
    a * (1.0 - sy) + b * sy
}

/// Toroidal cellular distance produces coherent stone edges, pebbles and soil
/// clods without a visible square texture boundary.
fn cells(x: f32, y: f32, periods: i32, seed: u32) -> (f32, f32, f32) {
    let px = x * periods as f32;
    let py = y * periods as f32;
    let ix = px.floor() as i32;
    let iy = py.floor() as i32;
    let mut first = 99.0_f32;
    let mut second = 99.0_f32;
    let mut tone = 0.0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let hx = (ix + dx).rem_euclid(periods);
            let hy = (iy + dy).rem_euclid(periods);
            let cx = (ix + dx) as f32 + 0.15 + 0.7 * hash(hx, hy, seed);
            let cy = (iy + dy) as f32 + 0.15 + 0.7 * hash(hx, hy, seed + 19);
            let distance = (px - cx).hypot(py - cy);
            if distance < first {
                second = first;
                first = distance;
                tone = hash(hx, hy, seed + 57);
            } else if distance < second {
                second = distance;
            }
        }
    }
    (first, second - first, tone)
}

fn opaque(layer: usize, x: usize, y: usize) -> Pixel {
    let u = x as f32 / DRAW as f32;
    let v = y as f32 / DRAW as f32;
    let broad = noise(u, v, 4, 31 + layer as u32);
    let medium = noise(u, v, 12, 13 + layer as u32);
    let grain = hash(x as i32, y as i32, layer as u32 + 77) - 0.5;
    let tau = std::f32::consts::TAU;
    let (mut pigment, height, roughness) = match layer {
        0 => {
            let (d, edge, tone) = cells(u, v, 12, 64);
            let seam = (edge * 12.0).min(1.0);
            let pebble = if tone > 0.84 && d < 0.27 { 0.15 } else { 0.0 };
            (
                0.51 + 0.2 * broad + 0.08 * seam + pebble + grain * 0.035,
                0.27 * broad + 0.16 * seam + pebble,
                0.91 - pebble,
            )
        }
        1 => {
            let litter = ((u * 7.0 + v * 11.0 + medium * 0.3) * tau).sin().abs();
            let blades = if litter > 0.95 && medium > 0.5 {
                0.08
            } else {
                0.0
            };
            (
                0.5 + broad * 0.18 + medium * 0.12 + blades + grain * 0.05,
                0.25 * broad + 0.15 * medium + blades,
                0.93,
            )
        }
        2 => {
            let (_, edge, facet) = cells(u + 0.025 * (v * tau).sin(), v, 5, 116);
            let seams = (edge * 13.0).min(1.0);
            let strata = ((v * 11.0 + (u * tau * 2.0).sin() * 0.23) * tau).sin();
            (
                0.48 + facet * 0.17 + seams * 0.13 + broad * 0.08 + strata * 0.018 + grain * 0.035,
                0.35 * facet + seams * 0.18 + broad * 0.1 + strata * 0.015,
                0.85,
            )
        }
        3 => {
            let warp = noise(u, v, 4, 188) * 0.085;
            let ribs = ((u + warp) * tau * 11.0 + (v * tau * 3.0).sin() * 0.65).sin();
            let crevice = ((ribs + 0.82) * 3.0).clamp(0.0, 1.0);
            let chips = noise(u, v, 16, 273);
            let plate = cells(u, v, 10, 183).1;
            let fissure = if plate < 0.055 && chips < 0.49 {
                0.14
            } else {
                0.0
            };
            (
                0.44 + crevice * 0.2 + broad * 0.08 + chips * 0.045 + grain * 0.025 - fissure,
                crevice * 0.2 + chips * 0.065 - fissure * 0.65,
                0.89,
            )
        }
        4 => {
            let grain_wave = (u * tau * 17.0 + (v * tau).sin() * 0.7 + broad * 2.0).sin();
            let board = ((u * 4.0).fract() * 32.0).min(1.0);
            let nail = ((u * 4.0 - 0.12).fract() - 0.5).abs() < 0.035
                && ((v * 2.0).fract() - 0.11).abs() < 0.025;
            (
                if nail {
                    0.27
                } else {
                    0.44 + board * 0.23 + grain_wave * 0.035 + broad * 0.07
                },
                board * 0.12 + grain_wave * 0.02,
                if nail { 0.43 } else { 0.78 },
            )
        }
        10 => {
            let ripple = (v * tau * 9.0 + (u * tau * 2.0).sin() * 0.5).sin();
            let shell = if hash((x / 2) as i32, (y / 2) as i32, 707) > 0.983 {
                0.12
            } else {
                0.0
            };
            (
                0.63 + broad * 0.08 + ripple * 0.025 + grain * 0.065 + shell,
                0.08 * broad + ripple * 0.035 + shell * 0.1,
                0.94,
            )
        }
        11 => {
            let weave = if (x + y) % 2 == 0 { 0.035 } else { -0.035 };
            let fold = (u * tau * 3.0 + (v * tau).sin() * 0.15).sin();
            (
                0.65 + fold * 0.055 + weave + broad * 0.04,
                0.06 * fold + weave * 0.3,
                0.93,
            )
        }
        12 => {
            let (_, edge, facet) = cells(u, v, 9, 929);
            let hammered = (edge * 9.0).min(1.0);
            let scratch = if x % 17 == 2 && medium > 0.5 {
                0.07
            } else {
                0.0
            };
            (
                0.57 + facet * 0.08 + hammered * 0.08 + scratch + grain * 0.02,
                facet * 0.04 + hammered * 0.025,
                0.23 + medium * 0.23,
            )
        }
        13 => {
            let crust = cells(u, v, 10, 171).2;
            let crystals = if grain > 0.475 { 0.11 } else { 0.0 };
            (
                0.68 + broad * 0.075 + crust * 0.05 + crystals,
                broad * 0.13 + medium * 0.035,
                0.72 + broad * 0.12,
            )
        }
        14 => {
            let wave = (u * tau * 4.0 + (v * tau * 2.0).sin() * 0.7).sin()
                + 0.5 * (v * tau * 7.0 + (u * tau).sin() * 0.8).sin();
            (0.67 + wave * 0.045, wave * 0.065, 0.11)
        }
        15 => {
            let (_, edge, facet) = cells(u, v, 17, 211);
            let plush = (edge * 8.0).min(1.0);
            (
                0.51 + broad * 0.12 + facet * 0.11 + plush * 0.07 + grain * 0.03,
                broad * 0.24 + plush * 0.12,
                0.96,
            )
        }
        16 => {
            // Tileable hot filaments; the shader scrolls this tiny 2D field
            // upward at a fixed rate, independent of weather/time-of-day speed.
            let warp = (v * tau * 2.0).sin() * 0.18 + broad * 0.24;
            let tongue = ((u * 6.0 + warp) * tau).sin() * 0.5 + 0.5;
            let heat = (tongue * 0.56 + medium * 0.26 + broad * 0.18).clamp(0.0, 1.0);
            (0.52 + heat * 0.39, heat, 0.92)
        }
        17 => {
            let row = (v * 7.).floor();
            let x = (u * 5. + (row % 2.) * 0.5).fract();
            let y = (v * 7.).fract();
            let mortar = x < 0.045 || y < 0.065;
            let stone = hash((u * 5. + (row % 2.) * 0.5).floor() as i32, row as i32, 713);
            (
                if mortar {
                    0.40
                } else {
                    0.62 + stone * 0.14 + grain * 0.035
                },
                if mortar { 0.12 } else { 0.42 + stone * 0.04 },
                0.91,
            )
        }
        18 => (
            0.73 + broad * 0.06 + medium * 0.025 + grain * 0.035,
            0.1 + medium * 0.025,
            0.94,
        ),
        19 => {
            let row = (v * 10.).floor();
            let x = (u * 8. + (row % 2.) * 0.5).fract();
            let y = (v * 10.).fract();
            let seam = x < 0.045 || y < 0.09;
            let tile = hash((u * 8. + (row % 2.) * 0.5).floor() as i32, row as i32, 491);
            (
                if seam {
                    0.36
                } else {
                    0.57 + tile * 0.12 + y * 0.05
                },
                if seam { 0.08 } else { 0.26 + y * 0.08 },
                0.84,
            )
        }
        20 => {
            let row = (v * 12.0).floor();
            let xx = u * 6.0 + (row % 2.0) * 0.5;
            let x = xx.fract();
            let y = (v * 12.0).fract();
            let joint = x < 0.045 || y < 0.095;
            let brick = hash(xx.floor() as i32, row as i32, 894);
            let chips = hash((u * 64.0) as i32, (v * 64.0) as i32, 822);
            (
                if joint {
                    0.38
                } else {
                    0.57 + brick * 0.19 + grain * 0.05 - chips.powi(12) * 0.10
                },
                if joint { 0.06 } else { 0.46 + grain * 0.08 },
                0.88,
            )
        }
        21 => {
            let board = (v * 12.0).floor();
            let edge = (v * 12.0).fract();
            let grainline = ((u * 15.0 + broad * 1.4 + board * 0.13) * tau).sin();
            let knot = ((u * 4.0 + board * 0.31).fract() - 0.5).hypot((edge - 0.5) * 0.30);
            let nail = (u * 4.0).fract() < 0.055 && (edge - 0.18).abs() < 0.07;
            (
                if edge < 0.075 {
                    0.29
                } else if nail {
                    0.23
                } else {
                    0.61 + hash(board as i32, 9, 88) * 0.13 + grainline * 0.035
                        - knot.mul_add(36.0, 0.0).sin() * 0.045 / (1.0 + knot * 18.0)
                },
                if edge < 0.075 {
                    0.09
                } else {
                    0.35 + grainline * 0.03
                },
                0.85,
            )
        }
        22 => {
            let ripple =
                ((u * 5.0 + broad * 0.15) * tau).sin() * ((v * 4.0 + medium * 0.1) * tau).cos();
            (
                0.65 + broad * 0.14 + grain * 0.025,
                0.42 + ripple * 0.035,
                0.20 + broad * 0.08,
            )
        }
        23 => {
            // Broad irregular flagstones; worn bevels and fine joints.
            let row = (v * 4.).floor();
            let xx = u * 3. + (row % 2.) * 0.5;
            let fx = xx.fract();
            let fy = (v * 4.).fract();
            let edge = fx.min(1. - fx).min(fy.min(1. - fy));
            let tile = hash(xx.floor() as i32, row as i32, 329);
            let joint = edge < 0.025;
            (
                if joint {
                    0.28
                } else {
                    0.64 + tile * 0.17 + medium * 0.035 + grain * 0.025 - edge.min(0.08) * 0.25
                },
                if joint {
                    0.05
                } else {
                    0.3 + edge.min(0.10) * 1.6
                },
                0.90,
            )
        }
        24 => {
            // Woven medallion runner: borders, nested diamonds and tiny stitches.
            let x = (u - 0.5).abs();
            let y = (v - 0.5).abs();
            let diamond = x * 1.1 + y * 0.85;
            let border = x > 0.42 || y > 0.45;
            let stitch = ((u * 32.).floor() + (v * 32.).floor()) % 2.;
            let ornament = (diamond * 15.).fract() < 0.18;
            (
                if border {
                    0.80 - stitch * 0.18
                } else if ornament {
                    0.92
                } else {
                    0.45 + grain * 0.04
                },
                0.16 + grain * 0.04,
                0.97,
            )
        }
        25 => {
            // Recessed oak panels with raised rails and carved inner mouldings.
            let x = (u * 3.).fract();
            let y = (v * 2.).fract();
            let e = x.min(1. - x).min(y.min(1. - y));
            let rail = e < 0.045;
            let recess = (e - 0.07).abs() < 0.025;
            let grainline = ((v * 38. + broad * 0.65) * tau).sin();
            (
                if rail {
                    0.72
                } else if recess {
                    0.32
                } else {
                    0.56 + grainline * 0.035 + grain * 0.025
                },
                if rail {
                    0.60
                } else if recess {
                    0.13
                } else {
                    0.30
                },
                0.79,
            )
        }
        26 => {
            // Glazed slipware, faint throwing rings and a darker foot.
            let rings = (v * 24. * tau).sin();
            (
                0.78 + broad * 0.08 + rings * 0.025 + grain * 0.012,
                0.28 + rings * 0.009,
                0.28 + medium * 0.09,
            )
        }
        27 => {
            // Original heraldic textile: lozenge, branching tree and woven frame.
            let x = (u - 0.5).abs();
            let y = (v - 0.5).abs();
            let diamond = (x + y - 0.30).abs() < 0.025;
            let trunk = x < 0.027 && v > 0.24 && v < 0.76;
            let branch = (x - (0.72 - v) * 0.6).abs() < 0.023 && v > 0.27 && v < 0.69;
            let border = x > 0.43 || y > 0.46;
            (
                if diamond || trunk || branch {
                    0.94
                } else if border {
                    0.77
                } else {
                    0.37 + grain * 0.05
                },
                0.18 + grain * 0.025,
                0.96,
            )
        }
        _ => unreachable!(),
    };
    pigment = pigment.clamp(0.2, 0.94);
    Pixel::grey(pigment, height, roughness, 1.0)
}

struct Plant {
    pixels: Vec<Pixel>,
}

impl Plant {
    fn new() -> Self {
        Self {
            pixels: vec![Pixel::grey(0.65, 0.0, 0.72, 0.0); DRAW * DRAW],
        }
    }

    fn dab(&mut self, x: i32, y: i32, pigment: f32, height: f32, roughness: f32) {
        if (0..DRAW as i32).contains(&x) && (0..DRAW as i32).contains(&y) {
            self.pixels[y as usize * DRAW + x as usize] =
                Pixel::grey(pigment, height, roughness, 1.0);
        }
    }

    fn line(&mut self, a: [f32; 2], b: [f32; 2], width: f32, pigment: f32) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let count = ((dx.hypot(dy) * 2.0).ceil() as usize).max(1);
        let r = width.ceil() as i32;
        for step in 0..=count {
            let t = step as f32 / count as f32;
            let cx = a[0] + dx * t;
            let cy = a[1] + dy * t;
            for oy in -r..=r {
                for ox in -r..=r {
                    let x = cx.floor() as i32 + ox;
                    let y = cy.floor() as i32 + oy;
                    if (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) < width {
                        self.dab(x, y, pigment, 0.35, 0.75);
                    }
                }
            }
        }
    }

    /// A pointed, veined blade; a slightly serrated edge reads as leaves even
    /// at the game's small internal resolutions. Angle points root to tip.
    fn leaf(&mut self, center: [f32; 2], length: f32, width: f32, angle: f32, shade: f32) {
        let (sin, cos) = angle.sin_cos();
        let radius = length.max(width).ceil() as i32 + 1;
        let cx = center[0].floor() as i32;
        let cy = center[1].floor() as i32;
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let dx = x as f32 + 0.5 - center[0];
                let dy = y as f32 + 0.5 - center[1];
                let along = (dx * cos + dy * sin) / length;
                let across = (-dx * sin + dy * cos) / width;
                let serration = (along * 27.0).sin().abs() * 0.035;
                let profile = (1.0 - along * along).max(0.0).powf(0.7) - serration;
                if along.abs() < 1.0 && across.abs() < profile {
                    let vein = if across.abs() < 0.12 { 0.07 } else { 0.0 };
                    let side = if across > 0.0 { 0.045 } else { -0.025 };
                    let edge = (profile - across.abs()).min(0.35) * 0.11;
                    let pigment = (shade + vein + side + edge - along * 0.025).clamp(0.3, 0.92);
                    self.dab(
                        x,
                        y,
                        pigment,
                        0.3 + profile * 0.09 - across.abs() * 0.09,
                        0.64,
                    );
                }
            }
        }
    }

    fn broadleaf(&mut self) {
        // An asymmetric branch network: dense leaf masses separated by a few
        // readable windows, not identical rosettes with holes in their centers.
        let branches = [
            ([31., 59.], [15., 36.], [10., 23.]),
            ([31., 59.], [33., 31.], [29., 9.]),
            ([31., 48.], [47., 34.], [52., 19.]),
            ([30., 51.], [18., 48.], [7., 41.]),
            ([33., 45.], [44., 48.], [55., 44.]),
        ];
        for (root, elbow, tip) in branches {
            self.line(root, elbow, 0.85, 0.40);
            self.line(elbow, tip, 0.6, 0.44);
        }
        let clusters = [
            [12., 25.],
            [27., 13.],
            [35., 30.],
            [49., 23.],
            [15., 44.],
            [47., 44.],
        ];
        // Rear leaves establish dark coherent volumes; the upper fringe gets
        // warm facets. Pigment stays neutral enough for regional species tint.
        for pass in 0..2 {
            for (cluster, center) in clusters.into_iter().enumerate() {
                let k = cluster as i32;
                for leaf in 0..7 {
                    let angle = hash(k, leaf, 913) * std::f32::consts::TAU;
                    let radius = hash(k, leaf, 817).sqrt() * 8.;
                    let pos = [
                        center[0] + angle.cos() * radius,
                        center[1] + angle.sin() * radius - pass as f32 * 1.7,
                    ];
                    self.leaf(
                        pos,
                        4.1 + hash(k, leaf, 819) * 1.6,
                        2.0 + hash(k, leaf, 829) * 1.1,
                        angle * 0.4 - 1.1,
                        0.42 + pass as f32 * 0.15 + hash(k, leaf, 821) * 0.10,
                    );
                }
            }
        }
    }

    fn conifer(&mut self) {
        // A single irregular bough. Its two sides have different branch lengths
        // and overlapping needle fans, leaving gaps between the branch tiers.
        self.line([32., 62.], [29., 36.], 0.85, 0.40);
        self.line([29., 36.], [33., 5.], 0.65, 0.46);
        for row in 0..6 {
            let y = [53., 44., 35., 27., 19., 11.][row as usize];
            for side in [-1.0_f32, 1.] {
                let k = row * 2 + if side > 0. { 1 } else { 0 };
                let span = (24. - row as f32 * 2.7) * (0.78 + hash(k, 0, 427) * 0.22);
                let root = [30.5, y + hash(k, 1, 428) * 2.];
                let tip = [root[0] + side * span, y - 4.5 - hash(k, 2, 429) * 3.];
                self.line(root, tip, 0.6, 0.45);
                for twig in 0..4 {
                    let t = (twig as f32 + 0.75) / 4.;
                    let center = [
                        root[0] + (tip[0] - root[0]) * t,
                        root[1] + (tip[1] - root[1]) * t,
                    ];
                    for fan in 0..3 {
                        let angle =
                            if side > 0. { -0.85 } else { -2.29 } + (fan as f32 - 1.) * 0.48;
                        let length = 3.7 + hash(k, twig, 439) * 1.5 - row as f32 * 0.12;
                        self.leaf(
                            [
                                center[0] + side * fan as f32 * 0.65,
                                center[1] - fan as f32 * 0.35,
                            ],
                            length,
                            1.35,
                            angle,
                            0.47 + t * 0.13 + hash(k, twig, 442) * 0.08,
                        );
                    }
                }
            }
        }
        self.leaf([33., 7.], 4.8, 1.7, -1.5, 0.71);
        // The drawable shape shares the exact trapezoid used by every mesh
        // pass; a one-pixel margin avoids clipping leaf edges at base mip.
        for y in 0..DRAW {
            let inset = NEEDLE_CARD_INSET * (1. - (y as f32 + 0.5) / DRAW as f32);
            for x in 0..DRAW {
                let u = (x as f32 + 0.5) / DRAW as f32;
                if u < inset + 0.012 || u > 1. - inset - 0.012 {
                    self.pixels[y * DRAW + x].alpha = 0.;
                }
            }
        }
    }

    fn grass(&mut self) {
        for blade in 0..14 {
            let base = 25.0 + hash(blade, 0, 330) * 15.0;
            let tip_x = 4.0 + hash(blade, 1, 330) * 55.0;
            let tip_y = 3.0 + hash(blade, 2, 330) * 30.0;
            let tint = 0.51 + hash(blade, 3, 330) * 0.19;
            for y in tip_y.floor() as i32..64 {
                let t = (63.5 - y as f32) / (63.5 - tip_y);
                let bend = t * t;
                let center = base + (tip_x - base) * bend;
                let half = (0.65 + (t * std::f32::consts::PI).sin() * 1.1) * (1.0 - t).sqrt();
                for x in center.floor() as i32 - 2..=center.ceil() as i32 + 2 {
                    if (x as f32 + 0.5 - center).abs() <= half {
                        let face = if x as f32 + 0.5 > center {
                            0.055
                        } else {
                            -0.01
                        };
                        self.dab(x, y, tint + t * 0.1 + face, 0.35 + t * 0.08, 0.78);
                    }
                }
            }
        }
    }

    fn fern(&mut self) {
        self.line([32.0, 63.0], [30.0, 4.0], 0.75, 0.58);
        for row in 0..11 {
            let t = (row as f32 + 0.5) / 11.0;
            let y = 58.0 - t * 51.0 + (hash(row, 0, 591) - 0.5) * 1.6;
            let span = 4.0 + (t * std::f32::consts::PI).sin() * 19.0;
            for side in [-1.0_f32, 1.0] {
                let root = [32.0 - t * 2.0, y];
                let end = [root[0] + side * span, y - 8.0];
                self.line(root, end, 0.4, 0.59);
                for leaf in 0..5 {
                    let f = (leaf as f32 + 0.7) / 5.0;
                    let center = [
                        root[0] + (end[0] - root[0]) * f,
                        root[1] + (end[1] - root[1]) * f - 1.2,
                    ];
                    self.leaf(
                        center,
                        3.7 - f * 0.8,
                        1.65,
                        if side > 0.0 { -0.9 } else { -2.24 },
                        0.58 + f * 0.11 + t * 0.025,
                    );
                }
            }
        }
        self.leaf([30.0, 5.5], 4.0, 1.6, -1.57, 0.73);
    }

    fn flowers(&mut self) {
        // Neutral petals are tinted per species by the mesh. Stems are separate
        // grass cards, preventing blue or ivory flowers from having blue stems.
        for (flower, center) in [[31.0, 30.0], [14.0, 45.0], [47.0, 46.0], [44.0, 12.0]]
            .into_iter()
            .enumerate()
        {
            let radius = if flower == 0 { 7.8 } else { 5.0 };
            for petal in 0..6 {
                let angle = petal as f32 * std::f32::consts::TAU / 6.0 + flower as f32 * 0.31;
                let c = [
                    center[0] + angle.cos() * radius * 0.64,
                    center[1] + angle.sin() * radius * 0.64,
                ];
                self.leaf(
                    c,
                    radius * 0.72,
                    radius * 0.42,
                    angle,
                    0.72 + petal as f32 * 0.012,
                );
            }
            self.leaf(center, radius * 0.3, radius * 0.3, 0.0, 0.49);
        }
    }
}

fn byte(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

fn coverage(pixels: &[u8], scale: f32) -> f32 {
    let count = pixels
        .chunks_exact(4)
        .filter(|p| p[3] as f32 * scale >= ALPHA_CUTOFF * 255.0)
        .count();
    count as f32 / (pixels.len() / 4) as f32
}

fn preserve_coverage(pixels: &mut [u8], desired: f32) {
    if pixels.len() == 4 && desired > 0.0 {
        // One texel cannot represent fractional binary coverage. Keep the last
        // mip visible so distant crowns do not disappear before their LOD swap.
        pixels[3] = 153;
        return;
    }
    let mut lo = 0.0_f32;
    let mut hi = 16.0_f32;
    // Coverage is a staircase. Track the best endpoint rather than assuming a
    // binary search can match arbitrary coverage exactly at very small mips.
    let mut best_scale = 1.0;
    let mut best_error = (coverage(pixels, 1.0) - desired).abs();
    for _ in 0..20 {
        let scale = (lo + hi) * 0.5;
        let value = coverage(pixels, scale);
        let error = (value - desired).abs();
        if error < best_error {
            best_error = error;
            best_scale = scale;
        }
        if value < desired {
            lo = scale;
        } else {
            hi = scale;
        }
    }
    for p in pixels.chunks_exact_mut(4) {
        p[3] = (p[3] as f32 * best_scale).round().min(255.0) as u8;
    }
}

impl MaterialPixels {
    pub fn generate() -> Self {
        let layer_bytes = (SIZE * SIZE * 4) as usize;
        let mut base = vec![0_u8; layer_bytes * LAYERS as usize];
        let mut surface = base.clone();
        for layer in 0..LAYERS as usize {
            let cutout = CUTOUTS.contains(&layer);
            let logical = if cutout {
                let mut plant = Plant::new();
                match layer {
                    5 => plant.broadleaf(),
                    6 => plant.conifer(),
                    7 => plant.grass(),
                    8 => plant.fern(),
                    9 => plant.flowers(),
                    _ => unreachable!(),
                }
                plant.pixels
            } else {
                (0..DRAW * DRAW)
                    .map(|i| opaque(layer, i % DRAW, i / DRAW))
                    .collect::<Vec<_>>()
            };
            let at = |x: i32, y: i32, center: Pixel| {
                let nx = if cutout {
                    x.clamp(0, DRAW as i32 - 1)
                } else {
                    x.rem_euclid(DRAW as i32)
                };
                let ny = if cutout {
                    y.clamp(0, DRAW as i32 - 1)
                } else {
                    y.rem_euclid(DRAW as i32)
                };
                let neighbor = logical[ny as usize * DRAW + nx as usize];
                if neighbor.alpha == 0.0 {
                    center.height
                } else {
                    neighbor.height
                }
            };
            for y in 0..SIZE as usize {
                for x in 0..SIZE as usize {
                    let lx = (x / 2) as i32;
                    let ly = (y / 2) as i32;
                    let p = logical[ly as usize * DRAW + lx as usize];
                    let dx = (at(lx - 1, ly, p) - at(lx + 1, ly, p)) * 1.7;
                    let dy = (at(lx, ly - 1, p) - at(lx, ly + 1, p)) * 1.7;
                    let length = (dx * dx + dy * dy + 1.0).sqrt();
                    let offset = layer * layer_bytes + (y * SIZE as usize + x) * 4;
                    base[offset..offset + 4].copy_from_slice(&[
                        byte(p.pigment[0]),
                        byte(p.pigment[1]),
                        byte(p.pigment[2]),
                        byte(p.alpha),
                    ]);
                    surface[offset..offset + 4].copy_from_slice(&[
                        byte(dx / length * 0.5 + 0.5),
                        byte(dy / length * 0.5 + 0.5),
                        byte(p.roughness),
                        if layer == 16 {
                            byte(0.15 + p.height * 0.85)
                        } else {
                            0
                        },
                    ]);
                }
            }
        }
        let targets: Vec<_> = (0..LAYERS as usize)
            .map(|layer| coverage(&base[layer * layer_bytes..(layer + 1) * layer_bytes], 1.0))
            .collect();
        let mut result = Self {
            albedo: vec![base],
            surface: vec![surface],
        };
        for level in 1..LEVELS {
            let previous_size = (SIZE >> (level - 1)) as usize;
            let size = previous_size / 2;
            let mut color_mip = vec![0_u8; size * size * 4 * LAYERS as usize];
            let mut surface_mip = color_mip.clone();
            let colors = &result.albedo[(level - 1) as usize];
            let surfaces = &result.surface[(level - 1) as usize];
            for layer in 0..LAYERS as usize {
                for y in 0..size {
                    for x in 0..size {
                        let mut color_sum = [0.0_f32; 3];
                        let mut surface_sum = [0.0_f32; 4];
                        let mut alpha_sum = 0.0;
                        for oy in 0..2 {
                            for ox in 0..2 {
                                let i = (layer * previous_size * previous_size
                                    + (y * 2 + oy) * previous_size
                                    + x * 2
                                    + ox)
                                    * 4;
                                let alpha = colors[i + 3] as f32 / 255.0;
                                alpha_sum += alpha;
                                for channel in 0..3 {
                                    color_sum[channel] += colors[i + channel] as f32 * alpha;
                                }
                                for channel in 0..4 {
                                    surface_sum[channel] += surfaces[i + channel] as f32 * alpha;
                                }
                            }
                        }
                        let o = (layer * size * size + y * size + x) * 4;
                        if alpha_sum > 0.0001 {
                            for channel in 0..3 {
                                color_mip[o + channel] =
                                    (color_sum[channel] / alpha_sum).round() as u8;
                            }
                            for channel in 0..4 {
                                surface_mip[o + channel] =
                                    (surface_sum[channel] / alpha_sum).round() as u8;
                            }
                        } else {
                            color_mip[o..o + 3].fill(166);
                            surface_mip[o..o + 4].copy_from_slice(&[128, 128, 190, 0]);
                        }
                        color_mip[o + 3] = byte(alpha_sum * 0.25);
                    }
                }
                if CUTOUTS.contains(&layer) {
                    let range = layer * size * size * 4..(layer + 1) * size * size * 4;
                    preserve_coverage(&mut color_mip[range], targets[layer]);
                }
            }
            result.albedo.push(color_mip);
            result.surface.push(surface_mip);
        }
        result
    }
}

/// GPU array textures are less than 4.67 MiB including every mip. They are built
/// once at renderer initialization and sampled by all streaming chunks.
pub struct MaterialLibrary {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    _albedo: wgpu::Texture,
    _surface: wgpu::Texture,
    _people: wgpu::Texture,
}

impl MaterialLibrary {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let pixels = MaterialPixels::generate();
        let make_texture = |label, mipmaps: &[Vec<u8>]| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: LAYERS,
                },
                mip_level_count: LEVELS,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, mip) in mipmaps.iter().enumerate() {
                let dimension = SIZE >> level;
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: level as u32,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    mip,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(dimension * 4),
                        rows_per_image: Some(dimension),
                    },
                    wgpu::Extent3d {
                        width: dimension,
                        height: dimension,
                        depth_or_array_layers: LAYERS,
                    },
                );
            }
            texture
        };
        let albedo = make_texture("Procedural pixel pigments", &pixels.albedo);
        let surface = make_texture("Procedural pixel normals and roughness", &pixels.surface);
        let view_descriptor = wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        };
        let people = crate::people_sprites::texture(device, queue);
        let people_view = people.create_view(&view_descriptor);
        let albedo_view = albedo.create_view(&view_descriptor);
        let surface_view = surface.create_view(&view_descriptor);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Crisp pixels with stable distant mip filtering"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let foliage_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Clamped foliage cutouts"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Pixel material library"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                texture_entry(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Pixel material texture arrays"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&people_view),
                },
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&albedo_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&surface_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&foliage_sampler),
                },
            ],
        });
        Self {
            layout,
            bind_group,
            _albedo: albedo,
            _surface: surface,
            _people: people,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tapered_needle_cards_keep_the_authored_mask() {
        let atlas = super::MaterialPixels::generate();
        let size = super::SIZE as usize;
        let layer = &atlas.albedo[0][6 * size * size * 4..7 * size * size * 4];
        let mut covered = 0;
        for y in 0..size {
            let v = (y as f32 + 0.5) / size as f32;
            let inset = super::NEEDLE_CARD_INSET * (1. - v);
            for x in 0..size {
                if layer[(y * size + x) * 4 + 3] as f32 / 255. < super::ALPHA_CUTOFF {
                    continue;
                }
                let u = (x as f32 + 0.5) / size as f32;
                assert!(u >= inset && u <= 1. - inset, "needle clipped at {x},{y}");
                covered += 1;
            }
        }
        assert!(covered > size * size / 6);
    }
    use super::*;

    #[test]
    fn material_pixels_are_deterministic_and_complete() {
        let first = MaterialPixels::generate();
        let second = MaterialPixels::generate();
        assert_eq!(first.albedo, second.albedo);
        assert_eq!(first.surface, second.surface);
        assert_eq!(first.albedo.len(), LEVELS as usize);
        for level in 0..LEVELS as usize {
            let dimension = SIZE as usize >> level;
            assert_eq!(
                first.albedo[level].len(),
                dimension * dimension * LAYERS as usize * 4
            );
            assert_eq!(first.surface[level].len(), first.albedo[level].len());
        }
        // Twenty-eight 128px albedo/surface layers, including architectural variants.
        assert!(first.albedo.iter().map(Vec::len).sum::<usize>() * 2 < 4_900_000);
    }

    #[test]
    fn cutout_mips_preserve_plant_density() {
        let pixels = MaterialPixels::generate();
        let base_bytes = (SIZE * SIZE * 4) as usize;
        for layer in CUTOUTS {
            let desired = coverage(
                &pixels.albedo[0][layer * base_bytes..(layer + 1) * base_bytes],
                1.0,
            );
            assert!(
                desired > 0.12 && desired < 0.85,
                "layer {layer} coverage {desired}"
            );
            for level in 1..=4 {
                let size = SIZE as usize >> level;
                let bytes = size * size * 4;
                let actual = coverage(
                    &pixels.albedo[level][layer * bytes..(layer + 1) * bytes],
                    1.0,
                );
                assert!(
                    (desired - actual).abs() < 0.085,
                    "layer {layer} mip {level}: base {desired}, mip {actual}"
                );
            }
        }
    }

    #[test]
    fn opaque_materials_stay_opaque_and_normals_are_valid() {
        let pixels = MaterialPixels::generate();
        for level in 0..LEVELS as usize {
            let size = SIZE as usize >> level;
            let bytes = size * size * 4;
            for layer in 0..LAYERS as usize {
                for p in pixels.surface[level][layer * bytes..(layer + 1) * bytes].chunks_exact(4) {
                    let nx = p[0] as f32 / 255.0 * 2.0 - 1.0;
                    let ny = p[1] as f32 / 255.0 * 2.0 - 1.0;
                    assert!(nx * nx + ny * ny <= 1.0);
                    assert!(p[2] >= 20 && p[2] <= 250);
                    if layer == 16 {
                        assert!(p[3] > 0);
                    } else {
                        assert_eq!(p[3], 0);
                    }
                }
                if !CUTOUTS.contains(&layer) {
                    assert!(pixels.albedo[level][layer * bytes..(layer + 1) * bytes]
                        .chunks_exact(4)
                        .all(|p| p[3] == 255));
                }
            }
        }
    }
}
