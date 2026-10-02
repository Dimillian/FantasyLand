//! Seeded landmass geometry, independent of drainage and roads. Broad connected
//! lobes, open coastal bays and domain-warped detail define the mainland; five
//! separated island envelopes prevent accidental archipelagos or land bridges.
use super::{hash, lerp, noise, rand01, smooth};
use serde::Serialize;
pub const SEA_LEVEL: f32 = 0.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShoreKind {
    None,
    Beach,
    Cliff,
    Dunes,
    Shingle,
    SaltMarsh,
    Estuary,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Info {
    /// Signed distance approximation in metres: positive inland, negative sea.
    /// Interior values saturate at12km; coastal clearance remains continuous.
    pub distance: f32,
    pub landmass_id: Option<u32>,
}
fn ellipse(x: f32, z: f32, cx: f32, cz: f32, rx: f32, rz: f32, angle: f32) -> f32 {
    let dx = x - cx;
    let dz = z - cz;
    let (s, c) = angle.sin_cos();
    let a = (dx * c + dz * s) / rx;
    let b = (-dx * s + dz * c) / rz;
    (1.0 - (a * a + b * b).sqrt()) * rx.min(rz)
}
fn union(a: f32, b: f32, r: f32) -> f32 {
    let h = (0.5 + 0.5 * (a - b) / r).clamp(0.0, 1.0);
    lerp(b, a, h) + r * h * (1.0 - h)
}
fn detail(seed: u32, x: f32, z: f32) -> f32 {
    (noise(seed ^ 0x6301, x / 6800., z / 6800.) - 0.5) * 2800.
        + (noise(seed ^ 0x6302, x / 2100., z / 2100.) - 0.5) * 850.
        + (noise(seed ^ 0x6303, x / 650., z / 650.) - 0.5) * 150.
}
pub fn info(seed: u32, x: f32, z: f32) -> Info {
    // This protected interior is far from every bay. Saturating distance lets
    // the common walking/terrain path skip all offshore work without a seam.
    if (x + 12000.).powi(2) + (z + 8000.).powi(2) < 42000. * 42000. {
        return Info {
            distance: 12000.,
            landmass_id: Some(0),
        };
    }
    let wx = x
        + (noise(seed ^ 0x6201, x / 47000., z / 47000.) - 0.5) * 9600.
        + (noise(seed ^ 0x6202, x / 16000., z / 16000.) - 0.5) * 2200.;
    let wz = z
        + (noise(seed ^ 0x6203, x / 47000., z / 47000.) - 0.5) * 9600.
        + (noise(seed ^ 0x6204, x / 16000., z / 16000.) - 0.5) * 2200.;
    let mut mainland = ellipse(wx, wz, -12000., -8000., 88000., 94000., -0.18);
    for (cx, cz, rx, rz, angle) in [
        (-50000., -56000., 60000., 50000., -0.35),
        (63000., -19000., 69000., 43000., 0.22),
        (35000., 59000., 59000., 69000., -0.3),
        (-63000., 47000., 55000., 43000., 0.35),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (cx, cz, rx, rz, a))| {
        let r = |k| rand01(hash(seed ^ 0x6D41, i as i32, k));
        (
            cx + (r(0) - 0.5) * 18000.,
            cz + (r(1) - 0.5) * 18000.,
            rx * (0.82 + r(2) * 0.24),
            rz * (0.82 + r(3) * 0.24),
            a + (r(4) - 0.5) * 0.62,
        )
    }) {
        mainland = union(mainland, ellipse(wx, wz, cx, cz, rx, rz, angle), 4200.);
    }
    // Every bay opens onto the surrounding ocean, avoiding inland sea holes.
    for (cx, cz, rx, rz, angle) in [
        (-114000., -17000., 40000., 29000., -0.2),
        (124000., 36000., 47000., 24000., 0.22),
        (18000., -106000., 32000., 30000., 0.1),
        (82000., 119000., 35000., 42000., -0.32),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (cx, cz, rx, rz, a))| {
        let r = |k| rand01(hash(seed ^ 0x6D42, i as i32, k));
        (
            cx,
            cz,
            rx * (0.72 + r(0) * 0.55),
            rz * (0.72 + r(1) * 0.55),
            a + (r(2) - 0.5) * 0.45,
        )
    }) {
        mainland = mainland.min(-ellipse(wx, wz, cx, cz, rx, rz, angle));
    }
    mainland += detail(seed, wx, wz);
    let mut best = mainland;
    let mut id = 0;
    // Envelopes are deliberately separated by generous straits. Seed variation
    // changes coast shapes and orientation while keeping the component budget.
    for (index, (_cx, _cz, rx, rz, angle)) in [
        (-158000., 42000., 15500., 21000., -0.35),
        (-18000., -155000., 23000., 17500., 0.25),
        (154000., -86000., 18000., 22500., -0.3),
        (149000., 131000., 22000., 17000., -0.6),
        (-38000., 159000., 25500., 20000., 0.3),
    ]
    .into_iter()
    .enumerate()
    {
        let salt = index as i32;
        let [cx, cz] = island_center(seed, index);
        let scale = 0.88 + rand01(hash(seed ^ 0x6403, salt, 3)) * 0.18;
        let angle = angle + (rand01(hash(seed ^ 0x6404, salt, 4)) - 0.5) * 0.30;
        let (sin, cos) = angle.sin_cos();
        let dx = wx - cx;
        let dz = wz - cz;
        let x = dx * cos + dz * sin;
        let z = -dx * sin + dz * cos;
        let rx = rx * scale;
        let rz = rz * scale;
        let shape = |cx: f32, cz: f32, sx: f32, sz: f32, a: f32| {
            ellipse(x, z, cx * rx, cz * rz, sx * rx, sz * rz, a)
        };
        let (core, lobe_a, lobe_b, bay) =
            match (index + (hash(seed ^ 0x6D43, salt, 0) % 5) as usize) % 5 {
                // A crooked north-south crescent, with a narrow southern tail.
                0 => (
                    shape(0., 0., 0.72, 0.78, 0.),
                    shape(-0.18, -0.44, 0.65, 0.55, -0.20),
                    shape(0.28, 0.55, 0.43, 0.60, 0.40),
                    shape(0.65, 0.05, 0.62, 0.46, 0.15),
                ),
                // A broad, two-lobed island with a long eastern headland.
                1 => (
                    shape(0., 0., 0.85, 0.65, 0.),
                    shape(-0.50, -0.22, 0.72, 0.60, -0.15),
                    shape(0.72, 0.20, 0.62, 0.38, 0.12),
                    shape(-0.08, -0.72, 0.45, 0.66, -0.1),
                ),
                // An elongated ridge with a deep western inlet and offset ends.
                2 => (
                    shape(0., 0., 0.60, 0.82, 0.),
                    shape(-0.25, -0.47, 0.70, 0.59, -0.20),
                    shape(0.26, 0.48, 0.55, 0.58, 0.35),
                    shape(-0.68, 0.10, 0.45, 0.62, -0.10),
                ),
                // Two unequal arms form an asymmetric branching peninsula.
                3 => (
                    shape(0., 0., 0.72, 0.60, 0.),
                    shape(-0.62, 0.20, 0.63, 0.40, -0.15),
                    shape(0.55, -0.29, 0.65, 0.50, 0.20),
                    shape(0.05, 0.66, 0.48, 0.52, 0.10),
                ),
                // A large western mass curls around a deeply indented northeast bay.
                _ => (
                    shape(0., 0., 0.70, 0.75, 0.),
                    shape(-0.56, -0.02, 0.60, 0.72, -0.25),
                    shape(0.48, 0.37, 0.60, 0.45, 0.30),
                    shape(0.40, -0.70, 0.60, 0.60, -0.20),
                ),
            };
        let mut island = union(union(core, lobe_a, 1500.), lobe_b, 1200.).min(-bay);
        island += detail(seed ^ (index as u32 * 0x1731 + 0x6511), wx, wz)
            * 0.5
            * (1. - smooth(10000., 16000., island.abs()));
        if island > best {
            best = island;
            id = index as u32 + 1;
        }
    }
    Info {
        distance: best.min(12000.),
        landmass_id: (best >= 0.).then_some(id),
    }
}
/// Shared with mountain descriptors, so island ranges follow seed variation.
pub fn island_center(seed: u32, index: usize) -> [f32; 2] {
    let centers = [
        [-158000., 42000.],
        [-18000., -155000.],
        [154000., -86000.],
        [149000., 131000.],
        [-38000., 159000.],
    ];
    let c = centers[index % 5];
    [
        c[0] + (rand01(hash(seed ^ 0x6401, index as i32, 1)) - 0.5) * 10000.,
        c[1] + (rand01(hash(seed ^ 0x6402, index as i32, 2)) - 0.5) * 10000.,
    ]
}
pub fn landmass_id(seed: u32, x: f32, z: f32) -> Option<u32> {
    info(seed, x, z).landmass_id
}
pub fn cliff_strength(inland_height: f32) -> f32 {
    smooth(400., 800., inland_height)
}
#[allow(dead_code)] // Retained legacy profile helper for deterministic coast audits.
pub fn shore(distance: f32, inland_height: f32) -> ShoreKind {
    let cliff = cliff_strength(inland_height);
    if distance >= -70. && distance < lerp(145., 270., cliff) {
        if cliff > 0.48 {
            ShoreKind::Cliff
        } else {
            ShoreKind::Beach
        }
    } else {
        ShoreKind::None
    }
}
pub fn elevation(info: Info, inland_height: f32) -> f32 {
    let d = info.distance;
    let cliff = cliff_strength(inland_height);
    if d <= 0. {
        let offshore = -d;
        return -(3. * smooth(0., 120., offshore)
            + 42. * smooth(100., 2200., offshore)
            + 260. * smooth(2000., 12000., offshore)
            + 350. * smooth(12000., 60000., offshore)
            + cliff * 80. * smooth(0., 250., offshore));
    }
    if d >= 5500. {
        return inland_height;
    }
    let beach = 3. * smooth(0., 90., d)
        + 12. * smooth(90., 550., d)
        + (inland_height - 15.) * smooth(500., 5500., d);
    let cliff_height = inland_height * (0.18 * smooth(0., 85., d) + 0.82 * smooth(350., 4400., d));
    lerp(beach, cliff_height, cliff)
}
pub fn ocean_color(depth: f32) -> [f32; 3] {
    let shallow = [0.15, 0.39, 0.43];
    let shelf = [0.09, 0.25, 0.33];
    let deep = [0.055, 0.16, 0.23];
    let first = smooth(0., 48., depth);
    let second = smooth(45., 190., depth);
    std::array::from_fn(|i| lerp(lerp(shallow[i], shelf[i], first), deep[i], second))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn continent_has_five_separate_sizable_islands_and_ocean_edges() {
        const N: usize = 769;
        const CELL: f32 = 500.;
        let half = 192000.;
        for seed in [1337, 42, 2026] {
            let mut land = vec![u8::MAX; N * N];
            let mut ids = [0usize; 6];
            let mut min_edge_depth = f32::INFINITY;
            for z in 0..N {
                for x in 0..N {
                    let wx = x as f32 * CELL - half;
                    let wz = z as f32 * CELL - half;
                    let c = info(seed, wx, wz);
                    if let Some(id) = c.landmass_id {
                        land[z * N + x] = id as u8;
                        ids[id as usize] += 1;
                    }
                    if x == 0 || z == 0 || x == N - 1 || z == N - 1 {
                        assert!(c.landmass_id.is_none(), "land reaches world boundary");
                        min_edge_depth = min_edge_depth
                            .min(-elevation(c, super::super::inland_height(seed, wx, wz)));
                    }
                }
            }
            let mut seen = vec![false; N * N];
            let mut components = Vec::new();
            for i in 0..land.len() {
                if land[i] == u8::MAX || seen[i] {
                    continue;
                }
                let identity = land[i];
                let mut q = std::collections::VecDeque::from([i]);
                seen[i] = true;
                let mut size = 0;
                while let Some(k) = q.pop_front() {
                    assert_eq!(land[k], identity, "two assigned landmasses touch");
                    size += 1;
                    let x = k % N;
                    let z = k / N;
                    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let nx = x as i32 + dx;
                        let nz = z as i32 + dz;
                        if nx < 0 || nz < 0 || nx >= N as i32 || nz >= N as i32 {
                            continue;
                        }
                        let n = nz as usize * N + nx as usize;
                        if land[n] != u8::MAX && !seen[n] {
                            seen[n] = true;
                            q.push_back(n);
                        }
                    }
                }
                components.push((identity, size));
            }
            assert_eq!(
                components.len(),
                6,
                "seed {seed}: unexpected detached rocks or joined islands: {components:?}"
            );
            assert!(min_edge_depth > 190., "world border must be deep ocean");
            for island in 1..=5 {
                let ratio = ids[island] as f32 / ids[0] as f32;
                assert!(
                    (0.01..=0.05).contains(&ratio),
                    "island{island} ratio{ratio}"
                );
            }
            assert!((33000.0..40000.0).contains(&(ids[0] as f32 * 0.25)));
            println!(
                "seed{seed}: areas km²{:?}, ratios{:?}, minimum edge depth{min_edge_depth:.1}m",
                ids.map(|n| n as f32 * 0.25),
                ids.map(|n| n as f32 / ids[0] as f32)
            );
        }
    }
    #[test]
    fn interior_is_preserved_and_coastal_profiles_are_continuous() {
        for seed in [1337, 42, 2026] {
            for (x, z) in [
                (0., 0.),
                (-16500., -12400.),
                (21000., 18000.),
                (-30000., -24000.),
            ] {
                let c = info(seed, x, z);
                assert_eq!(c.landmass_id, Some(0));
                assert_eq!(
                    super::super::raw_height(seed, x, z).to_bits(),
                    super::super::inland_height(seed, x, z).to_bits()
                );
            }
        }
        for inland in [220., 420., 850., 1500.] {
            let mut previous = elevation(
                Info {
                    distance: -1.,
                    landmass_id: None,
                },
                inland,
            );
            for i in 0..=200 {
                let distance = -1. + i as f32 * 0.01;
                let h = elevation(
                    Info {
                        distance,
                        landmass_id: (distance >= 0.).then_some(0),
                    },
                    inland,
                );
                assert!(h.is_finite());
                assert!((h - previous).abs() < 0.01, "shore elevation jumps");
                previous = h;
            }
            assert_eq!(
                elevation(
                    Info {
                        distance: 0.,
                        landmass_id: Some(0)
                    },
                    inland
                ),
                0.
            );
            assert_eq!(
                elevation(
                    Info {
                        distance: 6000.,
                        landmass_id: Some(0)
                    },
                    inland
                ),
                inland
            );
        }
        assert_eq!(shore(30., 220.), ShoreKind::Beach);
        assert_eq!(shore(30., 1000.), ShoreKind::Cliff);
        assert_eq!(shore(1000., 1000.), ShoreKind::None);
    }
}

/// Wind-facing shores receive more wave energy; land upwind shelters bays and
/// the lee of islands. Evaluated only inside the coastal strip.
pub fn wave_exposure(seed: u32, x: f32, z: f32) -> f32 {
    let near = info(seed, x - 1400., z + 620.).distance;
    let far = info(seed, x - 4800., z + 2100.).distance;
    (0.18 + (1. - smooth(-900., 600., near)) * 0.42 + (1. - smooth(-1600., 1400., far)) * 0.40)
        .clamp(0., 1.)
}
fn coastal_profile(seed: u32, x: f32, z: f32, region: &crate::regions::Base) -> (f32, f32) {
    let wave = wave_exposure(seed, x, z);
    let resistance = region.weights[1] * 0.92
        + region.weights[2] * 0.53
        + region.weights[3] * 0.63
        + region.weights[4] * 0.97
        + region.weights[0] * 0.22;
    let cliff = (cliff_strength(region.height) * (0.40 + resistance * 0.60)
        + region.weights[3] * 0.58
        + region.weights[4] * 0.13)
        * (0.70 + wave * 0.30);
    let sediment = ((1. - resistance) * 0.40
        + (1. - wave) * 0.30
        + (1. - smooth(150., 700., region.height)) * 0.34)
        .clamp(0., 1.);
    (cliff.clamp(0., 1.), sediment)
}
pub fn regional_shore(
    seed: u32,
    x: f32,
    z: f32,
    c: Info,
    region: &crate::regions::Base,
) -> ShoreKind {
    if c.distance < -70. || c.distance > 750. {
        return ShoreKind::None;
    }
    let (cliff, sediment) = coastal_profile(seed, x, z, region);
    let wave = wave_exposure(seed, x, z);
    if cliff < 0.32 && wave < 0.44 && sediment > 0.50 && c.distance < 380. {
        return ShoreKind::SaltMarsh;
    }
    if cliff < 0.38 && sediment > 0.46 && c.distance > 110. && c.distance < 700. {
        return ShoreKind::Dunes;
    }
    if c.distance > lerp(120., 280., sediment.max(cliff)) {
        return ShoreKind::None;
    }
    if cliff > 0.47 {
        ShoreKind::Cliff
    } else if wave > 0.60 && region.weights[1] + region.weights[3] + region.weights[4] > 0.42 {
        ShoreKind::Shingle
    } else {
        ShoreKind::Beach
    }
}
pub fn regional_elevation(
    seed: u32,
    x: f32,
    z: f32,
    c: Info,
    region: &crate::regions::Base,
) -> f32 {
    if c.distance >= 5500. || c.distance <= -5500. {
        return elevation(c, region.height);
    }
    let d = c.distance;
    let (cliff, sediment) = coastal_profile(seed, x, z, region);
    if d <= 0. {
        let offshore = -d;
        let regional = -(3. * smooth(0., lerp(90., 230., sediment), offshore)
            + 42. * smooth(100., lerp(1400., 3200., sediment), offshore)
            + 260. * smooth(2000., 12000., offshore)
            + cliff * 80. * smooth(0., 250., offshore));
        return lerp(
            regional,
            elevation(c, region.height),
            smooth(3000., 5500., offshore),
        );
    }
    let beach = 3. * smooth(0., lerp(70., 160., sediment), d)
        + 12. * smooth(90., 650., d)
        + (region.height - 15.) * smooth(500., 5500., d);
    let dune = (noise(seed ^ 0x6691, (x + z * 0.3) / 180., z / 330.) * 4.0
        + noise(seed ^ 0x6692, x / 75., z / 110.) * 1.2)
        * smooth(40., 130., d)
        * (1. - smooth(320., 750., d))
        * sediment;
    // An exposed shore has a low wave-cut rock platform before its cliff face.
    // Width changes along the coast, making shelves and protected indentations
    // visible from the ground without moving the actual land/ocean boundary.
    let joints = noise(
        seed ^ 0x6693,
        (x * 0.78 + z * 0.625) / 740.,
        (z * 0.78 - x * 0.625) / 740.,
    );
    let shelf_width = 28. + cliff * 45. + (1. - sediment) * joints * 62.;
    let platform = 0.7 * smooth(0., 15., d) + (1.7 + cliff * 2.2) * smooth(18., shelf_width, d);
    let cliff_rise = 30. + (1. - cliff) * 75. + joints * 38.;
    let rock = platform
        + region.height * 0.22 * smooth(shelf_width, shelf_width + cliff_rise, d)
        + (region.height * 0.78 - platform) * smooth(350., 4400., d);
    let dune = dune * (1.3 + sediment * 1.5);
    let marsh = (1. - smooth(0.30, 0.48, wave_exposure(seed, x, z)))
        * (1. - smooth(0.24, 0.36, cliff))
        * smooth(0.46, 0.61, sediment);
    let tidal = 0.35 * smooth(0., 25., d)
        + 1.8 * smooth(0., 340., d)
        + (region.height - 2.15) * smooth(330., 5500., d);
    let ground = lerp(lerp(beach + dune, rock, cliff), tidal, marsh);
    // Sheltered back-barrier pockets. Their salt-water plane is the same sea
    // level as the shore; the dune ridge remains between lagoon and beach.
    let pocket = smooth(0.60, 0.76, noise(seed ^ 0x6695, x / 900., z / 900.));
    let lagoon = (1. - smooth(0.28, 0.47, wave_exposure(seed, x, z)))
        * (1. - smooth(0.15, 0.32, cliff))
        * smooth(0.44, 0.65, sediment)
        * pocket
        * smooth(110., 200., d)
        * (1. - smooth(340., 460., d));
    lerp(ground, -2.4, lagoon)
}

#[cfg(test)]
mod regional_coast_checks {
    use super::*;
    #[test]
    fn regional_shores_include_dunes_marsh_shingle_and_sheltered_lagoons() {
        let seed = 1337;
        let mut kinds = std::collections::BTreeSet::new();
        let mut lagoons = 0;
        for i in 0..360 {
            let a = i as f32 * std::f32::consts::TAU / 360.;
            let (dx, dz) = a.sin_cos();
            let (mut lo, mut hi) = (0., 155000.);
            for _ in 0..20 {
                let r = (lo + hi) * 0.5;
                if landmass_id(seed, dx * r, dz * r) == Some(0) {
                    lo = r;
                } else {
                    hi = r;
                }
            }
            for d in [40., 120., 200., 280., 360., 480., 650.] {
                let x = dx * (lo - d);
                let z = dz * (lo - d);
                let c = info(seed, x, z);
                let b = crate::regions::base(seed, x, z);
                kinds.insert(format!("{:?}", regional_shore(seed, x, z, c, &b)));
                let h = regional_elevation(seed, x, z, c, &b);
                assert!(h.is_finite());
                if c.distance > 100. && h < -0.2 {
                    lagoons += 1;
                }
            }
        }
        for kind in ["Dunes", "SaltMarsh", "Shingle", "Cliff", "Beach"] {
            assert!(kinds.contains(kind), "missing {kind}");
        }
        assert!(lagoons > 0, "no actual back-barrier water pockets");
    }
}
