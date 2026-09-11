//! Deterministic regional geography. All coordinates and elevations are meters.
//!
//! A continental DEM drives Priority-Flood spill routing, D8 catchments and
//! accumulated river flow. Profiles carve downhill spillways through depressions;
//! this is hydrologic conditioning, not a dynamic erosion simulation.

use serde::Serialize;
use std::{cell::RefCell, rc::Rc};

pub const WORLD_SIZE: f32 = 384_000.0;
pub const SITE_SPACING: f32 = 2_400.0;
const HALF_WORLD: f32 = WORLD_SIZE * 0.5;
const SITE_LIMIT: i32 = (HALF_WORLD / SITE_SPACING) as i32 + 2;
const LANDMARK_LIMIT: i32 = (HALF_WORLD / 640.0) as i32 + 2;
#[path = "coast.rs"]
pub(crate) mod coast;
pub use coast::{Info as CoastInfo, ShoreKind, SEA_LEVEL};
#[path = "hydrology.rs"]
mod hydrology;
pub use hydrology::{LakeInfo, Stats as HydrologyStats, WaterEdge, WaterEdgeKind};
const LANDMARK_SPACING: f32 = 640.0;
#[path = "roads.rs"]
mod roads;
pub use roads::{Road, RoadKind, Stats as RoadStats};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Biome {
    Grassland,
    Forest,
    PineForest,
    Moor,
    Alpine,
    Desert,
    Wetland,
}

impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Self::Grassland => "Grassland",
            Self::Forest => "Temperate forest",
            Self::PineForest => "Pine forest",
            Self::Moor => "Highland moor",
            Self::Alpine => "Alpine highlands",
            Self::Desert => "Drylands",
            Self::Wetland => "River wetlands",
        }
    }

    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Grassland => [0.39, 0.45, 0.22],
            Self::Forest => [0.27, 0.37, 0.19],
            Self::PineForest => [0.24, 0.34, 0.28],
            Self::Moor => [0.39, 0.40, 0.31],
            Self::Alpine => [0.53, 0.54, 0.51],
            Self::Desert => [0.66, 0.55, 0.34],
            Self::Wetland => [0.30, 0.40, 0.29],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub height: f32,
    pub biome: Biome,
    pub road: f32,
    pub road_kind: Option<RoadKind>,
    pub ocean: bool,
    pub shore: ShoreKind,
    pub river: f32,
    pub water_height: f32,
    pub temperature: f32,
    pub moisture: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Site {
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub z: f32,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Landmark {
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub z: f32,
    pub kind: String,
}

#[derive(Clone, Debug)]
pub struct World {
    pub seed: u32,
    hydrology: Rc<hydrology::Hydrology>,
    roads: Rc<roads::Network>,
    spawn_cache: RefCell<Option<([f32; 2], f32)>>,
}

#[derive(Clone, Copy)]
struct RoadHit {
    distance: f32,
    point: [f32; 2],
    kind: Option<RoadKind>,
}

/// Spatial hashing uses wrapping integer arithmetic, including negative cells.
pub fn hash(seed: u32, x: i32, z: i32) -> u32 {
    let mut h = seed ^ (x as u32).wrapping_mul(0x9e37_79b9) ^ (z as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

pub fn rand01(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn noise(seed: u32, x: f32, z: f32) -> f32 {
    let ix = x.floor() as i32;
    let iz = z.floor() as i32;
    let tx = x - ix as f32;
    let tz = z - iz as f32;
    let tx = tx * tx * (3.0 - 2.0 * tx);
    let tz = tz * tz * (3.0 - 2.0 * tz);
    lerp(
        lerp(
            rand01(hash(seed, ix, iz)),
            rand01(hash(seed, ix + 1, iz)),
            tx,
        ),
        lerp(
            rand01(hash(seed, ix, iz + 1)),
            rand01(hash(seed, ix + 1, iz + 1)),
            tx,
        ),
        tz,
    )
}

fn distance2(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

fn segment_hit(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> RoadHit {
    let d = [b[0] - a[0], b[1] - a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2.max(0.001)).clamp(0.0, 1.0);
    let point = [a[0] + d[0] * t, a[1] + d[1] * t];
    RoadHit {
        distance: distance2(p, point).sqrt(),
        point,
        kind: None,
    }
}

fn raw_height(seed: u32, x: f32, z: f32) -> f32 {
    let region = crate::regions::base(seed, x, z);
    coast::regional_elevation(seed, x, z, coast::info(seed, x, z), &region)
}
#[cfg(test)]
fn inland_height(seed: u32, x: f32, z: f32) -> f32 {
    crate::regions::base(seed, x, z).height
}

impl World {
    pub fn new(seed: u32) -> Self {
        let mut world = Self {
            seed,
            hydrology: Rc::new(hydrology::Hydrology::new(seed)),
            roads: Rc::new(roads::Network::empty()),
            spawn_cache: RefCell::new(None),
        };
        world.roads = Rc::new(roads::Network::new(&world));
        world
    }

    fn river(&self, x: f32, z: f32) -> hydrology::Hit {
        self.hydrology.nearest(x, z)
    }

    fn ground(&self, x: f32, z: f32) -> f32 {
        self.hydrology
            .terrain(x, z, raw_height(self.seed, x, z))
            .height
    }

    pub fn coast_info(&self, x: f32, z: f32) -> CoastInfo {
        coast::info(self.seed, x, z)
    }
    pub fn is_land(&self, x: f32, z: f32) -> bool {
        self.landmass_id(x, z).is_some()
    }
    pub fn landmass_id(&self, x: f32, z: f32) -> Option<u32> {
        coast::landmass_id(self.seed, x, z)
    }
    pub fn lakes(&self) -> &[LakeInfo] {
        &self.hydrology.lakes
    }
    pub fn lake_at(&self, x: f32, z: f32) -> Option<&LakeInfo> {
        self.hydrology.lake_at(x, z, || raw_height(self.seed, x, z))
    }
    pub fn landscape(&self, x: f32, z: f32) -> crate::regions::Landscape {
        crate::regions::sample(self.seed, x, z, &self.natural_sample(x, z))
    }
    /// Descriptive near-bank ecology/prop metadata; never changes water geometry.
    pub fn water_edge(&self, x: f32, z: f32) -> WaterEdge {
        let c = self.coast_info(x, z);
        let region = crate::regions::base(self.seed, x, z);
        let raw = coast::regional_elevation(self.seed, x, z, c, &region);
        if c.distance.abs() < 260. {
            let shore = coast::regional_shore(self.seed, x, z, c, &region);
            if shore != ShoreKind::None {
                let influence = 1. - smooth(90., 260., c.distance.abs());
                return WaterEdge {
                    kind: if shore == ShoreKind::Cliff {
                        WaterEdgeKind::CoastalShelf
                    } else {
                        WaterEdgeKind::CoastalSand
                    },
                    influence,
                    sediment: if shore == ShoreKind::Beach {
                        0.85
                    } else {
                        0.15
                    },
                    wetness: (1. - smooth(0., 4., raw)) * influence,
                    water_level: SEA_LEVEL,
                    flow: [0.91, -0.41],
                };
            }
        }
        self.hydrology.water_edge(x, z, raw)
    }
    pub fn water_flow(&self, x: f32, z: f32) -> [f32; 2] {
        self.river(x, z).tangent
    }
    pub fn hydrology_stats(&self) -> HydrologyStats {
        self.hydrology.stats.clone()
    }
    pub fn vegetation_density(&self, x: f32, z: f32) -> f32 {
        self.vegetation_density_from_sample(x, z, &self.sample(x, z))
    }
    pub fn vegetation_density_from_sample(&self, x: f32, z: f32, sample: &Sample) -> f32 {
        crate::ecology::tree_density(self.seed, x, z, sample)
    }

    fn node(&self, i: i32, j: i32) -> [f32; 2] {
        let original = [
            i as f32 * SITE_SPACING + (rand01(hash(self.seed ^ 0x2401, i, j)) - 0.5) * 1600.0,
            j as f32 * SITE_SPACING + (rand01(hash(self.seed ^ 0x2402, i, j)) - 0.5) * 1600.0,
        ];
        let mut p = original;
        for _ in 0..4 {
            let river = self.river(p[0], p[1]);
            if river.distance >= river.width * 2.0 + 85.0 {
                break;
            }
            let dir = if river.distance > 0.1 {
                [
                    (p[0] - river.point[0]) / river.distance,
                    (p[1] - river.point[1]) / river.distance,
                ]
            } else {
                [river.tangent[1], -river.tangent[0]]
            };
            let radius = river.width * 2.0 + 115.0;
            p = [
                river.point[0] + dir[0] * radius,
                river.point[1] + dir[1] * radius,
            ];
        }
        p
    }

    fn nearest_road(&self, x: f32, z: f32) -> RoadHit {
        self.roads.nearest(self, x, z)
    }
    pub fn road_stats(&self) -> RoadStats {
        self.roads.stats.clone()
    }
    pub fn road_routes_near(&self, cx: f32, cz: f32, radius: f32) -> Vec<Road> {
        self.roads.near(self, cx, cz, radius)
    }
    pub fn road_map_routes(&self, cx: f32, cz: f32, span: f32) -> Vec<Road> {
        self.roads.map_routes(self, cx, cz, span)
    }
    pub fn natural_sample(&self, x: f32, z: f32) -> Sample {
        self.sample_impl(x, z, false)
    }
    pub fn sample(&self, x: f32, z: f32) -> Sample {
        self.sample_impl(x, z, true)
    }
    fn sample_impl(&self, x: f32, z: f32, include_roads: bool) -> Sample {
        let x = x.clamp(-HALF_WORLD, HALF_WORLD);
        let z = z.clamp(-HALF_WORLD, HALF_WORLD);
        let coast = self.coast_info(x, z);
        let regional = crate::regions::base(self.seed, x, z);

        let raw = coast::regional_elevation(self.seed, x, z, coast, &regional);
        let mut water = self.hydrology.terrain(x, z, raw);
        if coast.landmass_id.is_none() {
            water.height = raw;
        }
        let ocean = coast.landmass_id.is_none()
            || (coast.distance < 500.
                && water.height < SEA_LEVEL
                && water.water <= SEA_LEVEL + 0.05);
        if ocean {
            water.water = water.water.max(SEA_LEVEL);
        }

        let river = water.nearest;
        let mut height = water.height;
        let road_hit = if include_roads && !ocean {
            self.nearest_road(x, z)
        } else {
            RoadHit {
                distance: f32::MAX,
                point: [x, z],
                kind: None,
            }
        };
        let road = road_hit
            .kind
            .map_or(0., |kind| kind.strength(road_hit.distance));
        if road > 0.0 && water.water < height {
            height = lerp(
                height,
                self.ground(road_hit.point[0], road_hit.point[1]) + 0.015,
                road * road_hit.kind.map_or(0., RoadKind::grading),
            );
        }
        let continental_heat = noise(self.seed ^ 0x3101, x / 47000.0, z / 47000.0);
        let temperature = (0.66 + (continental_heat - 0.5) * 0.52 + z / 128000.0 * 0.23
            - (height - 180.0).max(0.0) * 0.00043)
            .clamp(0.0, 1.0);
        let rain = regional.rainfall;
        let wet_edge = 1.0 - smooth(river.width * 1.8, river.width + 520.0, river.distance);
        let moisture = (rain + wet_edge * 0.22 - regional.exposure * 0.08).clamp(0.0, 1.0);
        let cover = noise(self.seed ^ 0x3103, x / 720.0, z / 720.0);
        let biome = if height > 1250.0 || (temperature < 0.17 && height > 720.0) {
            Biome::Alpine
        } else if river.distance < river.width + 42.0 && moisture > 0.48 {
            Biome::Wetland
        } else if temperature > 0.61 && moisture < 0.36 {
            Biome::Desert
        } else if temperature < 0.38 || height > 720.0 {
            if moisture > 0.34 && cover > 0.30 && height < 1150.0 {
                Biome::PineForest
            } else {
                Biome::Moor
            }
        } else if moisture > 0.48 && cover > 0.37 {
            Biome::Forest
        } else {
            Biome::Grassland
        };
        Sample {
            height,
            biome,
            road,
            road_kind: road_hit.kind.filter(|_| road > 0.),
            ocean,
            shore: if coast.distance < 260.
                && coast.distance > -70.
                && height < 12.
                && river.width > 6.
                && river.distance < river.width * 3. + 65.
            {
                ShoreKind::Beach
            } else {
                coast::regional_shore(self.seed, x, z, coast, &regional)
            },
            river: water.river,
            water_height: water.water,
            temperature,
            moisture,
        }
    }
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let x = x.clamp(-HALF_WORLD, HALF_WORLD);
        let z = z.clamp(-HALF_WORLD, HALF_WORLD);
        let coast = self.coast_info(x, z);
        let regional = crate::regions::base(self.seed, x, z);

        let raw = coast::regional_elevation(self.seed, x, z, coast, &regional);
        let mut water = self.hydrology.terrain(x, z, raw);
        if coast.landmass_id.is_none() {
            water.height = raw;
        }
        let ocean = coast.landmass_id.is_none()
            || (coast.distance < 500.
                && water.height < SEA_LEVEL
                && water.water <= SEA_LEVEL + 0.05);
        if ocean {
            water.water = water.water.max(SEA_LEVEL);
        }

        let road_hit = if !ocean {
            self.nearest_road(x, z)
        } else {
            RoadHit {
                distance: f32::MAX,
                point: [x, z],
                kind: None,
            }
        };
        let road = road_hit
            .kind
            .map_or(0., |kind| kind.strength(road_hit.distance));
        if road > 0.0 && water.water < water.height {
            lerp(
                water.height,
                self.ground(road_hit.point[0], road_hit.point[1]) + 0.015,
                road * road_hit.kind.map_or(0., RoadKind::grading),
            )
        } else {
            water.height
        }
    }

    fn site(&self, i: i32, j: i32) -> Site {
        let id = hash(self.seed ^ 0x4101, i, j);
        let p = self.node(i, j);
        const PREFIX: [&str; 24] = [
            "Alder", "Ash", "Black", "Briar", "Brook", "Cinder", "Dawn", "Dun", "East", "Elder",
            "Fair", "Fallow", "Grey", "High", "Kings", "Mist", "North", "Oak", "Raven", "Red",
            "Silver", "Thorn", "West", "White",
        ];
        const SUFFIX: [&str; 20] = [
            "ford", "vale", "wick", "haven", "stead", "bridge", "mere", "holt", "watch", "fell",
            "bury", "crest", "field", "march", "wood", "hearth", "gate", "well", "cross", "reach",
        ];
        Site {
            id,
            name: format!(
                "{}{}",
                PREFIX[(id as usize) % PREFIX.len()],
                SUFFIX[((id >> 8) as usize) % SUFFIX.len()]
            ),
            x: p[0],
            z: p[1],
            kind: roads::site_kind(id).to_owned(),
        }
    }

    pub fn sites_near(&self, x: f32, z: f32, radius: f32) -> Vec<Site> {
        if !radius.is_finite() || radius < 0.0 {
            return Vec::new();
        }
        let radius = radius.min(WORLD_SIZE * 1.5);
        let min_i = (((x - radius) / SITE_SPACING).floor() as i32 - 1).max(-SITE_LIMIT);
        let max_i = (((x + radius) / SITE_SPACING).ceil() as i32 + 1).min(SITE_LIMIT);
        let min_j = (((z - radius) / SITE_SPACING).floor() as i32 - 1).max(-SITE_LIMIT);
        let max_j = (((z + radius) / SITE_SPACING).ceil() as i32 + 1).min(SITE_LIMIT);
        let mut result = Vec::new();
        for i in min_i..=max_i {
            for j in min_j..=max_j {
                let p = self.node(i, j);
                if p[0].abs() <= HALF_WORLD
                    && p[1].abs() <= HALF_WORLD
                    && self.coast_info(p[0], p[1]).distance > 80.
                    && self.ground(p[0], p[1]) > SEA_LEVEL + 0.5
                    && self.lake_at(p[0], p[1]).is_none()
                    && distance2(p, [x, z]) <= radius * radius
                {
                    result.push(self.site(i, j));
                }
            }
        }
        result
    }

    pub fn landmarks_near(&self, x: f32, z: f32, radius: f32) -> Vec<Landmark> {
        if !radius.is_finite() || radius < 0.0 {
            return Vec::new();
        }
        let radius = radius.min(WORLD_SIZE * 1.5);
        let min_i = (((x - radius) / LANDMARK_SPACING).floor() as i32 - 1).max(-LANDMARK_LIMIT);
        let max_i = (((x + radius) / LANDMARK_SPACING).ceil() as i32 + 1).min(LANDMARK_LIMIT);
        let min_j = (((z - radius) / LANDMARK_SPACING).floor() as i32 - 1).max(-LANDMARK_LIMIT);
        let max_j = (((z + radius) / LANDMARK_SPACING).ceil() as i32 + 1).min(LANDMARK_LIMIT);
        let mut result = Vec::new();
        const KINDS: [&str; 6] = [
            "ruin",
            "standing_stones",
            "watchtower",
            "camp",
            "shrine",
            "waystone",
        ];
        const TITLES: [&str; 6] = [
            "Ruins",
            "Standing Stones",
            "Watch",
            "Camp",
            "Shrine",
            "Waystone",
        ];
        const NAMES: [&str; 20] = [
            "The Forgotten",
            "Old King's",
            "Raven's",
            "The Hollow",
            "Last Light",
            "Pilgrim's",
            "The Silent",
            "Grey Wolf",
            "Wanderer's",
            "The Fallen",
            "Moonlit",
            "Ashen",
            "Saint Orin's",
            "Foxglove",
            "The Broken",
            "The Elder",
            "Wayfarer's",
            "The Lost",
            "Silver Oak",
            "Storm",
        ];
        for i in min_i..=max_i {
            for j in min_j..=max_j {
                let id = hash(self.seed ^ 0x5101, i, j);
                // Keep occasional empty cells so wilderness still has quiet stretches.
                if id % 7 == 0 {
                    continue;
                }
                let px = (i as f32 + 0.5) * LANDMARK_SPACING
                    + (rand01(hash(self.seed ^ 0x5102, i, j)) - 0.5) * 240.0;
                let pz = (j as f32 + 0.5) * LANDMARK_SPACING
                    + (rand01(hash(self.seed ^ 0x5103, i, j)) - 0.5) * 240.0;
                if px.abs() > HALF_WORLD
                    || pz.abs() > HALF_WORLD
                    || distance2([px, pz], [x, z]) > radius * radius
                {
                    continue;
                }
                if self.coast_info(px, pz).distance < 60. || self.ground(px, pz) < SEA_LEVEL + 0.25
                {
                    continue;
                }
                if self.lake_at(px, pz).is_some() {
                    continue;
                }
                let river = self.river(px, pz);
                if river.distance < river.width * 1.9 + 16.0 {
                    continue;
                }
                let site = self.node(
                    (px / SITE_SPACING).round() as i32,
                    (pz / SITE_SPACING).round() as i32,
                );
                if distance2(site, [px, pz]) < 150.0 * 150.0 {
                    continue;
                }
                let road = self.nearest_road(px, pz);
                if road.distance < 18.0 {
                    continue;
                }
                let h = self.ground(px, pz);
                let slope = (self.ground(px + 12.0, pz) - h)
                    .abs()
                    .max((self.ground(px, pz + 12.0) - h).abs())
                    / 12.0;
                if slope > 0.55 {
                    continue;
                }
                let k = ((id >> 8) as usize) % KINDS.len();
                result.push(Landmark {
                    id,
                    name: format!(
                        "{} {}",
                        NAMES[((id >> 12) as usize) % NAMES.len()],
                        TITLES[k]
                    ),
                    x: px,
                    z: pz,
                    kind: KINDS[k].to_owned(),
                });
            }
        }
        result
    }

    pub fn spawn(&self) -> [f32; 2] {
        self.spawn_view().0
    }

    /// Deterministically choose a real hillside road overlooking a short river
    /// crossing. No geometry or climate is overridden to manufacture this view.
    pub fn spawn_view(&self) -> ([f32; 2], f32) {
        if let Some(view) = *self.spawn_cache.borrow() {
            return view;
        }
        let roads = self.roads_near(0.0, 0.0, 20_000.0);
        let mut best: Option<([f32; 2], f32, f32)> = None;
        for road in &roads {
            for pair in road.windows(2) {
                let crossing = [
                    (pair[0][0] + pair[1][0]) * 0.5,
                    (pair[0][1] + pair[1][1]) * 0.5,
                ];
                let water = self.river(crossing[0], crossing[1]);
                if water.distance > water.width || water.width > 55.0 {
                    continue;
                }
                let bridge_len = distance2(pair[0], pair[1]).sqrt();
                if bridge_len > 450.0 || bridge_len < water.width * 2.0 {
                    continue;
                }
                for approach in self.roads_near(crossing[0], crossing[1], 900.0) {
                    for segment in approach.windows(2) {
                        for step in 0..8 {
                            let t = step as f32 / 8.0;
                            let p = [
                                lerp(segment[0][0], segment[1][0], t),
                                lerp(segment[0][1], segment[1][1], t),
                            ];
                            let dx = crossing[0] - p[0];
                            let dz = crossing[1] - p[1];
                            let distance = (dx * dx + dz * dz).sqrt();
                            if !(240.0..=850.0).contains(&distance) {
                                continue;
                            }
                            let dir = [dx / distance, dz / distance];
                            let tangent =
                                [segment[1][0] - segment[0][0], segment[1][1] - segment[0][1]];
                            let tangent_len =
                                (tangent[0] * tangent[0] + tangent[1] * tangent[1]).sqrt();
                            if ((tangent[0] * dir[0] + tangent[1] * dir[1]) / tangent_len).abs()
                                < 0.82
                            {
                                continue;
                            }
                            let s = self.sample(p[0], p[1]);
                            let rise = s.height - water.level;
                            if !(30.0..=120.0).contains(&rise) || s.road < 0.99 {
                                continue;
                            }
                            let mut obstructed = false;
                            for n in 1..16 {
                                let f = n as f32 / 16.0;
                                let ray = lerp(s.height + 1.72, water.level + 1.8, f);
                                if self.ground(p[0] + dx * f, p[1] + dz * f) > ray + 0.5 {
                                    obstructed = true;
                                    break;
                                }
                            }
                            if obstructed {
                                continue;
                            }
                            let opposite = [700.0, 1_500.0, 3_000.0, 5_000.0]
                                .into_iter()
                                .map(|d| {
                                    self.ground(crossing[0] + dir[0] * d, crossing[1] + dir[1] * d)
                                        - water.level
                                })
                                .fold(0.0, f32::max);
                            let biome_bonus = match s.biome {
                                Biome::Grassland => 22.0,
                                Biome::Forest => 18.0,
                                Biome::PineForest => 8.0,
                                Biome::Moor => 0.0,
                                _ => -30.0,
                            };
                            let trees = crate::ecology::tree_density(self.seed, p[0], p[1], &s);
                            let woodland_edge = (1.0 - (trees - 0.52).abs() / 0.52).clamp(0.0, 1.0);
                            let gx = self.ground(p[0] + 12., p[1]) - self.ground(p[0] - 12., p[1]);
                            let gz = self.ground(p[0], p[1] + 12.) - self.ground(p[0], p[1] - 12.);
                            let local_grade = gx.hypot(gz) / 24.;
                            let score = woodland_edge * 75.0 - local_grade.min(2.0) * 35.0
                                + biome_bonus
                                + rise.min(75.0) * 0.55
                                + opposite.min(700.0) * 0.09
                                - (distance - 430.0).abs() * 0.045
                                - distance2(p, [0.0, 0.0]).sqrt() * 0.00035;
                            if best.map_or(true, |(_, _, old)| score > old) {
                                best = Some((p, dx.atan2(-dz), score));
                            }
                        }
                    }
                }
            }
        }
        let view = best.map(|(p, yaw, _)| (p, yaw)).unwrap_or_else(|| {
            let road = roads
                .first()
                .expect("connected road network near world center");
            let a = road[0];
            let b = road[1];
            let p = [lerp(a[0], b[0], 0.35), lerp(a[1], b[1], 0.35)];
            (p, (b[0] - a[0]).atan2(-(b[1] - a[1])))
        });
        *self.spawn_cache.borrow_mut() = Some(view);
        view
    }

    pub fn roads_near(&self, cx: f32, cz: f32, radius: f32) -> Vec<Vec<[f32; 2]>> {
        self.road_routes_near(cx, cz, radius)
            .into_iter()
            .map(|r| r.points)
            .collect()
    }

    /// Top-down RGBA map: east is right; increasing world Z is down (south).
    pub fn map_rgba(&self, cx: f32, cz: f32, span: f32, res: u32) -> Vec<u8> {
        self.map_rgba_impl(cx, cz, span, res, true)
    }
    pub fn map_background_rgba(&self, cx: f32, cz: f32, span: f32, res: u32) -> Vec<u8> {
        self.map_rgba_impl(cx, cz, span, res, false)
    }
    fn map_rgba_impl(&self, cx: f32, cz: f32, span: f32, res: u32, draw_roads: bool) -> Vec<u8> {
        if res == 0 || !span.is_finite() || span <= 0.0 {
            return Vec::new();
        }
        let res = res.min(2048) as usize;
        let mut pixels = vec![0u8; res * res * 4];
        let mut heights = vec![0.0f32; res * res];
        let mpp = span / res as f32;
        for py in 0..res {
            let z = cz + (py as f32 + 0.5 - res as f32 * 0.5) * mpp;
            for px in 0..res {
                let x = cx + (px as f32 + 0.5 - res as f32 * 0.5) * mpp;
                let idx = py * res + px;
                let out = idx * 4;
                if x.abs() > HALF_WORLD || z.abs() > HALF_WORLD {
                    pixels[out..out + 4].copy_from_slice(&[14, 40, 58, 255]);
                    continue;
                }
                let s = self.natural_sample(x, z);
                heights[idx] = if s.ocean { 0.0 } else { s.height };
                let mut c = crate::ecology::ground_color(self.seed, x, z, &s);
                // Ensure rivers remain legible when narrower than a map pixel.
                let water = s.water_height > s.height;
                if s.ocean {
                    c = coast::ocean_color((SEA_LEVEL - s.height).max(0.));
                } else if water {
                    c = [0.24, 0.40, 0.47];
                } else {
                    match s.shore {
                        ShoreKind::Beach => c = [0.72, 0.65, 0.44],
                        ShoreKind::Cliff => c = [0.48, 0.47, 0.41],
                        ShoreKind::None => {}
                    }
                }
                let highland = smooth(420.0, 1000.0, s.height);
                for channel in 0..3 {
                    let v = lerp(c[channel], [0.75, 0.75, 0.69][channel], highland * 0.18);
                    pixels[out + channel] = (v * 255.0).clamp(0.0, 255.0) as u8;
                }
                pixels[out + 3] = 255;
            }
        }
        // Neighbor heights provide hill shading without extra terrain queries.
        for py in 1..res.saturating_sub(1) {
            for px in 1..res.saturating_sub(1) {
                let i = py * res + px;
                let gx = (heights[i + 1] - heights[i - 1]) / (2.0 * mpp);
                let gz = (heights[i + res] - heights[i - res]) / (2.0 * mpp);
                let shade = (0.98 - gx * 0.85 - gz * 0.55).clamp(0.64, 1.24);
                for k in 0..3 {
                    pixels[i * 4 + k] = (pixels[i * 4 + k] as f32 * shade).min(255.0) as u8;
                }
            }
        }
        if draw_roads {
            for road in self.road_map_routes(cx, cz, span) {
                let (color, alpha) = match road.kind {
                    RoadKind::Main => ([185, 151, 95], 0.78),
                    RoadKind::Lane => ([166, 145, 105], 0.65),
                    RoadKind::Trail => ([148, 135, 101], 0.55),
                };
                for pair in road.points.windows(2) {
                    let a = [
                        (pair[0][0] - cx) / mpp + res as f32 * 0.5,
                        (pair[0][1] - cz) / mpp + res as f32 * 0.5,
                    ];
                    let b = [
                        (pair[1][0] - cx) / mpp + res as f32 * 0.5,
                        (pair[1][1] - cz) / mpp + res as f32 * 0.5,
                    ];
                    draw_line(&mut pixels, res, a, b, color, alpha);
                }
            }
        }
        // Explicit network strokes preserve tributaries at continental zoom;
        // point sampling alone would miss channels narrower than one map pixel.
        let minimum_flow = if span > 120000.0 {
            65.0
        } else if span > 40000.0 {
            32.0
        } else {
            0.0
        };
        for segment in &self.hydrology.segments {
            if segment.flow < minimum_flow {
                continue;
            }
            let a = [
                (segment.a[0] - cx) / mpp + res as f32 * 0.5,
                (segment.a[1] - cz) / mpp + res as f32 * 0.5,
            ];
            let b = [
                (segment.b[0] - cx) / mpp + res as f32 * 0.5,
                (segment.b[1] - cz) / mpp + res as f32 * 0.5,
            ];
            draw_line(&mut pixels, res, a, b, [55, 115, 150], 0.94);
        }
        pixels
    }
}

fn draw_line(pixels: &mut [u8], res: usize, a: [f32; 2], b: [f32; 2], color: [u8; 3], alpha: f32) {
    if a[0].max(b[0]) < 0.0
        || a[0].min(b[0]) >= res as f32
        || a[1].max(b[1]) < 0.0
        || a[1].min(b[1]) >= res as f32
    {
        return;
    }
    let steps = ((b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil() as usize).clamp(1, res * 8);
    for n in 0..=steps {
        let t = n as f32 / steps as f32;
        let x = lerp(a[0], b[0], t).floor() as i32;
        let y = lerp(a[1], b[1], t).floor() as i32;
        if x < 0 || y < 0 || x >= res as i32 || y >= res as i32 {
            continue;
        }
        let idx = (y as usize * res + x as usize) * 4;
        for k in 0..3 {
            pixels[idx + k] = lerp(pixels[idx + k] as f32, color[k] as f32, alpha) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drainage_is_acyclic_downhill_and_conserves_accumulation() {
        let w = World::new(1337);
        let h = &w.hydrology;
        let mut rank = vec![0usize; h.order.len()];
        for (n, &id) in h.order.iter().enumerate() {
            rank[id as usize] = n;
        }
        let mut expected_rain = 0.0f64;
        let mut discharged_rain = 0.0f64;
        for i in 0..h.receiver.len() {
            if h.ocean[i] {
                assert_eq!(
                    h.receiver[i],
                    u32::MAX,
                    "ocean cell acquired a drainage receiver"
                );
                assert_eq!(h.conditioned[i], SEA_LEVEL);
                assert_eq!(h.water_level[i], SEA_LEVEL);
            } else {
                let x = -WORLD_SIZE * 0.5 + (i % hydrology::GRID) as f32 * hydrology::CELL;
                let z = -WORLD_SIZE * 0.5 + (i / hydrology::GRID) as f32 * hydrology::CELL;
                expected_rain += (0.24 + crate::regions::base(w.seed, x, z).rainfall * 1.52) as f64;
                assert!(
                    h.ocean[h.basin[i] as usize],
                    "inland catchment must terminate at an ocean outlet"
                );
            }
            let r = h.receiver[i];
            if r == u32::MAX {
                discharged_rain += h.accumulation[i] as f64;
                continue;
            }
            let r = r as usize;
            assert!(rank[r] < rank[i]);
            assert!(h.conditioned[r] <= h.conditioned[i]);
            assert!(h.water_level[r] <= h.water_level[i]);
            if h.water_level[i] > SEA_LEVEL
                && !h
                    .lakes
                    .iter()
                    .any(|l| (l.surface - h.water_level[i]).abs() < 0.0001)
            {
                assert!(
                    h.water_level[r] < h.water_level[i],
                    "non-lake, non-tidal reaches descend"
                );
            }
            assert!(h.accumulation[r] >= h.accumulation[i]);
            assert_eq!(h.basin[r], h.basin[i]);
        }
        assert!((expected_rain - discharged_rain).abs() < expected_rain * 0.00001,
            "land rainfall was lost or ocean rainfall was introduced: expected{expected_rain}, discharged{discharged_rain}");
        assert!(h.stats.confluences > 1000 && h.stats.headwaters > 1000 && h.stats.outlets > 20);
    }
    #[test]
    fn refined_channels_join_and_are_downhill() {
        let w = World::new(1337);
        let h = &w.hydrology;
        // Coast-clipped reaches have variable segment counts. Verify the actual
        // endpoint graph instead of accidentally joining unrelated groups of8.
        let mut nodes =
            std::collections::HashMap::<(u32, u32), (f32, usize, usize, [f32; 2])>::new();
        for s in &h.segments {
            assert!(s.level_a >= SEA_LEVEL && s.level_b >= SEA_LEVEL);
            assert!(s.level_b <= s.level_a + 0.0001);
            assert!(s.width_b >= s.width_a);
            for (point, level, incoming) in [(s.a, s.level_a, false), (s.b, s.level_b, true)] {
                let entry = nodes
                    .entry((point[0].to_bits(), point[1].to_bits()))
                    .or_insert((level, 0, 0, point));
                assert!(
                    (entry.0 - level).abs() < 0.0001,
                    "shared river endpoint has mismatched water levels at {point:?}: {} vs {level}",
                    entry.0
                );
                if incoming {
                    entry.1 += 1;
                } else {
                    entry.2 += 1;
                }
            }
        }
        let mut junctions = 0;
        let mut coastal_ends = 0;
        for &(level, incoming, outgoing, point) in nodes.values() {
            if incoming >= 2 && outgoing > 0 {
                junctions += 1;
            }
            if incoming > 0 && outgoing == 0 {
                let raw = raw_height(w.seed, point[0], point[1]);
                assert!(raw <= SEA_LEVEL + 0.05,
                    "river geometry ends inland instead of joining its receiver: {point:?}, raw {raw}");
                assert!(
                    (level - SEA_LEVEL).abs() < 0.001,
                    "coastal river endpoint does not meet sea level: {point:?}, level {level}"
                );
                coastal_ends += 1;
            }
        }
        assert!(junctions > 0 && coastal_ends > 20);
        // A raster junction at the sea may become separate mouths after clipping;
        // inland junctions must remain connected in the emitted geometry.
        assert!(junctions + h.stats.coastal_outlets >= h.stats.confluences);
        for s in h.segments.iter().step_by(23) {
            let p = [(s.a[0] + s.b[0]) * 0.5, (s.a[1] + s.b[1]) * 0.5];
            let raw = raw_height(w.seed, p[0], p[1]);
            assert!(raw >= SEA_LEVEL - 0.03, "river continued offshore at {p:?}");
            let sample = h.terrain(p[0], p[1], raw);
            assert!(sample.height < sample.water, "dry river {p:?}");
            assert!(
                h.lake_membership(p[0], p[1]).is_some()
                    || (sample.water - (s.level_a + s.level_b) * 0.5).abs() < 0.5,
                "inconsistent surface {p:?}"
            );
        }
    }
    #[test]
    fn deterministic_graph_seams_and_ecology() {
        let a = World::new(42);
        let b = World::new(42);
        assert_eq!(a.hydrology.receiver, b.hydrology.receiver);
        assert_eq!(a.hydrology.water_level, b.hydrology.water_level);
        for n in 0..400 {
            let x = (rand01(hash(17, n, 0)) - 0.5) * 250000.;
            let z = (rand01(hash(17, n, 1)) - 0.5) * 250000.;
            let s = a.sample(x, z);
            assert!(s.height.is_finite() && s.water_height.is_finite());
            assert_eq!(s.height.to_bits(), a.height(x, z).to_bits());
            assert_eq!(s.height.to_bits(), b.height(x, z).to_bits());
            assert_eq!(
                a.vegetation_density_from_sample(x, z, &s),
                crate::ecology::tree_density(a.seed, x, z, &s)
            );
        }
    }
    #[test]
    fn road_queries_and_spawn_are_consistent() {
        let w = World::new(1337);
        let (p, yaw) = w.spawn_view();
        assert!(yaw.is_finite());
        assert_eq!(w.spawn(), p);
        let s = w.sample(p[0], p[1]);
        assert!(s.road > 0.99 && s.height > s.water_height);
        let mut count = 0;
        for road in w.roads_near(p[0], p[1], 6000.) {
            for pair in road.windows(2) {
                for t in [0.0, 0.3, 0.7, 1.0] {
                    let x = lerp(pair[0][0], pair[1][0], t);
                    let z = lerp(pair[0][1], pair[1][1], t);
                    assert!(w.sample(x, z).road > 0.99, "missing road {},{}", x, z);
                    count += 1;
                }
            }
        }
        assert!(count > 500);
    }
    #[test]
    fn hydrology_diagnostics() {
        let w = World::new(1337);
        let h = &w.hydrology;
        let mut incision = Vec::new();
        let mut unbanked = 0;
        let mut worst_bank = 0.0f32;
        let mut count = 0;
        let mut max_step = 0.0f32;
        for s in h.segments.iter().step_by(11) {
            let p = [(s.a[0] + s.b[0]) * 0.5, (s.a[1] + s.b[1]) * 0.5];
            let water = (s.level_a + s.level_b) * 0.5;
            incision.push((raw_height(w.seed, p[0], p[1]) - water).max(0.0));
            let dx = s.b[0] - s.a[0];
            let dz = s.b[1] - s.a[1];
            let len = (dx * dx + dz * dz).sqrt();
            for side in [-1., 1.] {
                let distance = (s.width_a + s.width_b) * 0.5 * 2.05 + 8.;
                let q = [
                    p[0] + dz / len * distance * side,
                    p[1] - dx / len * distance * side,
                ];
                let sample = h.terrain(q[0], q[1], raw_height(w.seed, q[0], q[1]));
                let difference = water - sample.height;
                if difference > 3. {
                    unbanked += 1;
                }
                worst_bank = worst_bank.max(difference);
                count += 1;
            }
            let nearby = h.terrain(p[0] + 0.02, p[1], raw_height(w.seed, p[0] + 0.02, p[1]));
            let here = h.terrain(p[0] - 0.02, p[1], raw_height(w.seed, p[0] - 0.02, p[1]));
            max_step = max_step.max((nearby.height - here.height).abs());
        }
        incision.sort_by(|a, b| a.total_cmp(b));
        println!(
            "incision p50{} p95{} p99{} max{}; bank undercut {}of{} max{}; max4cmstep{}",
            incision[incision.len() / 2],
            incision[incision.len() * 95 / 100],
            incision[incision.len() * 99 / 100],
            incision[incision.len() - 1],
            unbanked,
            count,
            worst_bank,
            max_step
        );
    }
    #[test]
    fn water_beds_and_shorelines_remain_coherent() {
        for seed in [1337, 42, 2026] {
            let w = World::new(seed);
            let h = &w.hydrology;
            let mut crossings = 0;
            let mut wet_count = 0;
            let mut max_edge = 0.0f32;
            let mut max_uphill = 0.0f32;
            let mut max_step = 0.0f32;
            for segment in h.segments.iter().step_by(53) {
                let p = [
                    (segment.a[0] + segment.b[0]) * 0.5,
                    (segment.a[1] + segment.b[1]) * 0.5,
                ];
                let dx = segment.b[0] - segment.a[0];
                let dz = segment.b[1] - segment.a[1];
                let length = (dx * dx + dz * dz).sqrt();
                let width = (segment.width_a + segment.width_b) * 0.5;
                let a = h.terrain(
                    segment.a[0],
                    segment.a[1],
                    raw_height(seed, segment.a[0], segment.a[1]),
                );
                let b = h.terrain(
                    segment.b[0],
                    segment.b[1],
                    raw_height(seed, segment.b[0], segment.b[1]),
                );
                max_uphill = max_uphill.max(b.water - a.water);
                for side in [-1.0, 1.0] {
                    let mut previous = p;
                    let mut previous_sample = h.terrain(p[0], p[1], raw_height(seed, p[0], p[1]));
                    for n in 1..=28 {
                        let d = width * 2.6 * n as f32 / 28.0;
                        let q = [p[0] + dz / length * d * side, p[1] - dx / length * d * side];
                        let sample = h.terrain(q[0], q[1], raw_height(seed, q[0], q[1]));
                        if sample.water > sample.height {
                            assert!(
                                sample.water - sample.height
                                    <= if h.lake_membership(q[0], q[1]).is_some() {
                                        100.002
                                    } else {
                                        9.002
                                    },
                                "excessive depth seed{} {:?}:{}",
                                seed,
                                q,
                                sample.water - sample.height
                            );
                            wet_count += 1;
                        }
                        if previous_sample.water > previous_sample.height
                            && sample.water <= sample.height
                        {
                            let mut inside = previous;
                            let mut outside = q;
                            for _ in 0..12 {
                                let middle = [
                                    (inside[0] + outside[0]) * 0.5,
                                    (inside[1] + outside[1]) * 0.5,
                                ];
                                let m = h.terrain(
                                    middle[0],
                                    middle[1],
                                    raw_height(seed, middle[0], middle[1]),
                                );
                                if m.water > m.height {
                                    inside = middle;
                                } else {
                                    outside = middle;
                                }
                            }
                            let wet = h.terrain(
                                inside[0],
                                inside[1],
                                raw_height(seed, inside[0], inside[1]),
                            );
                            let dry = h.terrain(
                                outside[0],
                                outside[1],
                                raw_height(seed, outside[0], outside[1]),
                            );
                            let gap = (wet.water - dry.height).max(0.0);
                            max_edge = max_edge.max(gap);
                            assert!(
                                gap < 0.20,
                                "exposed water edge seed{} {:?} gap{}",
                                seed,
                                inside,
                                gap
                            );
                            crossings += 1;
                        }
                        let near =
                            h.terrain(q[0] + 0.03, q[1], raw_height(seed, q[0] + 0.03, q[1]));
                        max_step = max_step.max((sample.height - near.height).abs());
                        previous = q;
                        previous_sample = sample;
                    }
                }
            }
            println!(
                "seed{} shoreline checks{} wet beds{} maxedge{} max3cmstep{} maxqueryuphill{}",
                seed, crossings, wet_count, max_edge, max_step, max_uphill
            );
            assert!(crossings > 5000 && wet_count > 50000);
            assert!(
                max_uphill < 0.05,
                "interpolated surface deviates from downhill graph"
            );
        }
    }
}

// Append this module to world.rs; it exercises actual World terrain and channels.
#[cfg(test)]
mod terrain_character_regressions {
    use super::*;
    #[test]
    fn geological_faces_are_visible_at_walking_scale_and_axes_are_continuous() {
        let w = World::new(1337);
        let cases = [
            ([33800., 36280.], crate::regions::Formation::LayeredLedge),
            ([4660., -9600.], crate::regions::Formation::Ravine),
            ([14580., 39380.], crate::regions::Formation::ChalkScarp),
            ([32560., 87120.], crate::regions::Formation::BasaltBench),
        ];
        for (p, kind) in cases {
            let l = w.landscape(p[0], p[1]);
            assert_eq!(l.formation, kind);
            assert!(l.formation_strength > 0.60);
            let axis = l.formation_axis;
            assert!((axis[0].hypot(axis[1]) - 1.).abs() < 0.0001);
            let q = [p[0] - axis[1] * 110., p[1] + axis[0] * 110.];
            assert!(
                (w.height(q[0], q[1]) - w.height(p[0], p[1])).abs() > 35.,
                "unreadably flat {:?}",
                kind
            );
            let nearby = w.landscape(p[0] + 0.02, p[1]);
            assert!((nearby.formation_axis[0] - axis[0]).abs() < 0.002);
            assert!((w.height(p[0] + 0.02, p[1]) - w.height(p[0], p[1])).abs() < 0.15);
        }
    }
    #[test]
    fn inside_meander_bars_have_lower_shelves_than_opposing_cutbanks() {
        let w = World::new(1337);
        let mut considered = 0;
        let mut lower = 0;
        let mut difference = 0.;
        for s in w.hydrology.segments.iter().step_by(3) {
            if s.bend.abs() < 0.60 || s.deposition < 0.70 {
                continue;
            }
            let p = [(s.a[0] + s.b[0]) * 0.5, (s.a[1] + s.b[1]) * 0.5];
            if w.coast_info(p[0], p[1]).distance < 500.
                || w.hydrology.lake_membership(p[0], p[1]).is_some()
            {
                continue;
            }
            let delta = [s.b[0] - s.a[0], s.b[1] - s.a[1]];
            let length = delta[0].hypot(delta[1]);
            let side = s.bend.signum();
            let width = (s.width_a + s.width_b) * 0.5;
            let n = [
                -delta[1] / length * width * 1.55 * side,
                delta[0] / length * width * 1.55 * side,
            ];
            let inner = [p[0] + n[0], p[1] + n[1]];
            let outer = [p[0] - n[0], p[1] - n[1]];
            let a = w
                .hydrology
                .terrain(inner[0], inner[1], raw_height(w.seed, inner[0], inner[1]));
            let b = w
                .hydrology
                .terrain(outer[0], outer[1], raw_height(w.seed, outer[0], outer[1]));
            if (a.nearest.level - b.nearest.level).abs() > 0.15 {
                continue;
            }
            considered += 1;
            difference += b.height - a.height;
            if b.height > a.height + 0.30 {
                lower += 1;
            }
            if considered >= 120 {
                break;
            }
        }
        assert!(
            considered >= 35,
            "too few actual low-gradient bends: {considered}"
        );
        assert!(
            lower as f32 / considered as f32 > 0.65,
            "no consistent inside gravel shelf: {lower}/{considered}"
        );
        assert!(
            difference / considered as f32 > 0.50,
            "banks remain indistinguishable"
        );
        eprintln!(
            "bend bank checks {considered}, lower inner shelves {lower}, mean difference {}m",
            difference / considered as f32
        );
    }
}
