//! Shared ecological cover for meshes, distant terrain, and maps.
//! Climate chooses the eligible vegetation; continuous regional fields determine
//! forests, sparse woodland, and genuinely open country inside those climates.
use crate::world::{hash, rand01, Biome, Sample};

#[derive(Clone, Copy, Debug)]
pub struct Ecology {
    /// Occupancy probability on the shared 12-meter tree candidate grid.
    pub tree_density: f32,
    pub grass_density: f32,
    pub grass_height: f32,
    pub flowers: f32,
    pub ferns: f32,
    pub heather: f32,
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
fn field(seed: u32, x: f32, z: f32) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let u = smooth(0.0, 1.0, x - ix as f32);
    let v = smooth(0.0, 1.0, z - iz as f32);
    let a = rand01(hash(seed, ix, iz));
    let b = rand01(hash(seed, ix + 1, iz));
    let c = rand01(hash(seed, ix, iz + 1));
    let d = rand01(hash(seed, ix + 1, iz + 1));
    (a + (b - a) * u) * (1.0 - v) + (c + (d - c) * u) * v
}

pub fn tree_density(seed: u32, x: f32, z: f32, terrain: &Sample) -> f32 {
    if matches!(terrain.biome, Biome::Alpine | Biome::Desert)
        || terrain.water_height > terrain.height - 0.3
    {
        return 0.0;
    }
    // Domain warping removes the appearance of rectangular noise cells. The
    // 1.1km region, 380m stand, and 190m opening fields have distinct roles.
    let wx = x + (field(seed ^ 0x45435758, x / 1900.0, z / 1900.0) - 0.5) * 380.0;
    let wz = z + (field(seed ^ 0x4543575a, x / 1900.0, z / 1900.0) - 0.5) * 380.0;
    let province = field(seed ^ 0x45435052, wx / 1100.0, wz / 1100.0);
    let stand = field(seed ^ 0x45435354, wx / 380.0, wz / 380.0);
    let opening = field(seed ^ 0x45434f50, x / 190.0, z / 190.0);
    let woodland = smooth(0.36, 0.61, province * 0.66 + stand * 0.34);
    let clearings = smooth(0.54, 0.82, opening);
    let canopy = woodland * (1.0 - clearings);
    let capacity = match terrain.biome {
        Biome::Forest | Biome::PineForest => 0.98,
        Biome::Grassland => 0.90,
        Biome::Wetland => 0.74,
        Biome::Moor => 0.20,
        _ => 0.0,
    };
    let moisture = 0.86 + 0.14 * smooth(0.20, 0.60, terrain.moisture);
    (0.002 + canopy.powf(1.35) * capacity * moisture).min(0.985)
}

pub fn sample(seed: u32, x: f32, z: f32, terrain: &Sample) -> Ecology {
    let trees = tree_density(seed, x, z, terrain);
    let open = 1.0 - smooth(0.08, 0.75, trees);
    let small = field(seed ^ 0x45434752, x / 65.0, z / 65.0);
    let bloom = smooth(0.51, 0.76, field(seed ^ 0x4543464c, x / 120.0, z / 120.0));
    let base = match terrain.biome {
        Biome::Grassland | Biome::Forest => 0.58 + open * 0.14,
        Biome::PineForest => 0.45 + open * 0.22,
        Biome::Wetland => 0.76,
        Biome::Moor => 0.67,
        Biome::Alpine => 0.16,
        Biome::Desert => 0.055,
    };
    let temperate = !matches!(terrain.biome, Biome::Desert | Biome::Alpine | Biome::Moor);
    Ecology {
        tree_density: trees,
        grass_density: (base * (0.76 + small * 0.40)).clamp(0.0, 0.88),
        grass_height: if matches!(terrain.biome, Biome::Desert | Biome::Alpine) {
            0.64
        } else {
            0.66 + open * 0.53 + small * 0.12
        },
        flowers: if temperate { bloom * open * 0.36 } else { 0.0 },
        ferns: if temperate {
            smooth(0.24, 0.66, terrain.moisture) * (0.05 + trees * 0.48)
        } else {
            0.0
        },
        heather: if terrain.biome == Biome::Moor {
            0.24 + bloom * 0.32
        } else if terrain.biome == Biome::PineForest {
            open * bloom * 0.15
        } else {
            0.0
        },
    }
}

/// The same canopy field darkens visible forest floors and their map footprints.
/// This deliberately avoids per-triangle hue noise that would obscure clearings.
pub fn ground_color(seed: u32, x: f32, z: f32, terrain: &Sample) -> [f32; 3] {
    let cover = smooth(0.08, 0.76, tree_density(seed, x, z, terrain));
    match terrain.biome {
        Biome::Grassland | Biome::Forest => mix([0.36, 0.50, 0.18], [0.25, 0.40, 0.17], cover),
        Biome::PineForest => mix([0.35, 0.47, 0.24], [0.22, 0.36, 0.25], cover),
        Biome::Moor => mix([0.43, 0.47, 0.25], [0.32, 0.40, 0.25], cover),
        Biome::Wetland => mix([0.34, 0.48, 0.25], [0.25, 0.39, 0.23], cover),
        Biome::Alpine => [0.52, 0.56, 0.46],
        Biome::Desert => [0.70, 0.55, 0.29],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temperate() -> Sample {
        Sample {
            height: 120.0,
            biome: Biome::Forest,
            road: 0.0,
            road_kind: None,
            river: 0.0,
            water_height: -10000.0,
            temperature: 0.55,
            moisture: 0.60,
        }
    }
    #[test]
    fn coherent_cover_has_open_scattered_and_dense_regions() {
        let s = temperate();
        let mut bins = [0usize; 4];
        let mut largest_neighbor_change = 0.0_f32;
        for z in -70..70 {
            for x in -70..70 {
                let px = x as f32 * 120.0;
                let pz = z as f32 * 120.0;
                let d = tree_density(1337, px, pz, &s);
                bins[if d < 0.025 {
                    0
                } else if d < 0.20 {
                    1
                } else if d < 0.75 {
                    2
                } else {
                    3
                }] += 1;
                largest_neighbor_change =
                    largest_neighbor_change.max((d - tree_density(1337, px + 1.0, pz, &s)).abs());
                assert_eq!(d, tree_density(1337, px, pz, &s));
            }
        }
        assert!(bins[0] > 3000, "no meaningful open country: {:?}", bins);
        assert!(
            bins[1] > 1500 && bins[2] > 2000 && bins[3] > 1000,
            "cover distribution: {:?}",
            bins
        );
        assert!(
            largest_neighbor_change < 0.05,
            "abrupt ecological seam: {}",
            largest_neighbor_change
        );
    }
    #[test]
    fn clearings_extend_across_multiple_tree_cells() {
        let s = temperate();
        let mut wide_open = 0;
        let mut dense_stands = 0;
        for z in -40..40 {
            for x in -40..40 {
                let px = x as f32 * 120.;
                let pz = z as f32 * 120.;
                let neighborhood = [[-32., 0.], [32., 0.], [0., -32.], [0., 32.], [0., 0.]]
                    .map(|p| tree_density(42, px + p[0], pz + p[1], &s));
                if neighborhood.iter().all(|d| *d < 0.025) {
                    wide_open += 1;
                }
                if neighborhood.iter().all(|d| *d > 0.80) {
                    dense_stands += 1;
                }
            }
        }
        assert!(
            wide_open > 300 && dense_stands > 100,
            "open{},dense{}",
            wide_open,
            dense_stands
        );
    }
    #[test]
    fn climate_and_water_exclude_inappropriate_trees() {
        for biome in [Biome::Alpine, Biome::Desert] {
            let mut s = temperate();
            s.biome = biome;
            assert_eq!(tree_density(1337, 500., 100., &s), 0.0);
        }
        let mut s = temperate();
        s.water_height = s.height + 0.2;
        assert_eq!(tree_density(1337, 500., 100., &s), 0.0);
    }
}
