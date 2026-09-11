//! Shared, deterministic geological provinces and terrain-driven landscape fields.
//! A blended regional substrate controls landform shape, rainfall response, rock
//! palette and soil capacity. No World or ecology sampling occurs here.
use crate::world::{hash, rand01, Biome, Sample};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LandscapeKind {
    AncientWoodland,
    GraniteHighlands,
    WindsweptCoast,
    WetLowlands,
    SandstoneCountry,
    Meadowlands,
    Alpine,
}
impl LandscapeKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::AncientWoodland => "Ancient woodland",
            Self::GraniteHighlands => "Granite highlands",
            Self::WindsweptCoast => "Windswept coast",
            Self::WetLowlands => "Wet lowlands",
            Self::SandstoneCountry => "Sandstone country",
            Self::Meadowlands => "Meadowlands",
            Self::Alpine => "Alpine heights",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Geology {
    Granite,
    Sandstone,
    Chalk,
    Basalt,
    Alluvium,
    Loam,
}
impl Geology {
    pub fn name(self) -> &'static str {
        match self {
            Self::Granite => "Granite",
            Self::Sandstone => "Sandstone",
            Self::Chalk => "Chalk",
            Self::Basalt => "Basalt",
            Self::Alluvium => "Alluvium",
            Self::Loam => "Loam",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Landscape {
    pub kind: LandscapeKind,
    pub geology: Geology,
    pub exposure: f32,
    pub soil: f32,
    pub wetness: f32,
    pub rockiness: f32,
    pub slope: f32,
    pub ground_color: [f32; 3],
    pub rock_color: [f32; 3],
    pub tree_scale: f32,
    pub ancient: f32,
    pub pale: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Base {
    pub height: f32,
    pub slope: f32,
    pub exposure: f32,
    pub rainfall: f32,
    pub geology: Geology,
    pub weights: [f32; 5],
    pub shelter: f32,
    pub fertility: f32,
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
// Value and analytic partial derivatives of quintic value noise. All fields are
// globally evaluated, so neither chunks nor province boundaries introduce seams.
fn field(seed: u32, x: f32, z: f32, scale: f32) -> (f32, [f32; 2]) {
    let x = x / scale;
    let z = z / scale;
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let u = x - ix as f32;
    let v = z - iz as f32;
    let fade = |t: f32| t * t * t * (t * (t * 6. - 15.) + 10.);
    let deriv = |t: f32| 30. * t * t * (t - 1.) * (t - 1.) / scale;
    let a = rand01(hash(seed, ix, iz));
    let b = rand01(hash(seed, ix + 1, iz));
    let c = rand01(hash(seed, ix, iz + 1));
    let d = rand01(hash(seed, ix + 1, iz + 1));
    let fu = fade(u);
    let fv = fade(v);
    (
        mix(mix(a, b, fu), mix(c, d, fu), fv),
        [
            mix(b - a, d - c, fv) * deriv(u),
            mix(c - a, d - b, fu) * deriv(v),
        ],
    )
}
fn provinces(seed: u32, x: f32, z: f32) -> ([f32; 5], [[f32; 2]; 5]) {
    const CELL: f32 = 24000.;
    const R2: f32 = 1.21;
    let x = x / CELL;
    let z = z / CELL;
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let mut w = [0.; 5];
    let mut g = [[0.; 2]; 5];
    let mut total = 0.;
    let mut total_g = [0.; 2];
    for j in iz - 1..=iz + 1 {
        for i in ix - 1..=ix + 1 {
            let h = hash(seed ^ 0x7201, i, j);
            let dx = x - (i as f32 + 0.5 + (rand01(h) - 0.5) * 0.46);
            let dz = z - (j as f32 + 0.5 + (rand01(hash(h, j, i)) - 0.5) * 0.46);
            let q = 1. - (dx * dx + dz * dz) / R2;
            if q <= 0. {
                continue;
            }
            let k = match hash(seed ^ 0x7202, i, j) % 12 {
                0..=3 => 0,
                4..=6 => 1,
                7..=8 => 2,
                9..=10 => 3,
                _ => 4,
            };
            let v = q.powi(4);
            let dx = -8. * dx * q.powi(3) / (R2 * CELL);
            let dz = -8. * dz * q.powi(3) / (R2 * CELL);
            w[k] += v;
            g[k][0] += dx;
            g[k][1] += dz;
            total += v;
            total_g[0] += dx;
            total_g[1] += dz;
        }
    }
    for k in 0..5 {
        g[k][0] = (g[k][0] * total - w[k] * total_g[0]) / (total * total);
        g[k][1] = (g[k][1] * total - w[k] * total_g[1]) / (total * total);
        w[k] /= total;
    }
    (w, g)
}
pub fn base(seed: u32, x: f32, z: f32) -> Base {
    let (weights, weight_gradient) = provinces(seed, x, z);
    let (broad, bg) = field(seed ^ 0x7310, x, z, 45000.);
    let (crest, cg) = field(seed ^ 0x7311, x + z * 0.19, z - x * 0.11, 4800.);
    let ridge = 1. - (crest * 2. - 1.).abs();
    let ridge_derivative = if crest < 0.5 { 2. } else { -2. };
    let ridge2 = ridge * ridge;
    let ridge_g = [
        2. * ridge * ridge_derivative * (cg[0] - cg[1] * 0.11),
        2. * ridge * ridge_derivative * (cg[1] + cg[0] * 0.19),
    ];
    let (hill, hg) = field(seed ^ 0x7312, x, z, 1600.);
    let (roll, rg) = field(seed ^ 0x7313, x, z, 430.);
    let (detail, dg) = field(seed ^ 0x7314, x, z, 70.);
    // Smooth ledges are landform-scale strata, not quantized vertex heights.
    let mesa = smooth(0.27, 0.48, crest) * 0.57 + smooth(0.67, 0.78, crest) * 0.43;
    let derivative = |a: f32, b: f32, v: f32| {
        let t = ((v - a) / (b - a)).clamp(0., 1.);
        6. * t * (1. - t) / (b - a)
    };
    let mesa_d = derivative(0.27, 0.48, crest) * 0.57 + derivative(0.67, 0.78, crest) * 0.43;
    let profiles = [
        105. + broad * 140. + hill * 110. + ridge2 * 45.,
        260. + broad * 150. + ridge2 * 1620. + hill * 105.,
        140. + broad * 130. + mesa * 560. + hill * 55.,
        120. + broad * 110. + crest * 220. + hill * 55.,
        250. + broad * 170. + mesa * 710. + hill * 110.,
    ];
    let gradients = [
        [
            bg[0] * 140. + hg[0] * 110. + ridge_g[0] * 45.,
            bg[1] * 140. + hg[1] * 110. + ridge_g[1] * 45.,
        ],
        [
            bg[0] * 150. + hg[0] * 105. + ridge_g[0] * 1620.,
            bg[1] * 150. + hg[1] * 105. + ridge_g[1] * 1620.,
        ],
        [
            bg[0] * 130. + hg[0] * 55. + mesa_d * 560. * (cg[0] - cg[1] * 0.11),
            bg[1] * 130. + hg[1] * 55. + mesa_d * 560. * (cg[1] + cg[0] * 0.19),
        ],
        [
            bg[0] * 110. + hg[0] * 55. + (cg[0] - cg[1] * 0.11) * 220.,
            bg[1] * 110. + hg[1] * 55. + (cg[1] + cg[0] * 0.19) * 220.,
        ],
        [
            bg[0] * 170. + hg[0] * 110. + mesa_d * 710. * (cg[0] - cg[1] * 0.11),
            bg[1] * 170. + hg[1] * 110. + mesa_d * 710. * (cg[1] + cg[0] * 0.19),
        ],
    ];
    let mut height = 0.;
    let mut gradient = [0.; 2];
    let mut dominant = 0;
    for k in 0..5 {
        height += profiles[k] * weights[k];
        for a in 0..2 {
            gradient[a] += gradients[k][a] * weights[k] + profiles[k] * weight_gradient[k][a];
        }
        if weights[k] > weights[dominant] {
            dominant = k;
        }
    }
    let geology = [
        Geology::Loam,
        Geology::Granite,
        Geology::Sandstone,
        Geology::Chalk,
        Geology::Basalt,
    ][dominant];
    let roughness = weights[0] * 18.
        + weights[1] * 40.
        + weights[2] * 14.
        + weights[3] * 12.
        + weights[4] * 28.;
    height += (roll - 0.5) * roughness + (detail - 0.5) * 3.;
    let slope = (gradient[0] + rg[0] * roughness + dg[0] * 3.)
        .hypot(gradient[1] + rg[1] * roughness + dg[1] * 3.);
    // Moist ocean winds travel east-northeast. Analytic upslope lift and lee
    // shelter respond to the same relief that shapes the visible ridgelines.
    let lift = (gradient[0] * 0.91 - gradient[1] * 0.41).clamp(-0.35, 0.35);
    let regional_rain = field(seed ^ 0x7315, x, z, 38000.).0;
    let rainfall =
        (0.45 + regional_rain * 0.40 + lift * 0.62 - weights[2] * 0.16).clamp(0.10, 0.98);
    let exposure =
        (0.12 + smooth(300., 1300., height) * 0.60 + ridge2 * weights[1] * 0.24 + lift.abs() * 0.5)
            .clamp(0., 1.);
    let shelter = (0.90 - exposure * 0.64 - smooth(0.06, 0.6, slope) * 0.18).clamp(0., 1.);
    let fertility = (weights[0] * 0.94
        + weights[1] * 0.32
        + weights[2] * 0.36
        + weights[3] * 0.55
        + weights[4] * 0.72)
        .clamp(0., 1.);
    Base {
        height,
        slope,
        exposure,
        rainfall,
        geology,
        weights,
        shelter,
        fertility,
    }
}
pub fn sample(seed: u32, x: f32, z: f32, terrain: &Sample) -> Landscape {
    let b = base(seed, x, z);
    let coast = crate::world::coast::info(seed, x, z);
    let coastal = 1. - smooth(200., 4500., coast.distance);
    let exposure = (b.exposure + coastal * 0.60).clamp(0., 1.);
    let bank = (terrain.river * 0.65 + smooth(0.65, 0.98, terrain.moisture) * 0.25).clamp(0., 1.);
    let wetness = (terrain.moisture * 0.64 + bank * 0.42 + b.shelter * 0.08).clamp(0., 1.);
    let soil = (b.fertility * (1. - smooth(0.12, 0.65, b.slope) * 0.83) * (1. - exposure * 0.26)
        + bank * 0.18)
        .clamp(0., 1.);
    let rockiness = (smooth(0.08, 0.52, b.slope) * 0.75
        + (1. - soil) * 0.28
        + smooth(1000., 1600., terrain.height) * 0.35)
        .clamp(0., 1.);
    let ancient =
        smooth(0.52, 0.78, field(seed ^ 0x7411, x, z, 8500.).0) * soil * b.shelter * (1. - coastal);
    let pale = smooth(0.74, 0.90, field(seed ^ 0x7412, x, z, 4700.).0)
        * smooth(0.3, 0.6, wetness)
        * (1. - exposure);
    let kind = if coast.distance < 1900. {
        LandscapeKind::WindsweptCoast
    } else if terrain.biome == Biome::Alpine || terrain.height > 1380. {
        LandscapeKind::Alpine
    } else if bank > 0.34 && terrain.height < 620. {
        LandscapeKind::WetLowlands
    } else if b.weights[2] > 0.42 {
        LandscapeKind::SandstoneCountry
    } else if b.weights[1] > 0.43 && terrain.height > 420. {
        LandscapeKind::GraniteHighlands
    } else if ancient > 0.10 || (soil > 0.60 && wetness > 0.49 && b.shelter > 0.63) {
        LandscapeKind::AncientWoodland
    } else {
        LandscapeKind::Meadowlands
    };
    let geology = if bank > 0.58 && b.slope < 0.16 {
        Geology::Alluvium
    } else {
        b.geology
    };
    let ground = [
        [0.34, 0.44, 0.22],
        [0.40, 0.45, 0.35],
        [0.55, 0.40, 0.23],
        [0.48, 0.50, 0.30],
        [0.30, 0.36, 0.26],
    ];
    let rocks = [
        [0.45, 0.43, 0.36],
        [0.48, 0.51, 0.50],
        [0.65, 0.39, 0.22],
        [0.76, 0.75, 0.62],
        [0.24, 0.27, 0.28],
    ];
    let mut ground_color = [0.; 3];
    let mut rock_color = [0.; 3];
    for k in 0..5 {
        for c in 0..3 {
            ground_color[c] += ground[k][c] * b.weights[k];
            rock_color[c] += rocks[k][c] * b.weights[k];
        }
    }
    if geology == Geology::Alluvium {
        for c in 0..3 {
            ground_color[c] = mix(ground_color[c], [0.32, 0.39, 0.25][c], bank * 0.55);
        }
    }
    Landscape {
        kind,
        geology,
        exposure,
        soil,
        wetness,
        rockiness,
        slope: b.slope,
        ground_color,
        rock_color,
        tree_scale: (0.80 + soil * 0.32 + ancient * 0.75 - exposure * 0.20).clamp(0.60, 1.80),
        ancient,
        pale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regional_profiles_are_deterministic_continuous_and_geologically_diverse() {
        let mut families = std::collections::HashSet::new();
        let mut low = f32::MAX;
        let mut high = 0f32;
        for seed in [1337, 42, 2026] {
            for z in (-120000..120000).step_by(8500) {
                for x in (-120000..120000).step_by(8500) {
                    let x = x as f32;
                    let z = z as f32;
                    let a = base(seed, x, z);
                    let b = base(seed, x, z);
                    assert_eq!(a.height.to_bits(), b.height.to_bits());
                    families.insert(a.geology.name());
                    low = low.min(a.height);
                    high = high.max(a.height);
                    assert!(a.height.is_finite() && a.slope.is_finite() && a.slope >= 0.);
                    assert!((0.0..=1.0).contains(&a.rainfall) && (0.0..=1.0).contains(&a.exposure));
                    assert!(
                        (a.height - base(seed, x + 0.02, z).height).abs() < 0.12,
                        "province seam at {x},{z}"
                    );
                }
            }
        }
        assert_eq!(families.len(), 5);
        assert!(low < 180. && high > 1600.);
    }
    #[test]
    fn analytic_slope_matches_actual_rotated_terrain() {
        for seed in [1337, 42] {
            for i in 0..300 {
                let x = (rand01(hash(seed, i, 3)) - 0.5) * 240000.;
                let z = (rand01(hash(seed, i, 4)) - 0.5) * 240000.;
                let b = base(seed, x, z);
                let dx = (base(seed, x + 2., z).height - base(seed, x - 2., z).height) / 4.;
                let dz = (base(seed, x, z + 2.).height - base(seed, x, z - 2.).height) / 4.;
                let actual = dx.hypot(dz);
                assert!(
                    (b.slope - actual).abs() < 0.025 + actual * 0.08,
                    "analytic {} actual {actual} at{x},{z}",
                    b.slope
                );
            }
        }
    }
    #[test]
    fn prevailing_wind_makes_windward_slopes_wetter_than_nearby_lee_slopes() {
        let mut windward = (0f32, 0usize);
        let mut lee = (0f32, 0usize);
        for z in (-90000..90000).step_by(1200) {
            for x in (-90000..90000).step_by(1200) {
                let x = x as f32;
                let z = z as f32;
                let b = base(1337, x, z);
                if b.weights[1] < 0.9 {
                    continue;
                }
                let up = base(1337, x + 45.5, z - 20.5).height;
                let down = base(1337, x - 45.5, z + 20.5).height;
                let lift = (up - down) / 100.;
                if lift > 0.15 {
                    windward.0 += b.rainfall;
                    windward.1 += 1;
                }
                if lift < -0.15 {
                    lee.0 += b.rainfall;
                    lee.1 += 1;
                }
            }
        }
        assert!(windward.1 > 50 && lee.1 > 50);
        assert!(windward.0 / windward.1 as f32 > lee.0 / lee.1 as f32 + 0.12);
    }
}
