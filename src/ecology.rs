//! Shared ecological cover for meshes, distant terrain, and maps.
//! Climate chooses the eligible vegetation; continuous regional fields determine
//! forests, sparse woodland, and genuinely open country inside those climates.
use crate::{
    regions::{self, Landscape},
    world::{hash, rand01, Biome, Sample, ShoreKind},
};

/// Forest communities sit inside climate biomes. They repeat in suitable
/// regions and blend through species weights, rather than changing the land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForestKind {
    Oakwood,
    BirchGrove,
    TallPines,
    CathedralFirs,
    Wetwood,
    Rainforest,
    PalmGrove,
    SavannaWood,
}
impl ForestKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Oakwood => "Oak & beech woodland",
            Self::BirchGrove => "Silver birch grove",
            Self::TallPines => "Tall pine & heath forest",
            Self::CathedralFirs => "Cathedral fir forest",
            Self::Wetwood => "Willow & alder wetwood",
            Self::Rainforest => "Emergent tropical rainforest",
            Self::PalmGrove => "Coastal palm grove",
            Self::SavannaWood => "Umbrella acacia savanna",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ForestStand {
    pub kind: ForestKind,
    /// Oak/beech, birch, pine, fir, willow, snag.
    pub species: [f32; 6],
    pub stature: f32,
    pub regeneration: f32,
}
pub fn forest_stand(seed: u32, x: f32, z: f32, s: &Sample) -> ForestStand {
    forest_in(seed, x, z, s, &regions::sample(seed, x, z, s))
}
fn forest_thicket(seed: u32, x: f32, z: f32) -> f32 {
    smooth(0.30, 0.66, field(seed ^ 0x46544849, x / 31., z / 31.))
}
pub fn forest_in(seed: u32, x: f32, z: f32, s: &Sample, region: &Landscape) -> ForestStand {
    let regeneration = smooth(0.44, 0.70, field(seed ^ 0x46594f55, x / 43., z / 43.));
    let tropical = match s.biome {
        Biome::Jungle => Some((ForestKind::Rainforest, [0.74, 0., 0., 0., 0.23, 0.03], 1.85)),
        Biome::TropicalCoast => Some((ForestKind::PalmGrove, [0.20, 0., 0., 0., 0.77, 0.03], 1.0)),
        Biome::Savanna => Some((ForestKind::SavannaWood, [0.94, 0., 0., 0., 0., 0.06], 0.9)),
        _ => None,
    };
    if let Some((kind, species, stature)) = tropical {
        return ForestStand {
            kind,
            species,
            stature,
            regeneration,
        };
    }
    let warp = field(seed ^ 0x46575250, x / 1700., z / 1700.) * 240.;
    let stand = field(seed ^ 0x464f5245, (x + warp) / 780., (z - warp) / 780.);
    let cool = if s.biome == Biome::PineForest {
        0.78
    } else {
        smooth(0.61, 0.30, s.temperature) * 0.65
    };
    let wet = if matches!(s.biome, Biome::Wetland | Biome::Swamp) {
        1.
    } else {
        smooth(0.68, 0.9, region.wetness) * 0.8
    };
    let birch = (region.pale * 0.85 + (1. - (stand - 0.50).abs() * 7.).max(0.) * 0.68).min(0.9);
    let needle = (cool + smooth(0.59, 0.79, stand) * 0.64).min(0.92) * (1. - region.ancient * 0.30);
    let fir = smooth(0.32, 0.70, s.moisture * 0.65 + stand * 0.35);
    let weights = [
        (1. - needle) * (1. - birch) * (1. - wet),
        (1. - needle) * birch * (1. - wet),
        needle * (1. - fir) * (1. - wet),
        needle * fir * (1. - wet),
        wet,
    ];
    // Sharpen the interiors while retaining continuous ecotones: each forest
    // has a recognizable dominant species, plus its companion trees.
    let weights = weights.map(|w| w * w * w);
    let weight_sum = weights.iter().sum::<f32>().max(0.00001);
    let weights = weights.map(|w| w / weight_sum);
    let recipes = [
        [0.55, 0.17, 0.10, 0.14, 0., 0.04],
        [0.18, 0.61, 0.08, 0.09, 0., 0.04],
        [0.07, 0.12, 0.60, 0.17, 0., 0.04],
        [0.06, 0.11, 0.19, 0.60, 0., 0.04],
        [0.21, 0.24, 0.03, 0.04, 0.44, 0.04],
    ];
    let index = (0..5)
        .max_by(|&a, &b| weights[a].total_cmp(&weights[b]))
        .unwrap();
    ForestStand {
        kind: [
            ForestKind::Oakwood,
            ForestKind::BirchGrove,
            ForestKind::TallPines,
            ForestKind::CathedralFirs,
            ForestKind::Wetwood,
        ][index],
        species: std::array::from_fn(|i| (0..5).map(|j| weights[j] * recipes[j][i]).sum()),
        stature: 1.0 + needle * 0.18 + region.ancient * 0.12,
        regeneration: smooth(0.44, 0.70, field(seed ^ 0x46594f55, x / 43., z / 43.)),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ecology {
    /// Occupancy probability on the shared 12-meter tree candidate grid.
    pub tree_density: f32,
    pub grass_density: f32,
    pub grass_height: f32,
    pub flowers: f32,
    pub ferns: f32,
    pub heather: f32,
    pub seedheads: f32,
    pub shrubs: f32,
    pub litter: f32,
    pub reeds: f32,
    /// Stable community pigment and flower group: cream0, gold1, blue2.
    pub cover_color: [f32; 3],
    pub flower_group: u32,
    pub undergrowth: f32,
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
    if terrain.ocean
        || terrain.shore != ShoreKind::None
        || matches!(terrain.biome, Biome::Alpine | Biome::Desert)
        || terrain.water_height > terrain.height - 0.3
    {
        return 0.0;
    }
    tree_density_in(seed, x, z, terrain, &regions::sample(seed, x, z, terrain))
}
pub(crate) fn tree_density_in(
    seed: u32,
    x: f32,
    z: f32,
    terrain: &Sample,
    region: &Landscape,
) -> f32 {
    if terrain.ocean
        || terrain.shore != ShoreKind::None
        || matches!(terrain.biome, Biome::Alpine | Biome::Desert)
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
    let ancient = region.ancient;
    let canopy = (woodland + ancient * 0.18).min(1.0) * (1.0 - clearings * (1.0 - ancient * 0.24));
    let capacity = match terrain.biome {
        Biome::Forest | Biome::PineForest => 0.98,
        Biome::Grassland => 0.24,
        Biome::Savanna => 0.12,
        Biome::Jungle => 0.98,
        Biome::TropicalCoast => 0.52,
        Biome::Swamp => 0.68,
        Biome::Wetland => 0.74,
        Biome::Moor => 0.20,
        _ => 0.0,
    };
    let moisture = 0.86 + 0.14 * smooth(0.20, 0.60, terrain.moisture);
    let shelter = 1.0 - region.exposure * 0.22;
    (0.002 + canopy.powf(1.35) * capacity * moisture * shelter).min(0.985)
}

pub fn sample(seed: u32, x: f32, z: f32, terrain: &Sample) -> Ecology {
    let region = regions::sample(seed, x, z, terrain);
    let clump = field(seed ^ 0x45434752, x / 34.0, z / 34.0);
    if terrain.ocean || terrain.shore != ShoreKind::None {
        // Salty exposed sites have sparse tough grass, never inland fern beds.
        let dry = !terrain.ocean && terrain.water_height < terrain.height - 0.3;
        let density = if dry && terrain.shore == ShoreKind::Beach && terrain.height > 2.0 {
            0.025 + smooth(0.40, 0.72, clump) * 0.11
        } else if dry
            && terrain.shore == ShoreKind::Cliff
            && terrain.height > 6.0
            && region.slope < 0.75
        {
            smooth(0.48, 0.72, clump) * 0.09
        } else {
            0.0
        };
        return Ecology {
            tree_density: 0.0,
            grass_density: density,
            grass_height: 0.82,
            flowers: 0.0,
            ferns: 0.0,
            heather: 0.0,
            seedheads: 0.0,
            shrubs: 0.0,
            litter: 0.0,
            reeds: 0.0,
            cover_color: mix([0.42, 0.49, 0.25], [0.59, 0.61, 0.34], clump),
            flower_group: 0,
            undergrowth: 0.,
        };
    }
    let trees = tree_density_in(seed, x, z, terrain, &region);
    let thicket = forest_thicket(seed, x, z);
    let interior = smooth(0.22, 0.78, trees);
    let open = 1.0 - smooth(0.08, 0.75, trees);
    let bloom_field = field(seed ^ 0x4543464c, x / 76.0, z / 76.0);
    let patch = smooth(0.47, 0.72, bloom_field) * smooth(0.22, 0.64, clump);
    let fern_patch = smooth(0.30, 0.68, field(seed ^ 0x4645524e, x / 29.0, z / 29.0));
    let wet_bank = smooth(0.28, 0.70, region.wetness) * (0.65 + region.soil * 0.35);
    let dry = (1.0 - smooth(0.25, 0.62, region.wetness)) * open;
    let bank = if terrain.water_height > -999.0 {
        (1.0 - smooth(0.6, 5.5, terrain.height - terrain.water_height))
            * (1.0 - smooth(0.20, 0.55, region.slope))
    } else {
        0.0
    };
    let temperate = !matches!(terrain.biome, Biome::Desert | Biome::Alpine | Biome::Moor);
    // Dense stands deliberately leave quiet litter between fern colonies, while
    // open meadows carry grasses punctuated by whole patches of flowering plants.
    let base = match terrain.biome {
        Biome::Grassland | Biome::Forest => 0.37 + open * 0.36,
        Biome::PineForest => 0.30 + open * 0.30,
        Biome::Wetland => 0.69,
        Biome::Swamp => 0.42,
        Biome::Jungle => 0.72,
        Biome::TropicalCoast => 0.52,
        Biome::Savanna => 0.79,
        Biome::Moor => 0.58,
        Biome::Alpine => 0.14,
        Biome::Desert => 0.055,
    };
    let mut flowers = if temperate {
        patch * (open + interior * 0.09) * (0.40 + region.soil * 0.25)
    } else {
        0.0
    };
    let mut ferns = if temperate {
        (0.18 + wet_bank * 0.82)
            * (0.09 + trees * 0.82)
            * fern_patch
            * (1.0 - region.exposure * 0.60)
    } else {
        0.0
    };
    let mut heather = if terrain.biome == Biome::Moor {
        0.18 + patch * 0.44
    } else if terrain.biome == Biome::PineForest {
        patch * open * 0.17
    } else {
        0.0
    };
    let mut seedheads = if terrain.biome != Biome::Wetland {
        dry * (0.13 + clump * 0.30)
    } else {
        0.0
    };
    let edge = smooth(0.02, 0.22, trees) * (1.0 - smooth(0.55, 0.9, trees));
    let mut shrubs = if temperate || terrain.biome == Biome::Moor {
        (edge * 0.25 + dry * 0.10) * smooth(0.26, 0.68, clump) + interior * thicket * 0.44
    } else {
        0.0
    };
    let mut litter = if temperate {
        trees * (0.12 + (1.0 - fern_patch) * 0.28)
    } else {
        0.0
    };
    let mut reeds = bank * (0.35 + wet_bank * 0.44) * smooth(0.25, 0.64, clump);
    if terrain.biome == Biome::Jungle {
        ferns += fern_patch * 0.55;
        shrubs += interior * thicket * 0.22;
        flowers *= 0.3;
    }
    if terrain.biome == Biome::Swamp {
        reeds += smooth(0.35, 0.72, clump) * 0.38;
        shrubs *= 0.7;
        flowers *= 0.08;
        seedheads *= 0.25;
    }
    if terrain.biome == Biome::Savanna {
        seedheads += 0.25;
        ferns = 0.;
        flowers *= 0.2;
    }
    let normalize = (0.97
        / (flowers + ferns + heather + seedheads + shrubs + litter + reeds).max(0.97))
    .min(1.0);
    flowers *= normalize;
    ferns *= normalize;
    heather *= normalize;
    seedheads *= normalize;
    shrubs *= normalize;
    litter *= normalize;
    reeds *= normalize;
    let colony = field(seed ^ 0x50455441, x / 170.0, z / 170.0);
    let flower_group = if region.wetness > 0.55 && colony > 0.57 {
        2
    } else if colony < 0.45 {
        0
    } else {
        1
    };
    let grass_color = match terrain.biome {
        Biome::Grassland | Biome::Forest => mix([0.24, 0.38, 0.16], [0.40, 0.53, 0.20], open),
        Biome::PineForest => mix([0.25, 0.35, 0.22], [0.39, 0.47, 0.26], open),
        Biome::Wetland => [0.34, 0.48, 0.23],
        Biome::Swamp => [0.26, 0.37, 0.20],
        Biome::Jungle => [0.16, 0.40, 0.23],
        Biome::TropicalCoast => [0.33, 0.53, 0.22],
        Biome::Savanna => [0.64, 0.57, 0.26],
        Biome::Moor => [0.46, 0.44, 0.25],
        Biome::Alpine => [0.47, 0.48, 0.32],
        Biome::Desert => [0.65, 0.53, 0.29],
    };
    Ecology {
        tree_density: trees,
        grass_density: ((base + interior * thicket * 0.22 + ferns * 0.22)
            * (0.67 + clump * 0.47)
            * (1.0 - region.rockiness * 0.48))
            .clamp(0.0, 0.86),
        grass_height: if matches!(terrain.biome, Biome::Desert | Biome::Alpine) {
            0.61
        } else {
            (0.68 + open * 0.37 + wet_bank * 0.12) * (1.0 - region.exposure * 0.14)
        },
        flowers,
        ferns,
        heather,
        seedheads,
        shrubs,
        litter,
        reeds,
        cover_color: mix(grass_color, [0.58, 0.49, 0.25], dry * 0.30),
        flower_group,
        undergrowth: interior * thicket,
    }
}

/// Broad substrate follows geology; canopy and community-scale fields tie the
/// visible ground to the plants above it. The atlas samples this same palette.
pub fn ground_color(seed: u32, x: f32, z: f32, terrain: &Sample) -> [f32; 3] {
    let patch = field(seed ^ 0x534f494c, x / 24.0, z / 24.0);
    if terrain.ocean || terrain.shore == ShoreKind::Beach {
        let sand = mix([0.55, 0.50, 0.36], [0.70, 0.64, 0.45], patch);
        return mix([0.34, 0.38, 0.30], sand, smooth(-0.5, 2.8, terrain.height));
    }
    let region = regions::sample(seed, x, z, terrain);
    if terrain.shore == ShoreKind::Cliff {
        return mix(region.rock_color, [0.48, 0.48, 0.39], 0.10 + patch * 0.10);
    }
    let cover = smooth(0.08, 0.76, tree_density_in(seed, x, z, terrain, &region));
    let open_color = match terrain.biome {
        Biome::Grassland | Biome::Forest => [0.39, 0.51, 0.22],
        Biome::PineForest => [0.38, 0.46, 0.28],
        Biome::Moor => [0.46, 0.44, 0.31],
        Biome::Wetland => [0.36, 0.46, 0.26],
        Biome::Swamp => [0.29, 0.26, 0.18],
        Biome::Jungle => [0.22, 0.37, 0.18],
        Biome::TropicalCoast => [0.41, 0.53, 0.25],
        Biome::Savanna => [0.60, 0.53, 0.25],
        Biome::Alpine => [0.53, 0.55, 0.46],
        Biome::Desert => [0.71, 0.57, 0.36],
    };
    let forest_floor = if terrain.biome == Biome::PineForest {
        [0.31, 0.29, 0.18]
    } else {
        [0.28, 0.34, 0.15]
    };
    let substrate = mix(
        open_color,
        region.ground_color,
        0.42 + region.rockiness * 0.40,
    );
    // Thin soil exposes the actual regional stone, with broad patches rather
    // than a uniformly green alpine or sandstone surface.
    let exposed = smooth(0.35, 0.82, region.rockiness) * (0.62 + patch * 0.32);
    let substrate = mix(substrate, region.rock_color, exposed);
    let moss = field(seed ^ 0x4645524e, x / 29., z / 29.);
    let soil = mix(
        [0.34, 0.27, 0.16],
        forest_floor,
        smooth(0.25, 0.71, moss) * (0.6 + region.wetness * 0.4),
    );
    let mut ground = mix(substrate, soil, cover * 0.82);
    if terrain.water_height > -999.0 {
        let clearance = terrain.height - terrain.water_height;
        let margin =
            (1.0 - smooth(0.35, 5.0, clearance)) * (1.0 - smooth(0.14, 0.50, region.slope));
        let gravel = mix(region.rock_color, [0.47, 0.43, 0.31], 0.52);
        let mud = mix([0.26, 0.29, 0.20], gravel, smooth(0.35, 0.72, patch));
        ground = mix(ground, mud, margin * 0.85);
    }
    if terrain.biome == Biome::Swamp {
        ground = mix(ground, [0.25, 0.225, 0.16], 0.58 + patch * 0.24);
    }
    if terrain.biome == Biome::Savanna {
        ground = mix(ground, [0.61, 0.53, 0.28], 0.56);
    }
    if terrain.biome == Biome::Desert {
        ground = mix(ground, [0.73, 0.57, 0.34], 0.65);
    }
    let snow = crate::climate::snow_cover(terrain.temperature, region.slope)
        * smooth(1400., 2000., terrain.height);
    mix(ground, [0.82, 0.87, 0.90], snow)
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
            ocean: false,
            shore: ShoreKind::None,
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
    #[test]
    fn forests_have_blended_species_regeneration_and_interior_colonies() {
        let mut kinds = std::collections::HashSet::new();
        let mut colonies = 0;
        let mut gaps = 0;
        for biome in [Biome::Forest, Biome::PineForest, Biome::Wetland] {
            let mut terrain = temperate();
            terrain.biome = biome;
            for z in -20..20 {
                for x in -20..20 {
                    let px = x as f32 * 533.;
                    let pz = z as f32 * 533.;
                    terrain.moisture = 0.25 + (x + 20) as f32 / 40. * 0.55;
                    let a = forest_stand(1337, px, pz, &terrain);
                    let b = forest_stand(1337, px + 1., pz, &terrain);
                    assert!((a.species.iter().sum::<f32>() - 1.).abs() < 0.0001);
                    assert!(a.species.iter().filter(|&&w| w > 0.05).count() >= 3);
                    assert!(a
                        .species
                        .iter()
                        .zip(b.species)
                        .all(|(a, b)| (a - b).abs() < 0.015));
                    assert_eq!(a.species, forest_stand(1337, px, pz, &terrain).species);
                    kinds.insert(a.kind.name());
                    let e = sample(1337, px, pz, &terrain);
                    if e.tree_density > 0.75 {
                        colonies += usize::from(e.shrubs > 0.15 || e.ferns > 0.20);
                        gaps += usize::from(e.shrubs < 0.06 && e.ferns < 0.10);
                    }
                }
            }
        }
        assert_eq!(kinds.len(), 5, "missing forest families: {kinds:?}");
        assert!(
            colonies > 30 && gaps > 10,
            "colonies={colonies} gaps={gaps}"
        );
    }
    #[test]
    fn saltwater_excludes_plants_and_exposed_shores_have_only_sparse_grass() {
        for shore in [ShoreKind::Beach, ShoreKind::Cliff] {
            let mut terrain = temperate();
            terrain.shore = shore;
            let cover = sample(1337, 500., 100., &terrain);
            assert_eq!(cover.tree_density, 0.0);
            assert_eq!(cover.ferns + cover.flowers + cover.heather, 0.0);
            assert!(cover.grass_density <= 0.14);
            assert_eq!(cover.seedheads, 0.0);
        }
        let mut terrain = temperate();
        terrain.ocean = true;
        terrain.height = -15.0;
        let cover = sample(1337, 500., 100., &terrain);
        assert_eq!(cover.tree_density + cover.grass_density, 0.0);
    }
    #[test]
    fn communities_and_palettes_are_bounded_repeatable_and_patchy() {
        let mut terrain = temperate();
        let mut flower_patches = 0;
        let mut quiet_patches = 0;
        let mut fern_patches = 0;
        for biome in [
            Biome::Grassland,
            Biome::Forest,
            Biome::PineForest,
            Biome::Wetland,
            Biome::Moor,
            Biome::Alpine,
            Biome::Desert,
        ] {
            terrain.biome = biome;
            for z in -15..15 {
                for x in -15..15 {
                    let (x, z) = (x as f32 * 173.0, z as f32 * 173.0);
                    let a = sample(1337, x, z, &terrain);
                    let b = sample(1337, x, z, &terrain);
                    assert_eq!(a.cover_color, b.cover_color);
                    assert_eq!(a.flower_group, b.flower_group);
                    for value in [
                        a.tree_density,
                        a.grass_density,
                        a.flowers,
                        a.ferns,
                        a.heather,
                        a.seedheads,
                    ] {
                        assert!(value.is_finite() && (0.0..=1.0).contains(&value));
                    }
                    assert!(
                        a.flowers
                            + a.ferns
                            + a.heather
                            + a.seedheads
                            + a.shrubs
                            + a.litter
                            + a.reeds
                            <= 0.971
                    );
                    assert!(a.grass_height > 0.0 && a.grass_height < 1.4);
                    assert!(a.flower_group <= 2);
                    let color = ground_color(1337, x, z, &terrain);
                    assert!(color
                        .iter()
                        .chain(a.cover_color.iter())
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v)));
                    if biome == Biome::Forest {
                        if a.flowers > 0.16 {
                            flower_patches += 1;
                        }
                        if a.flowers < 0.01 {
                            quiet_patches += 1;
                        }
                        if a.ferns > 0.10 {
                            fern_patches += 1;
                        }
                    }
                }
            }
        }
        assert!(
            flower_patches > 20 && quiet_patches > 200 && fern_patches > 20,
            "communities: flowers{flower_patches}, quiet{quiet_patches}, ferns{fern_patches}"
        );
    }
}
