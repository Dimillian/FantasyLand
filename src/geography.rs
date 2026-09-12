//! Continental anatomy, evaluated analytically rather than stored as a giant DEM.
//! Curved mountain backbones, branching spurs, lowered saddles and sheltered
//! basins share the same deterministic descriptors. Coordinates are metres.
use crate::world::{hash, rand01};
use serde::Serialize;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug)]
pub struct Geography {
    pub uplift: f32,
    pub gradient: [f32; 2],
    pub mountain: f32,
    pub mountain_gradient: [f32; 2],
    pub foothill: f32,
    pub basin: f32,
    pub basin_gradient: [f32; 2],
    pub rain_shadow: f32,
    pub windward: f32,
    pub axis: [f32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeoLandmarkKind {
    MountainPass,
    ShelteredBasin,
    RidgeSummit,
    TarnBasin,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct GeoLandmark {
    pub id: u32,
    pub kind: GeoLandmarkKind,
    pub position: [f32; 2],
    /// Along-range strike. Pass traversal is perpendicular to this direction.
    pub axis: [f32; 2],
    pub radius: f32,
    pub range_id: u32,
}
#[derive(Clone, Copy)]
struct D {
    v: f32,
    g: [f32; 2],
}
impl D {
    fn constant(v: f32) -> Self {
        Self { v, g: [0.; 2] }
    }
    fn scale(self, k: f32) -> Self {
        Self {
            v: self.v * k,
            g: [self.g[0] * k, self.g[1] * k],
        }
    }
    fn add(self, b: Self) -> Self {
        Self {
            v: self.v + b.v,
            g: [self.g[0] + b.g[0], self.g[1] + b.g[1]],
        }
    }
    fn offset(self, k: f32) -> Self {
        Self {
            v: self.v + k,
            ..self
        }
    }
    fn mul(self, b: Self) -> Self {
        Self {
            v: self.v * b.v,
            g: [
                self.g[0] * b.v + self.v * b.g[0],
                self.g[1] * b.v + self.v * b.g[1],
            ],
        }
    }
    fn sin(self) -> Self {
        let (s, c) = self.v.sin_cos();
        Self {
            v: s,
            g: [self.g[0] * c, self.g[1] * c],
        }
    }
    fn bell(self) -> Self {
        let q = 1. - self.v * self.v;
        if q <= 0. {
            return Self::constant(0.);
        }
        let dv = -6. * self.v * q * q;
        Self {
            v: q * q * q,
            g: [self.g[0] * dv, self.g[1] * dv],
        }
    }
    fn step(self, a: f32, b: f32) -> Self {
        let t = ((self.v - a) / (b - a)).clamp(0., 1.);
        let d = 6. * t * (1. - t) / (b - a);
        Self {
            v: t * t * (3. - 2. * t),
            g: [self.g[0] * d, self.g[1] * d],
        }
    }
    fn complement(self) -> Self {
        self.scale(-1.).offset(1.)
    }
}
#[derive(Clone, Copy)]
struct Range {
    center: [f32; 2],
    axis: [f32; 2],
    length: f32,
    width: f32,
    height: f32,
    phase: f32,
    bend: f32,
    period: f32,
    passes: [f32; 2],
}
fn ranges(seed: u32) -> [Range; 8] {
    let layouts = [
        (-48000., -21000., 0.88, 89000., 15500., 1510.),
        (46000., 33000., -0.32, 82000., 17000., 1670.),
        (-54000., 65000., -0.64, 55000., 13300., 1430.),
        (-158000., 42000., 1.35, 19000., 6500., 980.),
        (-18000., -155000., 0.18, 22000., 7200., 1040.),
        (154000., -86000., 1.18, 21000., 6800., 1130.),
        (149000., 131000., -0.5, 20500., 6900., 990.),
        (-38000., 159000., 0.15, 23500., 7300., 1190.),
    ];
    std::array::from_fn(|i| {
        let (x, z, a, l, w, h) = layouts[i];
        let i = i as i32;
        let random = |salt| rand01(hash(seed ^ 0x7812, i, salt));
        let angle = a + (random(0) - 0.5) * 0.25;
        let (s, c) = angle.sin_cos();
        Range {
            center: [x + (random(1) - 0.5) * 7500., z + (random(2) - 0.5) * 7500.],
            axis: [c, s],
            length: l * (0.93 + random(3) * 0.14),
            width: w,
            height: h * (0.92 + random(4) * 0.16),
            phase: random(5) * 6.2831855,
            bend: w * 0.25,
            period: l * 0.31,
            passes: [
                (-0.32 + (random(6) - 0.5) * 0.13) * l,
                (0.32 + (random(7) - 0.5) * 0.13) * l,
            ],
        }
    })
}
thread_local! {static RANGES:RefCell<Option<(u32,[Range;8])>>=const {RefCell::new(None)};}
fn with_ranges<T>(seed: u32, f: impl FnOnce(&[Range; 8]) -> T) -> T {
    RANGES.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().is_none_or(|(s, _)| *s != seed) {
            *c = Some((seed, ranges(seed)));
        }
        f(&c.as_ref().unwrap().1)
    })
}
fn spine(r: Range, u: D) -> D {
    u.scale(1. / r.period)
        .offset(r.phase)
        .sin()
        .scale(r.bend)
        .add(
            u.scale(2.7 / r.period)
                .offset(r.phase * 0.37)
                .sin()
                .scale(r.bend * 0.23),
        )
}
fn world_point(r: Range, u: f32, v: f32) -> [f32; 2] {
    let v = v + spine(r, D::constant(u)).v;
    [
        r.center[0] + r.axis[0] * u - r.axis[1] * v,
        r.center[1] + r.axis[1] * u + r.axis[0] * v,
    ]
}
/// Metadata is an inexpensive descriptor catalog, not guaranteed safe arrival
/// coordinates. Filter through coast/water/walking/collision before using it.
pub fn landmarks(seed: u32) -> Vec<GeoLandmark> {
    with_ranges(seed, |rs| {
        let mut out = Vec::with_capacity(56);
        for (i, &r) in rs.iter().enumerate() {
            for (j, &p) in r.passes.iter().enumerate() {
                out.push(GeoLandmark {
                    id: hash(seed ^ 0x7821, i as i32, j as i32),
                    kind: GeoLandmarkKind::MountainPass,
                    position: world_point(r, p, 0.),
                    axis: r.axis,
                    radius: r.width * 0.31,
                    range_id: i as u32,
                });
                for side in [-1., 1.] {
                    out.push(GeoLandmark {
                        id: hash(
                            seed ^ 0x7822,
                            i as i32,
                            j as i32 * 2 + if side > 0. { 1 } else { 0 },
                        ),
                        kind: GeoLandmarkKind::ShelteredBasin,
                        position: world_point(r, p + side * r.width * 0.20, side * r.width * 0.78),
                        axis: r.axis,
                        radius: r.width * 0.38,
                        range_id: i as u32,
                    });
                }
            }
            for (j, side) in [-1., 1.].into_iter().enumerate() {
                out.push(GeoLandmark {
                    id: hash(seed ^ 0x7824, i as i32, j as i32),
                    kind: GeoLandmarkKind::TarnBasin,
                    position: world_point(r, side * r.length * 0.12, side * r.width * 0.16),
                    axis: r.axis,
                    radius: (r.width * 0.11).max(1250.),
                    range_id: i as u32,
                });
            }
            out.push(GeoLandmark {
                id: hash(seed ^ 0x7823, i as i32, 0),
                kind: GeoLandmarkKind::RidgeSummit,
                position: world_point(r, 0., 0.),
                axis: r.axis,
                radius: r.width * 0.2,
                range_id: i as u32,
            });
        }
        out
    })
}
pub fn sample(seed: u32, x: f32, z: f32) -> Geography {
    with_ranges(seed, |rs| {
        let mut uplift = D::constant(0.);
        let mut mountain = D::constant(0.);
        let mut basin = D::constant(0.);
        let mut foothill = 0f32;
        let mut rain_shadow = 0f32;
        let mut windward = 0f32;
        let mut axis = [0.82, 0.5723635];
        let mut strongest = 0.;
        for &r in rs {
            let dx = x - r.center[0];
            let dz = z - r.center[1];
            let u = D {
                v: dx * r.axis[0] + dz * r.axis[1],
                g: r.axis,
            };
            if u.v.abs() > r.length {
                continue;
            }
            let v = D {
                v: -dx * r.axis[1] + dz * r.axis[0],
                g: [-r.axis[1], r.axis[0]],
            };
            if v.v.abs() > r.width * 2. + r.bend * 1.23 {
                continue;
            }
            let cross = v.add(spine(r, u).scale(-1.));
            let envelope = u.scale(1. / r.length).bell();
            let foot = cross.scale(1. / r.width).bell().mul(envelope);
            let crest = cross.scale(1. / (r.width * 0.29)).bell().mul(envelope);
            let mut pass = D::constant(1.);
            for &p in &r.passes {
                pass = pass.mul(
                    u.offset(-p)
                        .scale(1. / (r.width * 0.28))
                        .bell()
                        .scale(-0.73)
                        .offset(1.),
                );
                for side in [-1., 1.] {
                    let cu = u
                        .offset(-p - side * r.width * 0.20)
                        .scale(1. / (r.width * 0.48));
                    let cv = cross
                        .offset(-side * r.width * 0.78)
                        .scale(1. / (r.width * 0.46));
                    let hollow = cu.bell().mul(cv.bell()).mul(envelope);
                    // Union of compact basin weights; overlapping ranges cannot overflatten.
                    basin = basin.add(hollow.mul(basin.complement()));
                }
            }
            let mut local = crest
                .mul(pass)
                .scale(r.height)
                .add(foot.scale(r.height * 0.22));
            // Six offshoots taper away from the main divide. Their curved axes
            // are expressed relative to the parent spine, so they stay attached.
            for (j, along) in [-0.60, 0.0, 0.60].into_iter().enumerate() {
                for side in [-1., 1.] {
                    let lateral = cross.scale(side / r.width);
                    let reach = lateral.offset(-0.43).scale(1. / 0.55).bell();
                    let offset = u
                        .offset(-along * r.length)
                        .add(cross.scale(-side * (0.35 + j as f32 * 0.08)));
                    let rib = offset
                        .scale(1. / (r.width * 0.19))
                        .bell()
                        .mul(reach)
                        .mul(envelope);
                    local = local.add(rib.scale(r.height * 0.22));
                }
            }
            // Glacial-style cirques interrupt an upper shoulder with a small
            // flattened shelf and a closed bowl. A compact smooth support keeps
            // the outer mountainside continuous; Priority-Flood chooses the real
            // spill lip, and only compatible basins become retained flat lakes.
            for side in [-1., 1.] {
                let cu = side * r.length * 0.12;
                let cv = side * r.width * 0.16;
                let radius = (r.width * 0.11).max(1250.);
                let du = u.offset(-cu).scale(1. / radius);
                let dv = cross.offset(-cv).scale(1. / radius);
                if du.v.abs() > 1. || dv.v.abs() > 1. {
                    continue;
                }
                let radial = du.mul(du).add(dv.mul(dv));
                let shelf = radial.step(0.28, 1.).complement();
                let hollow = radial.step(0., 0.42).complement().scale(64.);
                let env = D::constant(cu / r.length).bell().v;
                let mut pass = 1.;
                for &p in &r.passes {
                    pass *= 1. - D::constant((cu - p) / (r.width * 0.28)).bell().v * 0.73;
                }
                let target = r.height
                    * env
                    * (D::constant(cv / (r.width * 0.29)).bell().v * pass
                        + D::constant(cv / r.width).bell().v * 0.22);
                local = local
                    .mul(shelf.complement())
                    .add(D::constant(target).add(hollow.scale(-1.)).mul(shelf));
            }
            uplift = uplift.add(local);
            mountain = mountain.add(foot.mul(mountain.complement()));
            foothill = foothill.max(foot.v);
            if local.v > strongest {
                strongest = local.v;
                axis = r.axis;
            }
            // Moist winds head east-northeast. The entire lee side retains a dry
            // footprint for tens of kilometres, independently of tiny face slopes.
            let wind_cross = -r.axis[1] * 0.91 - r.axis[0] * 0.41;
            let lee_sign = if wind_cross >= 0. { 1. } else { -1. };
            let lee = cross
                .scale(lee_sign)
                .offset(-r.width * 0.92)
                .scale(1. / (r.width * 1.05))
                .bell()
                .mul(envelope)
                .v;
            let wet = cross
                .scale(lee_sign)
                .offset(r.width * 0.38)
                .scale(1. / (r.width * 0.70))
                .bell()
                .mul(envelope)
                .v;
            rain_shadow = rain_shadow.max(lee * wind_cross.abs());
            windward = windward.max(wet * wind_cross.abs());
        }
        Geography {
            uplift: uplift.v,
            gradient: uplift.g,
            mountain: mountain.v,
            mountain_gradient: mountain.g,
            foothill,
            basin: basin.v,
            basin_gradient: basin.g,
            rain_shadow,
            windward,
            axis,
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analytic_range_gradient_matches_height_and_is_seeded() {
        for seed in [1337, 42, 2026] {
            for i in 0..1800 {
                let x = (rand01(hash(seed, i, 1)) - 0.5) * 370000.;
                let z = (rand01(hash(seed, i, 2)) - 0.5) * 370000.;
                let f = sample(seed, x, z);
                assert_eq!(f.uplift.to_bits(), sample(seed, x, z).uplift.to_bits());
                let gx = (sample(seed, x + 2., z).uplift - sample(seed, x - 2., z).uplift) * 0.25;
                let gz = (sample(seed, x, z + 2.).uplift - sample(seed, x, z - 2.).uplift) * 0.25;
                assert!((f.gradient[0] - gx).abs() < 0.004 && (f.gradient[1] - gz).abs() < 0.004);
                assert!(f.mountain >= 0. && f.mountain <= 1. && f.basin >= 0. && f.basin <= 1.);
            }
        }
    }
    #[test]
    fn passes_lower_real_divides_and_basins_match_descriptors() {
        for seed in [1337, 42, 2026] {
            with_ranges(seed, |rs| {
                for &r in rs {
                    // Avoid recursively borrowing descriptor cache during sampling.
                    for &p in &r.passes {
                        let crest = D::constant(p / r.length).bell().v * r.height;
                        assert!(crest > 300.);
                        let at = world_point(r, p, 0.);
                        assert!(at.iter().all(|v| v.is_finite()));
                    }
                }
            });
            let points = landmarks(seed);
            let passes: Vec<_> = points
                .iter()
                .filter(|p| p.kind == GeoLandmarkKind::MountainPass)
                .collect();
            let basins: Vec<_> = points
                .iter()
                .filter(|p| p.kind == GeoLandmarkKind::ShelteredBasin)
                .collect();
            assert_eq!(passes.len(), 16);
            assert_eq!(basins.len(), 32);
            assert!(
                basins
                    .iter()
                    .filter(|p| sample(seed, p.position[0], p.position[1]).basin > 0.1)
                    .count()
                    > 20
            );
            let mut lower = 0;
            for p in passes {
                let h = sample(seed, p.position[0], p.position[1]).uplift;
                let d = p.radius * 1.35;
                let r = with_ranges(seed, |rs| rs[p.range_id as usize]);
                let u = (p.position[0] - r.center[0]) * r.axis[0]
                    + (p.position[1] - r.center[1]) * r.axis[1];
                let pa = world_point(r, u + d, 0.);
                let pb = world_point(r, u - d, 0.);
                let a = sample(seed, pa[0], pa[1]).uplift;
                let b = sample(seed, pb[0], pb[1]).uplift;
                if a > h + 100. && b > h + 100. {
                    lower += 1;
                }
            }
            assert!(lower >= 12, "seed{seed} only{lower} lowered passes");
        }
    }
}
