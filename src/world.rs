//! Deterministic regional geography. All coordinates and elevations are meters.
//!
//! This is an analytic landscape model, not an erosion or hydraulic simulation.
//! Shared spatial functions define river valleys and settlement connections before
//! terrain chunks are sampled, so loading order never changes their boundaries.

use serde::Serialize;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub const WORLD_SIZE: f32 = 256_000.0;
pub const SITE_SPACING: f32 = 2_400.0;
const HALF_WORLD: f32 = WORLD_SIZE * 0.5;
const RIVER_SPACING: f32 = 11_000.0;
const LANDMARK_SPACING: f32 = 640.0;
const NO_WATER: f32 = -10_000.0;

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
    road_cache: RefCell<HashMap<(i32, i32, bool), Rc<Vec<[f32; 2]>>>>,
    spawn_cache: RefCell<Option<([f32; 2], f32)>>,
}

#[derive(Clone, Copy)]
struct River {
    center: f32,
    distance: f32,
    width: f32,
    level: f32,
}

#[derive(Clone, Copy)]
struct RoadHit {
    distance: f32,
    point: [f32; 2],
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

fn fbm(seed: u32, x: f32, z: f32) -> f32 {
    noise(seed, x, z) * 0.58
        + noise(seed ^ 0x129a, x * 2.03 + 7.1, z * 2.03 - 5.7) * 0.28
        + noise(seed ^ 0xab73, x * 4.09 - 11.3, z * 4.09 + 8.2) * 0.14
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
    }
}

impl World {
    pub fn new(seed: u32) -> Self {
        Self {
            seed,
            road_cache: RefCell::new(HashMap::new()),
            spawn_cache: RefCell::new(None),
        }
    }

    fn river_center(&self, stripe: i32, z: f32) -> f32 {
        let base = stripe as f32 * RIVER_SPACING;
        let offset = (rand01(hash(self.seed ^ 0x7001, stripe, 0)) - 0.5) * 1_200.0;
        base + offset
            + (noise(self.seed ^ 0x7102, stripe as f32 * 3.73 + 0.31, z / 6_200.0) - 0.5) * 3_800.0
            + (noise(self.seed ^ 0x7203, stripe as f32 * 7.19 + 0.83, z / 1_500.0) - 0.5) * 400.0
    }

    fn river(&self, x: f32, z: f32) -> River {
        let index = (x / RIVER_SPACING).round() as i32;
        let mut best = River {
            center: 0.0,
            distance: f32::MAX,
            width: 20.0,
            level: 0.0,
        };
        for stripe in (index - 1)..=(index + 1) {
            let center = self.river_center(stripe, z);
            let distance = (x - center).abs();
            if distance < best.distance {
                // Every main channel has the same continuous downstream profile.
                // Broad wet reaches are widened sections of the same watercourse.
                let reach = noise(self.seed ^ 0x7320, stripe as f32 * 1.87 + 0.3, z / 3_000.0);
                let width = 15.0 + 11.0 * reach + 200.0 * smooth(0.79, 0.96, reach);
                best = River {
                    center,
                    distance,
                    width,
                    level: 45.0 + (z + HALF_WORLD) * 0.00075,
                };
            }
        }
        best
    }

    fn ground_with_river(&self, x: f32, z: f32, river: River) -> f32 {
        let warp_x = (noise(self.seed ^ 0x1801, x / 13_000.0, z / 13_000.0) - 0.5) * 2_300.0;
        let warp_z = (noise(self.seed ^ 0x1802, x / 13_000.0, z / 13_000.0) - 0.5) * 2_300.0;
        let wx = x + warp_x;
        let wz = z + warp_z;
        let province = fbm(self.seed ^ 0x1901, wx / 25_000.0, wz / 25_000.0);
        let mountains = smooth(0.38, 0.70, province);
        let ridge_noise = noise(self.seed ^ 0x1902, wx / 5_600.0, wz / 5_600.0);
        let ridge = (1.0 - (ridge_noise * 2.0 - 1.0).abs()).powi(3);
        let hill = fbm(self.seed ^ 0x1903, wx / 1_400.0, wz / 1_400.0);
        let rolling = fbm(self.seed ^ 0x1913, wx / 410.0, wz / 510.0);
        // A narrow floodplain transitions quickly into walkable foothills. The
        // previous kilometer-wide suppression made the first view look flat.
        let bank_end = river.width * 1.85;
        let local_fade = smooth(bank_end + 14.0, bank_end + 390.0, river.distance);
        let regional_fade = smooth(bank_end + 70.0, bank_end + 1_650.0, river.distance);
        let relief = (12.0 + 76.0 * rolling) * local_fade
            + (36.0 + 135.0 * hill + mountains * (170.0 + 1_420.0 * ridge)) * regional_fade;
        let detail = (noise(self.seed ^ 0x1904, x / 95.0, z / 95.0) - 0.5) * 6.0
            + (noise(self.seed ^ 0x1905, x / 30.0, z / 30.0) - 0.5) * 0.9;
        let land = river.level + 3.0 + relief + detail * local_fade;
        let bank = smooth(river.width * 0.68, bank_end, river.distance);
        lerp(river.level - 3.6, land, bank)
    }

    fn ground(&self, x: f32, z: f32) -> f32 {
        self.ground_with_river(x, z, self.river(x, z))
    }

    fn node(&self, i: i32, j: i32) -> [f32; 2] {
        let mut x =
            i as f32 * SITE_SPACING + (rand01(hash(self.seed ^ 0x2401, i, j)) - 0.5) * 700.0;
        let z = j as f32 * SITE_SPACING + (rand01(hash(self.seed ^ 0x2402, i, j)) - 0.5) * 700.0;
        // Only the nearest nominal stripe can be close enough to displace a
        // site: meanders remain well inside half the inter-river distance.
        let stripe = (x / RIVER_SPACING).round() as i32;
        let center = self.river_center(stripe, z);
        if (x - center).abs() < 380.0 {
            let reach = noise(self.seed ^ 0x7320, stripe as f32 * 1.87 + 0.3, z / 3_000.0);
            let width = 15.0 + 11.0 * reach + 200.0 * smooth(0.79, 0.96, reach);
            if (x - center).abs() < width + 140.0 {
                let sign = if x >= center { 1.0 } else { -1.0 };
                x = center + sign * (width + 165.0);
            }
        }
        [x, z]
    }

    fn edge_exists(&self, i: i32, j: i32, vertical: bool) -> bool {
        // Every east-west chain is continuous. North-south links are intentionally
        // incomplete, with a guaranteed link in each three-column block.
        !vertical || i.rem_euclid(3) == 0 || hash(self.seed ^ 0x2501, i, j) % 3 != 0
    }

    fn road_points(&self, i: i32, j: i32, vertical: bool) -> Rc<Vec<[f32; 2]>> {
        let key = (i, j, vertical);
        if let Some(points) = self.road_cache.borrow().get(&key) {
            return points.clone();
        }
        let a = self.node(i, j);
        let b = self.node(i + i32::from(!vertical), j + i32::from(vertical));
        let points = Rc::new(self.plan_road(i, j, vertical, a, b));
        self.road_cache.borrow_mut().insert(key, points.clone());
        points
    }

    fn plan_road(&self, i: i32, j: i32, vertical: bool, a: [f32; 2], b: [f32; 2]) -> Vec<[f32; 2]> {
        let ra = self.river(a[0], a[1]);
        let rb = self.river(b[0], b[1]);
        let sa = a[0] - ra.center;
        let sb = b[0] - rb.center;
        if sa * sb < 0.0 && (ra.center - rb.center).abs() < 4_500.0 {
            let stripe = ((ra.center + rb.center) * 0.5 / RIVER_SPACING).round() as i32;
            // Choose a short crossing near the direct connection. Width is
            // evaluated explicitly so broad reaches favor nearby narrows.
            let mut chosen = (a[1] + b[1]) * 0.5;
            let mut best = f32::MAX;
            for n in -5..=5 {
                let z = (a[1] + b[1]) * 0.5 + n as f32 * 100.0;
                let x = self.river_center(stripe, z);
                let r = self.river(x, z);
                let detour = distance2(a, [x, z]).sqrt() + distance2(b, [x, z]).sqrt();
                let score = detour + r.width * 14.0;
                if score < best {
                    best = score;
                    chosen = z;
                }
            }
            let center = [self.river_center(stripe, chosen), chosen];
            let r = self.river(center[0], center[1]);
            let derivative = (self.river_center(stripe, chosen + 8.0)
                - self.river_center(stripe, chosen - 8.0))
                / 16.0;
            let inv = (1.0 + derivative * derivative).sqrt().recip();
            let normal = [inv, -derivative * inv];
            let half_span = r.width * 2.15 + 16.0;
            let sign = sa.signum();
            let bank_a = [
                center[0] + normal[0] * half_span * sign,
                center[1] + normal[1] * half_span * sign,
            ];
            let bank_b = [
                center[0] - normal[0] * half_span * sign,
                center[1] - normal[1] * half_span * sign,
            ];
            let mut points = Vec::with_capacity(12);
            self.bank_approach(&mut points, stripe, a, bank_a, sign, true);
            points.push(bank_b);
            self.bank_approach(&mut points, stripe, bank_b, b, -sign, false);
            return points;
        }
        if sa * sb > 0.0
            && (ra.center - rb.center).abs() < 4_500.0
            && ra.distance.min(rb.distance) < 650.0
        {
            let stripe = ((ra.center + rb.center) * 0.5 / RIVER_SPACING).round() as i32;
            let mut points = Vec::with_capacity(5);
            self.bank_approach(&mut points, stripe, a, b, sa.signum(), true);
            return points;
        }
        let dx = b[0] - a[0];
        let dz = b[1] - a[1];
        let len = (dx * dx + dz * dz).sqrt();
        let side = [-dz / len, dx / len];
        let bend = (rand01(hash(
            self.seed ^ if vertical { 0x2601 } else { 0x2602 },
            i,
            j,
        )) - 0.5)
            * 400.0;
        let mut points = Vec::with_capacity(9);
        points.push(a);
        let mut previous_height = self.ground(a[0], a[1]);
        let end_height = self.ground(b[0], b[1]);
        let mut previous_offset = 0.0;
        for n in 1..8 {
            let t = n as f32 / 8.0;
            let arc = 4.0 * t * (1.0 - t);
            let base = [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
            let mut chosen = base;
            let mut score_best = f32::MAX;
            let mut chosen_h = 0.0;
            let mut chosen_offset = 0.0;
            // A bounded contour preference moves trails around local high spots;
            // this is intentionally not advertised as full least-cost routing.
            for offset in [-230.0, -115.0, 0.0, 115.0, 230.0] {
                let lateral = (bend + offset) * arc;
                let p = [base[0] + side[0] * lateral, base[1] + side[1] * lateral];
                let r = self.river(p[0], p[1]);
                if r.distance < r.width * 2.0 + 12.0 {
                    continue;
                }
                let h = self.ground_with_river(p[0], p[1], r);
                let grade = (h - previous_height).abs() / (len / 8.0);
                let score = (h - previous_height).abs() * 0.75
                    + (h - end_height).abs() * 0.11
                    + offset.abs() * 0.13
                    + (lateral - previous_offset).abs() * 0.23
                    + ((grade - 0.24).max(0.0)).powi(2) * 1_400.0;
                if score < score_best {
                    score_best = score;
                    chosen = p;
                    chosen_h = h;
                    chosen_offset = lateral;
                }
            }
            if score_best == f32::MAX {
                chosen_h = self.ground(chosen[0], chosen[1]);
            }
            points.push(chosen);
            previous_height = chosen_h;
            previous_offset = chosen_offset;
        }
        points.push(b);
        points
    }

    fn bank_approach(
        &self,
        out: &mut Vec<[f32; 2]>,
        stripe: i32,
        a: [f32; 2],
        b: [f32; 2],
        sign: f32,
        include_first: bool,
    ) {
        let oa = (a[0] - self.river_center(stripe, a[1])).abs();
        let ob = (b[0] - self.river_center(stripe, b[1])).abs();
        let length = distance2(a, b).sqrt();
        let crosswise = (oa - ob).abs() > length * 0.60;
        let bends = if length > 450.0 && crosswise {
            [-420.0, -280.0, -160.0, 160.0, 280.0, 420.0]
        } else {
            [0.0; 6]
        };
        let mut best_score = f32::INFINITY;
        let mut chosen = Vec::new();
        for bend in bends {
            let mut route = Vec::with_capacity(9);
            let mut prev_h = self.ground(a[0], a[1]);
            let mut worst_grade = 0.0f32;
            let mut ascent = 0.0;
            let mut route_len = 0.0;
            for n in 0..=8 {
                let t = n as f32 / 8.0;
                // An S bend has a straight tangent at both ends, keeping the
                // approach aligned with the bridge before following the slope.
                let shape = (std::f32::consts::TAU * t).sin() * (std::f32::consts::PI * t).sin();
                let z = lerp(a[1], b[1], t) + bend * shape;
                let center = self.river_center(stripe, z);
                let r = self.river(center, z);
                let offset = lerp(oa, ob, t).max(r.width * 2.15 + 14.0);
                let p = if n == 0 {
                    a
                } else if n == 8 {
                    b
                } else {
                    [center + sign * offset, z]
                };
                let h = self.ground(p[0], p[1]);
                if let Some(previous) = route.last() {
                    let d = distance2(*previous, p).sqrt();
                    route_len += d;
                    worst_grade = worst_grade.max((h - prev_h).abs() / d);
                    ascent += (h - prev_h).abs();
                }
                route.push(p);
                prev_h = h;
            }
            let score = worst_grade * 700.0 + route_len * 0.20 + ascent * 0.12;
            if score < best_score {
                best_score = score;
                chosen = route;
            }
            if !crosswise || length <= 450.0 {
                break;
            }
        }
        out.extend(chosen.into_iter().skip(usize::from(!include_first)));
    }

    fn nearest_road(&self, x: f32, z: f32) -> RoadHit {
        let ix = (x / SITE_SPACING).floor() as i32;
        let iz = (z / SITE_SPACING).floor() as i32;
        let p = [x, z];
        let mut best = RoadHit {
            distance: f32::MAX,
            point: p,
        };
        // River-following approaches can leave their nominal site cell. A
        // conservative spatial envelope keeps querying/map/meshes in agreement.
        for i in ix - 2..=ix + 2 {
            for j in iz - 1..=iz + 1 {
                for vertical in [false, true] {
                    let ax = i as f32 * SITE_SPACING;
                    let az = j as f32 * SITE_SPACING;
                    let (bx, bz, padx, padz) = if vertical {
                        (ax, az + SITE_SPACING, 3_200.0, 950.0)
                    } else {
                        (ax + SITE_SPACING, az, 950.0, 1_050.0)
                    };
                    if x < ax.min(bx) - padx
                        || x > ax.max(bx) + padx
                        || z < az.min(bz) - padz
                        || z > az.max(bz) + padz
                    {
                        continue;
                    }
                    if !self.edge_exists(i, j, vertical) {
                        continue;
                    }
                    let points = self.road_points(i, j, vertical);
                    for pair in points.windows(2) {
                        if x < pair[0][0].min(pair[1][0]) - 8.0
                            || x > pair[0][0].max(pair[1][0]) + 8.0
                            || z < pair[0][1].min(pair[1][1]) - 8.0
                            || z > pair[0][1].max(pair[1][1]) + 8.0
                        {
                            continue;
                        }
                        let hit = segment_hit(p, pair[0], pair[1]);
                        if hit.distance < best.distance {
                            best = hit;
                        }
                    }
                }
            }
        }
        best
    }

    pub fn sample(&self, x: f32, z: f32) -> Sample {
        let x = x.clamp(-HALF_WORLD, HALF_WORLD);
        let z = z.clamp(-HALF_WORLD, HALF_WORLD);
        let river = self.river(x, z);
        let mut height = self.ground_with_river(x, z, river);
        let road_hit = self.nearest_road(x, z);
        let road = 1.0 - smooth(3.1, 6.4, road_hit.distance);
        if road > 0.0 && river.distance > river.width * 1.9 {
            let center_height = self.ground(road_hit.point[0], road_hit.point[1]);
            height = lerp(height, center_height + 0.015, road * 0.92);
        }
        // Temperature has a continental-scale north/south gradient; moisture
        // combines broad weather provinces with a local riparian contribution.
        let continental_heat = noise(self.seed ^ 0x3101, x / 47_000.0, z / 47_000.0);
        let temperature = (0.49 + (continental_heat - 0.5) * 0.52 + z / HALF_WORLD * 0.23
            - (height - river.level) * 0.00056)
            .clamp(0.0, 1.0);
        let province_moisture = fbm(self.seed ^ 0x3102, x / 29_000.0, z / 29_000.0);
        let wet_edge = 1.0 - smooth(river.width * 1.6, 480.0 + river.width, river.distance);
        let moisture = ((province_moisture - 0.5) * 1.55 + 0.49 + wet_edge * 0.24).clamp(0.0, 1.0);
        let cover = noise(self.seed ^ 0x3103, x / 720.0, z / 720.0);
        let relief = height - river.level;
        let biome = if relief > 490.0 || (temperature < 0.19 && relief > 180.0) {
            Biome::Alpine
        } else if river.distance < river.width + 85.0 && moisture > 0.48 {
            Biome::Wetland
        } else if temperature > 0.58 && moisture < 0.38 {
            Biome::Desert
        } else if temperature < 0.38 || relief > 230.0 {
            if moisture > 0.35 && cover > 0.31 && relief < 420.0 {
                Biome::PineForest
            } else {
                Biome::Moor
            }
        } else if moisture > 0.48 && cover > 0.37 {
            Biome::Forest
        } else {
            Biome::Grassland
        };
        let water_height = if river.distance < river.width * 1.85 {
            river.level
        } else {
            NO_WATER
        };
        Sample {
            height,
            biome,
            road,
            river: 1.0 - smooth(river.width, river.width * 1.9, river.distance),
            water_height,
            temperature,
            moisture,
        }
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        // Match sample().height exactly; collisions and visible terrain agree.
        let x = x.clamp(-HALF_WORLD, HALF_WORLD);
        let z = z.clamp(-HALF_WORLD, HALF_WORLD);
        let river = self.river(x, z);
        let height = self.ground_with_river(x, z, river);
        let road_hit = self.nearest_road(x, z);
        let road = 1.0 - smooth(3.1, 6.4, road_hit.distance);
        if road > 0.0 && river.distance > river.width * 1.9 {
            lerp(
                height,
                self.ground(road_hit.point[0], road_hit.point[1]) + 0.015,
                road * 0.92,
            )
        } else {
            height
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
            kind: if (i + 2 * j).rem_euclid(5) == 0 {
                "town"
            } else if id % 3 == 0 {
                "hamlet"
            } else {
                "village"
            }
            .to_owned(),
        }
    }

    pub fn sites_near(&self, x: f32, z: f32, radius: f32) -> Vec<Site> {
        if !radius.is_finite() || radius < 0.0 {
            return Vec::new();
        }
        let radius = radius.min(WORLD_SIZE * 1.5);
        let min_i = (((x - radius) / SITE_SPACING).floor() as i32 - 1).max(-54);
        let max_i = (((x + radius) / SITE_SPACING).ceil() as i32 + 1).min(54);
        let min_j = (((z - radius) / SITE_SPACING).floor() as i32 - 1).max(-54);
        let max_j = (((z + radius) / SITE_SPACING).ceil() as i32 + 1).min(54);
        let mut result = Vec::new();
        for i in min_i..=max_i {
            for j in min_j..=max_j {
                let p = self.node(i, j);
                if p[0].abs() <= HALF_WORLD
                    && p[1].abs() <= HALF_WORLD
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
        let min_i = (((x - radius) / LANDMARK_SPACING).floor() as i32 - 1).max(-201);
        let max_i = (((x + radius) / LANDMARK_SPACING).ceil() as i32 + 1).min(201);
        let min_j = (((z - radius) / LANDMARK_SPACING).floor() as i32 - 1).max(-201);
        let max_j = (((z + radius) / LANDMARK_SPACING).ceil() as i32 + 1).min(201);
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
                let h = self.ground_with_river(px, pz, river);
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
                if bridge_len > 320.0 {
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
                            let score =
                                biome_bonus + rise.min(75.0) * 0.55 + opposite.min(700.0) * 0.09
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
            let road = self.road_points(0, 0, false);
            let a = road[0];
            let b = road[1];
            let p = [lerp(a[0], b[0], 0.35), lerp(a[1], b[1], 0.35)];
            (p, (b[0] - a[0]).atan2(-(b[1] - a[1])))
        });
        *self.spawn_cache.borrow_mut() = Some(view);
        view
    }

    pub fn roads_near(&self, cx: f32, cz: f32, radius: f32) -> Vec<Vec<[f32; 2]>> {
        if !radius.is_finite() || radius < 0.0 {
            return Vec::new();
        }
        let radius = radius.min(WORLD_SIZE * 1.5);
        let min_i = (((cx - radius) / SITE_SPACING).floor() as i32 - 2).max(-54);
        let max_i = (((cx + radius) / SITE_SPACING).ceil() as i32 + 2).min(54);
        let min_j = (((cz - radius) / SITE_SPACING).floor() as i32 - 1).max(-54);
        let max_j = (((cz + radius) / SITE_SPACING).ceil() as i32 + 1).min(54);
        let mut roads = Vec::new();
        for i in min_i..=max_i {
            for j in min_j..=max_j {
                for vertical in [false, true] {
                    if !self.edge_exists(i, j, vertical) {
                        continue;
                    }
                    let points = self.road_points(i, j, vertical);
                    if points[0][0].abs() > HALF_WORLD
                        || points[0][1].abs() > HALF_WORLD
                        || points[points.len() - 1][0].abs() > HALF_WORLD
                        || points[points.len() - 1][1].abs() > HALF_WORLD
                    {
                        continue;
                    }
                    if points
                        .windows(2)
                        .any(|p| segment_hit([cx, cz], p[0], p[1]).distance <= radius + 8.0)
                    {
                        roads.push(points.as_ref().clone());
                    }
                }
            }
        }
        roads
    }

    /// Top-down RGBA map: east is right; increasing world Z is down (south).
    pub fn map_rgba(&self, cx: f32, cz: f32, span: f32, res: u32) -> Vec<u8> {
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
                    pixels[out..out + 4].copy_from_slice(&[20, 28, 29, 255]);
                    continue;
                }
                let s = self.sample(x, z);
                heights[idx] = s.height;
                let mut c = s.biome.color();
                // Ensure rivers remain legible when narrower than a map pixel.
                let river = self.river(x, z);
                let water = s.water_height > s.height || river.distance < river.width + mpp * 0.43;
                if water {
                    c = [0.24, 0.40, 0.47];
                }
                if span < 20_000.0 && s.road > 0.1 && !water {
                    c = [0.67, 0.54, 0.33];
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
        // At continental zoom only selected trunk routes are drawn, avoiding a
        // dense lattice of minor trails obscuring climate and mountain regions.
        let radius = span * 0.72;
        let min_i = (((cx - radius) / SITE_SPACING).floor() as i32 - 2).max(-54);
        let max_i = (((cx + radius) / SITE_SPACING).ceil() as i32 + 2).min(54);
        let min_j = (((cz - radius) / SITE_SPACING).floor() as i32 - 1).max(-54);
        let max_j = (((cz + radius) / SITE_SPACING).ceil() as i32 + 1).min(54);
        for i in min_i..=max_i {
            for j in min_j..=max_j {
                for vertical in [false, true] {
                    if !self.edge_exists(i, j, vertical) {
                        continue;
                    }
                    if span > 40_000.0
                        && if vertical {
                            i.rem_euclid(6) != 0
                        } else {
                            j.rem_euclid(6) != 0
                        }
                    {
                        continue;
                    }
                    let points = self.road_points(i, j, vertical);
                    for pair in points.windows(2) {
                        let a = [
                            (pair[0][0] - cx) / mpp + res as f32 * 0.5,
                            (pair[0][1] - cz) / mpp + res as f32 * 0.5,
                        ];
                        let b = [
                            (pair[1][0] - cx) / mpp + res as f32 * 0.5,
                            (pair[1][1] - cz) / mpp + res as f32 * 0.5,
                        ];
                        draw_line(
                            &mut pixels,
                            res,
                            a,
                            b,
                            [185, 151, 95],
                            if span > 40_000.0 { 0.35 } else { 0.73 },
                        );
                    }
                }
            }
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
    fn deterministic_seed_and_chunk_edges() {
        let a = World::new(71);
        let b = World::new(71);
        let c = World::new(72);
        let mut changed = 0;
        for x in -8..8 {
            for z in -8..8 {
                let p = [x as f32 * 128.0, z as f32 * 128.0];
                let s = a.sample(p[0], p[1]);
                assert_eq!(s.height.to_bits(), b.sample(p[0], p[1]).height.to_bits());
                assert_eq!(s.height.to_bits(), a.height(p[0], p[1]).to_bits());
                // The same chunk boundary coordinate reached from either side.
                let edge_from_left = (x - 1) as f32 * 128.0 + 128.0;
                assert_eq!(
                    s.height.to_bits(),
                    a.sample(edge_from_left, p[1]).height.to_bits()
                );
                changed += usize::from((s.height - c.height(p[0], p[1])).abs() > 0.01);
            }
        }
        assert!(changed > 240);
    }

    #[test]
    fn finite_world_and_biome_variety() {
        let w = World::new(4281);
        let mut counts = [0usize; 7];
        let mut min_h = f32::MAX;
        let mut max_h = f32::MIN;
        for i in 0..96 {
            for j in 0..96 {
                let x = -HALF_WORLD + (i as f32 + 0.31) * WORLD_SIZE / 96.0;
                let z = -HALF_WORLD + (j as f32 + 0.67) * WORLD_SIZE / 96.0;
                let s = w.sample(x, z);
                assert!(s.height.is_finite() && s.water_height.is_finite());
                assert!((0.0..=1.0).contains(&s.temperature));
                assert!((0.0..=1.0).contains(&s.moisture));
                assert!((0.0..=1.0).contains(&s.road));
                counts[s.biome as usize] += 1;
                min_h = min_h.min(s.height);
                max_h = max_h.max(s.height);
            }
        }
        assert!(
            counts.iter().all(|c| *c > 0),
            "biome coverage: {:?}",
            counts
        );
        assert!(max_h - min_h > 500.0, "terrain range {}..{}", min_h, max_h);
    }

    #[test]
    fn roads_connect_sites_and_match_sampler() {
        let w = World::new(4281);
        for i in -3..3 {
            for j in -3..3 {
                let a = w.node(i, j);
                let b = w.node(i + 1, j);
                let distance = distance2(a, b).sqrt();
                assert!((1_500.0..3_400.0).contains(&distance));
                let points = w.road_points(i, j, false);
                assert_eq!(points[0], a);
                assert_eq!(*points.last().unwrap(), b);
                for pair in points.windows(2) {
                    for t in [0.0, 0.37, 0.73, 1.0] {
                        let x = lerp(pair[0][0], pair[1][0], t);
                        let z = lerp(pair[0][1], pair[1][1], t);
                        assert!(w.sample(x, z).road > 0.99, "missing road at {},{}", x, z);
                    }
                }
            }
        }
        let p = w.spawn();
        assert!(w.sample(p[0], p[1]).road > 0.99);
        assert!(w.sample(p[0], p[1]).height > w.sample(p[0], p[1]).water_height);
    }

    #[test]
    fn continuous_downstream_water_and_dry_sites() {
        let w = World::new(4281);
        for stripe in -8..=8 {
            let mut previous = f32::MIN;
            for j in -100..100 {
                let z = j as f32 * 1000.0;
                let x = w.river_center(stripe, z);
                let s = w.sample(x, z);
                assert!(s.water_height > s.height);
                assert!(s.water_height > previous);
                assert!(s.river > 0.99);
                previous = s.water_height;
                let nearby = w.sample(w.river_center(stripe, z + 0.1), z + 0.1);
                assert!((nearby.water_height - s.water_height).abs() < 0.001);
            }
        }
        for site in w.sites_near(0.0, 0.0, 30_000.0) {
            let s = w.sample(site.x, site.z);
            assert!(s.height > s.water_height, "submerged site {}", site.name);
        }
    }

    #[test]
    fn maps_and_spatial_queries_are_repeatable() {
        let w = World::new(4281);
        let a = w.map_rgba(0.0, 0.0, 5_000.0, 64);
        assert_eq!(a.len(), 64 * 64 * 4);
        assert_eq!(a, w.map_rgba(0.0, 0.0, 5_000.0, 64));
        assert!(a.chunks_exact(4).all(|p| p[3] == 255));
        let sites = w.sites_near(0.0, 0.0, 7_500.0);
        assert!((20..40).contains(&sites.len()));
        let landmarks = w.landmarks_near(0.0, 0.0, 2_000.0);
        assert!(
            (12..40).contains(&landmarks.len()),
            "{} nearby landmarks",
            landmarks.len()
        );
        for l in landmarks {
            assert!(distance2([l.x, l.z], [0.0, 0.0]) <= 2_000.0f32.powi(2));
            assert!(w.sample(l.x, l.z).height > w.sample(l.x, l.z).water_height);
        }
        assert!(w.roads_near(0.0, 0.0, 5_000.0).len() > 10);
    }

    #[test]
    fn procedural_spawn_is_dry_on_a_road_for_multiple_seeds() {
        for seed in [0, 1, 71, 1337, 4281, 98765] {
            let w = World::new(seed);
            let p = w.spawn();
            let s = w.sample(p[0], p[1]);
            assert!(s.road > 0.99, "spawn off road for seed {}", seed);
            assert!(s.height > s.water_height, "wet spawn for seed {}", seed);
            eprintln!(
                "seed {} spawn {:?}: {:?}, height {}",
                seed, p, s.biome, s.height
            );
        }
    }

    #[test]
    fn river_crossings_are_short_and_near_perpendicular() {
        for seed in [1337, 42, 2026] {
            let w = World::new(seed);
            let mut crossings = 0;
            for road in w.roads_near(0.0, 0.0, 20_000.0) {
                for p in road.windows(2) {
                    let dx = p[1][0] - p[0][0];
                    let dz = p[1][1] - p[0][1];
                    let length = (dx * dx + dz * dz).sqrt();
                    for n in 1..8 {
                        let t = n as f32 / 8.0;
                        let x = lerp(p[0][0], p[1][0], t);
                        let z = lerp(p[0][1], p[1][1], t);
                        let r = w.river(x, z);
                        if w.ground_with_river(x, z, r) >= r.level {
                            continue;
                        }
                        crossings += 1;
                        let stripe = (r.center / RIVER_SPACING).round() as i32;
                        let slope = (w.river_center(stripe, z + 5.0)
                            - w.river_center(stripe, z - 5.0))
                            / 10.0;
                        let alignment =
                            ((dx * slope + dz) / (length * (1.0 + slope * slope).sqrt())).abs();
                        assert!(
                            alignment < 0.38,
                            "oblique crossing seed{} at{},{}: alignment{}",
                            seed,
                            x,
                            z,
                            alignment
                        );
                        assert!(
                            length < r.width * 6.0 + 80.0,
                            "excessive bridge length {}",
                            length
                        );
                        assert!(w.sample(x, z).road > 0.99);
                    }
                }
            }
            assert!(crossings > 40);
        }
    }

    #[test]
    fn first_view_has_immediate_relief_and_distant_mountains() {
        for seed in [1337, 42, 2026] {
            let w = World::new(seed);
            let (p, yaw) = w.spawn_view();
            let initial = w.height(p[0], p[1]);
            let down = w.height(p[0] + yaw.sin() * 400.0, p[1] - yaw.cos() * 400.0);
            let ridge = [1_600.0, 3_200.0, 5_000.0]
                .into_iter()
                .map(|d| w.height(p[0] + yaw.sin() * d, p[1] - yaw.cos() * d))
                .fold(0.0, f32::max);
            assert!(initial - down > 40.0, "spawn needs visible downhill relief");
            assert!(
                ridge - initial > 300.0,
                "spawn needs a real mountain horizon"
            );
            assert_eq!(w.spawn(), p);
            assert_eq!(w.spawn_view(), (p, yaw));
        }
    }

    #[test]
    #[ignore = "manual release-mode throughput measurement"]
    fn sampler_throughput() {
        let w = World::new(4281);
        let now = std::time::Instant::now();
        let mut sum = 0.0;
        for n in 0..50_000 {
            let x = (n % 250) as f32 * 8.0 - 1_000.0;
            let z = (n / 250) as f32 * 8.0 - 800.0;
            sum += std::hint::black_box(w.sample(x, z)).height;
        }
        eprintln!("50,000 samples: {:?}; checksum {}", now.elapsed(), sum);
    }
}
