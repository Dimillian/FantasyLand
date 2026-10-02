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
    ChalkDowns,
    BasaltUplands,
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
            Self::ChalkDowns => "Chalk downs",
            Self::BasaltUplands => "Basalt uplands",
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
/// Formation identity is descriptive; blended height fields remain continuous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Formation {
    None,
    BrokenRidge,
    LayeredLedge,
    Ravine,
    ChalkScarp,
    BasaltBench,
}
impl Formation {
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "Open ground",
            Self::BrokenRidge => "Broken granite ridge",
            Self::LayeredLedge => "Sandstone ledges",
            Self::Ravine => "Incised ravine",
            Self::ChalkScarp => "Chalk scarp",
            Self::BasaltBench => "Basalt benches",
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
    pub formation: Formation,
    pub formation_strength: f32,
    /// Unit world X/Z geological strike, shared by terrain and larger outcrops.
    pub formation_axis: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct Base {
    pub height: f32,
    /// Relief before walk-scale outcrops and soil hollows. Used to generalize
    /// atlas shading without undersampling fine terrain into false ridgelines.
    pub macro_height: f32,
    pub slope: f32,
    pub exposure: f32,
    pub rainfall: f32,
    pub geology: Geology,
    pub weights: [f32; 5],
    pub shelter: f32,
    pub fertility: f32,
    pub formation: Formation,
    pub formation_strength: f32,
    pub formation_axis: [f32; 2],
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
// A small first-order value keeps the formation profiles and their true slope
// together. Formation derivatives never need neighboring terrain queries.
#[derive(Clone, Copy)]
struct Differential {
    v: f32,
    g: [f32; 2],
}
impl Differential {
    fn scaled(self, k: f32) -> Self {
        Self {
            v: self.v * k,
            g: [self.g[0] * k, self.g[1] * k],
        }
    }
    fn plus(self, b: Self) -> Self {
        Self {
            v: self.v + b.v,
            g: [self.g[0] + b.g[0], self.g[1] + b.g[1]],
        }
    }
    fn shifted(self, k: f32) -> Self {
        Self {
            v: self.v + k,
            ..self
        }
    }
    fn times(self, b: Self) -> Self {
        Self {
            v: self.v * b.v,
            g: [
                self.g[0] * b.v + self.v * b.g[0],
                self.g[1] * b.v + self.v * b.g[1],
            ],
        }
    }
    fn abs(self) -> Self {
        self.scaled(if self.v < 0. { -1. } else { 1. })
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
        self.scaled(-1.).shifted(1.)
    }
}
struct LocalForms {
    profiles: [Differential; 5],
    strength: [f32; 5],
    ravine: f32,
    axis: [f32; 2],
}
fn local_forms(seed: u32, x: f32, z: f32) -> LocalForms {
    // Outcrops occupy irregular patches. An isotropic, domain-warped fold field
    // avoids the old globally stretched bands shared by every geological family.
    // Warp coordinates, not the angle of absolute world positions: distortion
    // must remain bounded equally far from the origin and across chunk edges.
    let (wx, wxg) = field(seed ^ 0x7361, x, z, 3100.);
    let (wz, wzg) = field(seed ^ 0x7362, x, z, 2700.);
    let (j, jg) = field(
        seed ^ 0x7351,
        x * 0.76 + z * 0.65,
        z * 0.76 - x * 0.65,
        1600.,
    );
    let jg = [jg[0] * 0.76 - jg[1] * 0.65, jg[0] * 0.65 + jg[1] * 0.76];
    let u = x + (wx - 0.5) * 650.;
    let v = z + (wz - 0.5) * 650.;
    let (f, fg) = field(seed ^ 0x7352, u, v, 520.);
    let fold = Differential {
        v: f,
        g: [
            fg[0] * (1. + wxg[0] * 650.) + fg[1] * wzg[0] * 650.,
            fg[0] * wxg[1] * 650. + fg[1] * (1. + wzg[1] * 650.),
        ],
    };
    let joint = Differential { v: j, g: jg };
    let outcrop = joint.step(0.42, 0.68);
    let broken = joint.step(0.64, 0.86).scaled(-0.64).shifted(1.);
    let rib = fold
        .scaled(2.)
        .shifted(-1.)
        .abs()
        .complement()
        .step(0.61, 0.96)
        .times(broken);
    let ravine = fold
        .shifted(0.0)
        .shifted(-0.53)
        .abs()
        .step(0.015, 0.09)
        .complement()
        .times(joint.step(0.18, 0.43));
    let ledge = fold
        .step(0.22, 0.33)
        .scaled(40.)
        .plus(fold.step(0.68, 0.765).scaled(27.));
    let basalt = fold
        .step(0.20, 0.29)
        .scaled(54.)
        .plus(fold.step(0.52, 0.625).scaled(36.));
    let chalk = fold
        .step(0.44, 0.57)
        .scaled(37.)
        .plus(joint.step(0.58, 0.79).scaled(-13.));
    let length = fold.g[0].hypot(fold.g[1]);
    let axis = if length > 0.0000001 {
        [fold.g[1] / length, -fold.g[0] / length]
    } else {
        [1., 0.]
    };
    LocalForms {
        profiles: [
            fold.shifted(-0.5).scaled(3.),
            rib.scaled(68.)
                .plus(ravine.scaled(-21.))
                .shifted(-15.)
                .times(outcrop),
            ledge.plus(ravine.scaled(-46.)).shifted(-26.).times(outcrop),
            chalk.shifted(-15.).times(outcrop),
            basalt
                .plus(ravine.scaled(-15.))
                .shifted(-37.)
                .times(outcrop),
        ],
        strength: [
            0.,
            rib.v * outcrop.v,
            fold.step(0.18, 0.32).v * (1. - fold.step(0.78, 0.89).v) * outcrop.v,
            fold.step(0.37, 0.49).v * (1. - fold.step(0.59, 0.75).v) * outcrop.v,
            fold.step(0.15, 0.30).v * (1. - fold.step(0.66, 0.83).v) * outcrop.v,
        ],
        ravine: ravine.v * outcrop.v,
        axis,
    }
}
pub fn base(seed: u32, x: f32, z: f32) -> Base {
    let (weights, weight_gradient) = provinces(seed, x, z);
    let (broad, bg) = field(seed ^ 0x7310, x, z, 45000.);
    let (crest, cg) = field(seed ^ 0x7311, x + z * 0.19, z - x * 0.11, 4800.);
    // Round the ridge apex over a narrow band so slope-dependent ecology has
    // no derivative jump at the former absolute-value cusp.
    let crest_delta = crest * 2. - 1.;
    let rounded = crest_delta.hypot(0.02);
    let ridge = (1.00019998 - rounded) / 0.98019998;
    let ridge_derivative = -2. * crest_delta / (rounded * 0.98019998);
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
    let mut profiles = [
        105. + broad * 140. + hill * 110. + ridge2 * 45.,
        260. + broad * 150. + ridge2 * 650. + hill * 105.,
        140. + broad * 130. + mesa * 560. + hill * 55.,
        120. + broad * 110. + crest * 220. + hill * 55.,
        250. + broad * 170. + mesa * 710. + hill * 110.,
    ];
    let mut gradients = [
        [
            bg[0] * 140. + hg[0] * 110. + ridge_g[0] * 45.,
            bg[1] * 140. + hg[1] * 110. + ridge_g[1] * 45.,
        ],
        [
            bg[0] * 150. + hg[0] * 105. + ridge_g[0] * 650.,
            bg[1] * 150. + hg[1] * 105. + ridge_g[1] * 650.,
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
    // Explicit continental ranges organize the macro relief. Geological noise
    // supplies subordinate ridges/plateaus; basins suppress that local uplift.
    // The product rule keeps slope identical to the height actually returned.
    let geography = crate::geography::sample(seed, x, z);
    let relief = 0.28 + geography.mountain * 0.48 - geography.basin * 0.12;
    let relief_gradient = [
        geography.mountain_gradient[0] * 0.48 - geography.basin_gradient[0] * 0.12,
        geography.mountain_gradient[1] * 0.48 - geography.basin_gradient[1] * 0.12,
    ];
    for k in 0..5 {
        let old = profiles[k] - 85.;
        profiles[k] = 85. + old * relief + geography.uplift;
        for a in 0..2 {
            gradients[k][a] =
                gradients[k][a] * relief + old * relief_gradient[a] + geography.gradient[a];
        }
    }
    let climate_profiles = profiles;
    let climate_gradients = gradients;
    let forms = local_forms(seed, x, z);
    for k in 0..5 {
        profiles[k] += forms.profiles[k].v;
        gradients[k][0] += forms.profiles[k].g[0];
        gradients[k][1] += forms.profiles[k].g[1];
    }
    let mut height = 0.;
    let mut gradient = [0.; 2];
    let mut climate_gradient = [0.; 2];
    let mut dominant = 0;
    for k in 0..5 {
        height += profiles[k] * weights[k];
        for a in 0..2 {
            gradient[a] += gradients[k][a] * weights[k] + profiles[k] * weight_gradient[k][a];
            climate_gradient[a] +=
                climate_gradients[k][a] * weights[k] + climate_profiles[k] * weight_gradient[k][a];
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
    for a in 0..2 {
        gradient[a] += rg[a] * roughness + dg[a] * 3.;
        for k in 0..5 {
            gradient[a] += (roll - 0.5) * [18., 40., 14., 12., 28.][k] * weight_gradient[k][a];
        }
    }
    // Walk-scale hollows and shallow drainage grooves. Amplitude follows
    // substrate continuously; these do not alter the macro climate derivative.
    let (pocket, pocket_gradient) = field(seed ^ 0x7358, x, z, 110.);
    let hollow = Differential {
        v: pocket,
        g: pocket_gradient,
    }
    .step(0.22, 0.52)
    .scaled(4.6)
    .shifted(-2.3);
    let groove = Differential {
        v: pocket,
        g: pocket_gradient,
    }
    .shifted(-0.51)
    .abs()
    .step(0.025, 0.18)
    .complement()
    .scaled(-2.4);
    let micro = hollow.plus(groove);
    let amplitudes = [1.0, 0.26, 0.52, 0.83, 0.40];
    let capacity: f32 = (0..5).map(|k| weights[k] * amplitudes[k]).sum();
    height += micro.v * capacity;
    for a in 0..2 {
        gradient[a] += micro.g[a] * capacity
            + micro.v
                * (0..5)
                    .map(|k| weight_gradient[k][a] * amplitudes[k])
                    .sum::<f32>();
    }
    let slope = gradient[0].hypot(gradient[1]);
    // Moist ocean winds travel east-northeast. Analytic upslope lift and lee
    // shelter respond to the same relief that shapes the visible ridgelines.
    // Rain and shelter respond to hills/ridges, not to a two-metre ledge face.
    let lift = (climate_gradient[0] * 0.91 - climate_gradient[1] * 0.41).clamp(-0.35, 0.35);
    let rainfall =
        crate::climate::rainfall(seed, x, z, geography.windward, geography.rain_shadow, lift);
    let exposure =
        (0.12 + smooth(300., 1300., height) * 0.60 + ridge2 * weights[1] * 0.24 + lift.abs() * 0.5)
            .clamp(0., 1.);
    let shelter = (0.90 - exposure * 0.64 + geography.basin * 0.10
        - smooth(0.06, 0.6, climate_gradient[0].hypot(climate_gradient[1])) * 0.18)
        .clamp(0., 1.);
    let fertility = (weights[0] * 0.94
        + weights[1] * 0.32
        + weights[2] * 0.36
        + weights[3] * 0.55
        + weights[4] * 0.72)
        .clamp(0., 1.);
    let mut formation = Formation::None;
    let mut formation_strength = 0.;
    for k in 1..5 {
        let value = forms.strength[k] * weights[k];
        if value > formation_strength {
            formation_strength = value;
            formation = [
                Formation::None,
                Formation::BrokenRidge,
                Formation::LayeredLedge,
                Formation::ChalkScarp,
                Formation::BasaltBench,
            ][k];
        }
    }
    let ravine = forms.ravine * (weights[2] + weights[1] * 0.35 + weights[4] * 0.35);
    if ravine > formation_strength * 0.85 {
        formation = Formation::Ravine;
        formation_strength = ravine;
    }
    Base {
        height,
        macro_height: (0..5).map(|k| climate_profiles[k] * weights[k]).sum(),
        slope,
        exposure,
        rainfall,
        geology,
        weights,
        shelter,
        fertility,
        formation,
        formation_strength,
        formation_axis: forms.axis,
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
    } else if b.weights[4] > 0.45 {
        LandscapeKind::BasaltUplands
    } else if b.weights[3] > 0.46 {
        LandscapeKind::ChalkDowns
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
        formation: b.formation,
        formation_strength: b.formation_strength,
        formation_axis: b.formation_axis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outcrops_have_quiet_ground_and_no_continental_stripe_direction() {
        for seed in [1337, 42, 2026] {
            let (mut quiet, mut visible) = (0, 0);
            let (mut xx, mut xz, mut zz) = (0f64, 0f64, 0f64);
            for i in 0..4000 {
                let x = (rand01(hash(seed ^ 91, i, 0)) - 0.5) * 300000.;
                let z = (rand01(hash(seed ^ 91, i, 1)) - 0.5) * 300000.;
                let f = local_forms(seed, x, z);
                if f.profiles[1..].iter().all(|p| p.v == 0.) {
                    quiet += 1;
                    assert!(f.profiles[1..].iter().all(|p| p.g == [0., 0.]));
                    assert!(f.strength.iter().all(|&v| v == 0.) && f.ravine == 0.);
                }
                for p in &f.profiles[1..] {
                    assert!(p.v.abs() < 95., "local outcrop became a mountain");
                    let [a, b] = p.g.map(|g| g as f64);
                    xx += a * a;
                    xz += a * b;
                    zz += b * b;
                }
                visible += usize::from(f.profiles[1..].iter().any(|p| p.g[0].hypot(p.g[1]) > 0.1));
                assert!((f.axis[0].hypot(f.axis[1]) - 1.).abs() < 0.001);
            }
            let dominant = 0.5 * (1. + ((xx - zz).powi(2) + 4. * xz * xz).sqrt() / (xx + zz));
            assert!(
                quiet > 800 && visible > 400,
                "quiet{quiet} exposed{visible}"
            );
            assert!(
                dominant < 0.76,
                "globally aligned outcrop gradients: {dominant}"
            );
        }
    }

    #[test]
    fn warped_outcrop_derivatives_match_their_actual_faces() {
        for i in 0..500 {
            let x = (rand01(hash(914, i, 0)) - 0.5) * 330000.;
            let z = (rand01(hash(914, i, 1)) - 0.5) * 330000.;
            let f = local_forms(1337, x, z);
            let a = local_forms(1337, x - 1., z);
            let b = local_forms(1337, x + 1., z);
            let c = local_forms(1337, x, z - 1.);
            let d = local_forms(1337, x, z + 1.);
            for k in 0..5 {
                let gradient = [
                    (b.profiles[k].v - a.profiles[k].v) * 0.5,
                    (d.profiles[k].v - c.profiles[k].v) * 0.5,
                ];
                for axis in 0..2 {
                    assert!(
                        (gradient[axis] - f.profiles[k].g[axis]).abs() < 0.018,
                        "outcrop derivative at{x},{z} profile{k}"
                    );
                }
            }
        }
    }

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
        // Rain follows mountain-scale uplift, not a small granite rib face.
        // Derive the slope independently from actual terrain across one km,
        // then compare opposite sides of the same range in three worlds.
        let lift = |seed, p: [f32; 2]| {
            (base(seed, p[0] + 455., p[1] - 205.).height
                - base(seed, p[0] - 455., p[1] + 205.).height)
                / 1000.
        };
        for seed in [1337, 42, 2026] {
            let (mut pairs, mut wetter, mut difference) = (0, 0, 0.);
            for ridge in crate::geography::landmarks(seed)
                .into_iter()
                .filter(|p| p.kind == crate::geography::GeoLandmarkKind::RidgeSummit)
            {
                let mut cross = [-ridge.axis[1], ridge.axis[0]];
                let alignment = cross[0] * 0.91 - cross[1] * 0.41;
                if alignment.abs() < 0.30 {
                    continue;
                }
                if alignment < 0. {
                    cross = cross.map(|v| -v);
                }
                for factor in [1., 1.5, 2., 2.5] {
                    let d = ridge.radius * factor;
                    let a = [
                        ridge.position[0] - cross[0] * d,
                        ridge.position[1] - cross[1] * d,
                    ];
                    let b = [
                        ridge.position[0] + cross[0] * d,
                        ridge.position[1] + cross[1] * d,
                    ];
                    if lift(seed, a) <= 0.05 || lift(seed, b) >= -0.05 {
                        continue;
                    }
                    let delta = base(seed, a[0], a[1]).rainfall - base(seed, b[0], b[1]).rainfall;
                    pairs += 1;
                    wetter += usize::from(delta > 0.);
                    difference += delta;
                }
            }
            assert!(
                pairs >= 12,
                "too few mountain pairs for seed{seed}: {pairs}"
            );
            assert!(
                wetter * 5 >= pairs * 4,
                "windward rain reversed for seed{seed}"
            );
            assert!(
                difference / pairs as f32 > 0.12,
                "weak rain shadow for seed{seed}"
            );
        }
    }
}
