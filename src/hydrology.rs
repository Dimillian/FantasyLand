//! Terrain-derived drainage. Priority-Flood finds spill routes; D8 receivers
//! accumulate runoff. River profiles breach depressions and stay below terrain.
//! See Barnes, Lehman & Mulla (2014), doi:10.1016/j.cageo.2013.04.024.
use super::{distance2, hash, lerp, noise, rand01, raw_height, smooth, WORLD_SIZE};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashSet},
};

pub const GRID: usize = 513;
pub const CELL: f32 = WORLD_SIZE / (GRID - 1) as f32;
const HALF: f32 = WORLD_SIZE * 0.5;
const BUCKET: f32 = 1000.0;
const BUCKETS: usize = 257;
const CHANNEL_THRESHOLD: f32 = 24.0;
const NONE: u32 = u32::MAX;
const EPS: f32 = 0.003;
const SUBDIV: usize = 8;
pub const NO_WATER: f32 = -10000.0;

#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub level_a: f32,
    pub level_b: f32,
    pub width_a: f32,
    pub width_b: f32,
    pub flow: f32,
    pub influence: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub point: [f32; 2],
    pub tangent: [f32; 2],
    pub distance: f32,
    pub width: f32,
    pub level: f32,
    pub segment: u32,
}
impl Hit {
    pub fn none() -> Self {
        Self {
            point: [0.0; 2],
            tangent: [0.0, 1.0],
            distance: f32::MAX,
            width: 0.0,
            level: NO_WATER,
            segment: NONE,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TerrainWater {
    pub height: f32,
    pub water: f32,
    pub river: f32,
    pub nearest: Hit,
}
#[derive(Clone, Copy, Debug)]
pub struct Crossing {
    pub hit: Hit,
    pub t: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    pub grid_resolution: usize,
    pub grid_spacing_m: f32,
    pub drainage_cells: usize,
    pub river_segments: usize,
    pub headwaters: usize,
    pub confluences: usize,
    pub outlets: usize,
    /// Actual contributing cell area; channel widths use rainfall-weighted runoff.
    pub max_catchment_km2: f32,
    pub max_incision_m: f32,
    pub mean_incision_m: f32,
    pub index_references: usize,
}
#[derive(Clone, Debug)]
pub struct Hydrology {
    pub segments: Vec<Segment>,
    buckets: Vec<Vec<u32>>,
    pub stats: Stats,
    // Audit data is dropped after generation in browser/native builds.
    #[cfg(test)]
    pub conditioned: Vec<f32>,
    #[cfg(test)]
    pub receiver: Vec<u32>,
    #[cfg(test)]
    pub accumulation: Vec<f32>,
    #[cfg(test)]
    pub water_level: Vec<f32>,
    #[cfg(test)]
    pub order: Vec<u32>,
    #[cfg(test)]
    pub basin: Vec<u32>,
}
#[derive(Clone, Copy)]
struct FloodNode {
    height: f32,
    index: u32,
}
impl PartialEq for FloodNode {
    fn eq(&self, o: &Self) -> bool {
        self.height == o.height && self.index == o.index
    }
}
impl Eq for FloodNode {}
impl PartialOrd for FloodNode {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for FloodNode {
    fn cmp(&self, o: &Self) -> Ordering {
        o.height
            .total_cmp(&self.height)
            .then_with(|| o.index.cmp(&self.index))
    }
}
fn node_position(index: usize) -> [f32; 2] {
    [
        -HALF + (index % GRID) as f32 * CELL,
        -HALF + (index / GRID) as f32 * CELL,
    ]
}
fn neighbors(index: usize) -> impl Iterator<Item = (usize, f32)> {
    let x = (index % GRID) as i32;
    let z = (index / GRID) as i32;
    (-1i32..=1).flat_map(move |dz| {
        (-1i32..=1).filter_map(move |dx| {
            let nx = x + dx;
            let nz = z + dz;
            if dx == 0 && dz == 0 || nx < 0 || nz < 0 || nx >= GRID as i32 || nz >= GRID as i32 {
                None
            } else {
                Some((
                    nz as usize * GRID + nx as usize,
                    if dx != 0 && dz != 0 {
                        CELL * std::f32::consts::SQRT_2
                    } else {
                        CELL
                    },
                ))
            }
        })
    })
}
fn normalize(v: [f32; 2]) -> [f32; 2] {
    let inv = (v[0] * v[0] + v[1] * v[1]).sqrt().max(0.001).recip();
    [v[0] * inv, v[1] * inv]
}
fn width(flow: f32) -> f32 {
    (2.5 + 1.5 * (flow * 0.25).sqrt()).min(90.0)
}
fn bezier(a: [f32; 2], b: [f32; 2], ta: [f32; 2], tb: [f32; 2], t: f32) -> [f32; 2] {
    let length = distance2(a, b).sqrt();
    let c1 = [a[0] + ta[0] * length * 0.30, a[1] + ta[1] * length * 0.30];
    let c2 = [b[0] - tb[0] * length * 0.30, b[1] - tb[1] * length * 0.30];
    let u = 1.0 - t;
    [
        u * u * u * a[0] + 3.0 * u * u * t * c1[0] + 3.0 * u * t * t * c2[0] + t * t * t * b[0],
        u * u * u * a[1] + 3.0 * u * u * t * c1[1] + 3.0 * u * t * t * c2[1] + t * t * t * b[1],
    ]
}
fn hit(segment: &Segment, p: [f32; 2], id: u32) -> Hit {
    let d = [segment.b[0] - segment.a[0], segment.b[1] - segment.a[1]];
    let l2 = d[0] * d[0] + d[1] * d[1];
    let t = (((p[0] - segment.a[0]) * d[0] + (p[1] - segment.a[1]) * d[1]) / l2.max(0.001))
        .clamp(0.0, 1.0);
    let point = [segment.a[0] + d[0] * t, segment.a[1] + d[1] * t];
    Hit {
        point,
        tangent: normalize(d),
        distance: distance2(point, p).sqrt(),
        width: lerp(segment.width_a, segment.width_b, t),
        level: lerp(segment.level_a, segment.level_b, t),
        segment: id,
    }
}
fn bucket(v: f32) -> usize {
    (((v + HALF) / BUCKET).floor() as i32).clamp(0, BUCKETS as i32 - 1) as usize
}

impl Hydrology {
    pub fn new(seed: u32) -> Self {
        let size = GRID * GRID;
        let raw: Vec<f32> = (0..size)
            .map(|i| {
                let p = node_position(i);
                raw_height(seed, p[0], p[1])
            })
            .collect();
        let mut conditioned = raw.clone();
        let mut parent = vec![NONE; size];
        let mut visited = vec![false; size];
        let mut queue = BinaryHeap::new();
        for i in 0..size {
            let x = i % GRID;
            let z = i / GRID;
            if x == 0 || z == 0 || x == GRID - 1 || z == GRID - 1 {
                visited[i] = true;
                queue.push(FloodNode {
                    height: raw[i],
                    index: i as u32,
                });
            }
        }
        let mut order = Vec::with_capacity(size);
        let mut rank = vec![0u32; size];
        while let Some(node) = queue.pop() {
            let i = node.index as usize;
            rank[i] = order.len() as u32;
            order.push(node.index);
            for (n, distance) in neighbors(i) {
                if visited[n] {
                    continue;
                }
                visited[n] = true;
                parent[n] = node.index;
                // A small terrain-weighted outlet gradient resolves filled flats.
                // Correlated friction favors old low ground without the index-
                // ordered diagonal fans produced by a perfectly level D8 basin.
                let p = node_position(n);
                let roughness = 0.25
                    + 1.7 * noise(seed ^ 0x6901, p[0] / 1300.0, p[1] / 1300.0)
                    + 0.3 * rand01(hash(seed ^ 0x6902, (n % GRID) as i32, (n / GRID) as i32));
                let depth = (node.height - raw[n]).max(0.0);
                let outlet_gradient = distance * 0.00012 * roughness / (1.0 + depth / 120.0);
                conditioned[n] = raw[n].max(node.height + outlet_gradient);
                queue.push(FloodNode {
                    height: conditioned[n],
                    index: n as u32,
                });
            }
        }
        let mut receiver = vec![NONE; size];
        for &id in &order {
            let i = id as usize;
            if parent[i] == NONE {
                continue;
            }
            let mut best = parent[i];
            let mut steepest = 0.0;
            for (n, d) in neighbors(i) {
                let drop = conditioned[i] - conditioned[n];
                if drop > 0.0 && drop / d > steepest {
                    steepest = drop / d;
                    best = n as u32;
                }
            }
            if steepest == 0.0 {
                // Priority-Flood parent gives a deterministic acyclic route across filled flats.
                best = parent[i];
            }
            debug_assert!(rank[best as usize] < rank[i]);
            receiver[i] = best;
        }
        let mut accumulation: Vec<f32> = (0..size)
            .map(|i| {
                let p = node_position(i);
                0.42 + noise(seed ^ 0x3102, p[0] / 29000.0, p[1] / 29000.0) * 1.28
            })
            .collect();
        let mut catchment_cells = vec![1u32; size];
        for &id in order.iter().rev() {
            let i = id as usize;
            if receiver[i] != NONE {
                let d = receiver[i] as usize;
                accumulation[d] += accumulation[i];
                catchment_cells[d] += catchment_cells[i];
            }
        }
        #[cfg(test)]
        let basin = {
            let mut ids = vec![0u32; size];
            for &id in &order {
                let i = id as usize;
                ids[i] = if receiver[i] == NONE {
                    id
                } else {
                    ids[receiver[i] as usize]
                };
            }
            ids
        };
        let active: Vec<bool> = accumulation
            .iter()
            .map(|&a| a >= CHANNEL_THRESHOLD)
            .collect();
        let mut upstream = vec![NONE; size];
        let mut incoming = vec![0u8; size];
        for i in 0..size {
            let r = receiver[i];
            if r == NONE || !active[i] {
                continue;
            }
            let d = r as usize;
            incoming[d] += 1;
            if upstream[d] == NONE || accumulation[i] > accumulation[upstream[d] as usize] {
                upstream[d] = i as u32;
            }
        }
        let mut positions: Vec<[f32; 2]> = (0..size).map(node_position).collect();
        // A small shared-node offset removes the rigid 500m lattice without
        // changing the drainage topology. All tributaries use the same junction.
        for i in 0..size {
            if !active[i] || receiver[i] == NONE {
                continue;
            }
            let x = i % GRID;
            let z = i / GRID;
            if x > 0 && z > 0 && x + 1 < GRID && z + 1 < GRID {
                positions[i][0] += (rand01(hash(seed ^ 0x6a31, x as i32, z as i32)) - 0.5) * 72.0;
                positions[i][1] += (rand01(hash(seed ^ 0x6a32, x as i32, z as i32)) - 0.5) * 72.0;
            }
        }
        let mut tangents = vec![[0.0, 1.0]; size];
        for i in 0..size {
            if !active[i] {
                continue;
            }
            let previous = if upstream[i] != NONE {
                positions[upstream[i] as usize]
            } else {
                positions[i]
            };
            let next = if receiver[i] != NONE {
                positions[receiver[i] as usize]
            } else {
                positions[i]
            };
            tangents[i] = normalize([next[0] - previous[0], next[1] - previous[1]]);
        }
        let mut water_level: Vec<f32> = positions
            .iter()
            .map(|p| raw_height(seed, p[0], p[1]) - 1.5)
            .collect();
        // Burn the lowest upstream basin level through its spill route. Unlike
        // rendering the flood-filled DEM directly, this never makes aqueducts.
        for &id in order.iter().rev() {
            let i = id as usize;
            let target = receiver[i];
            if target == NONE {
                continue;
            }
            let d = target as usize;
            let mut lowest = water_level[i] - EPS;
            if active[i] {
                for n in 0..=SUBDIV {
                    let t = n as f32 / SUBDIV as f32;
                    let p = bezier(positions[i], positions[d], tangents[i], tangents[d], t);
                    let before = bezier(
                        positions[i],
                        positions[d],
                        tangents[i],
                        tangents[d],
                        (t - 0.01).max(0.0),
                    );
                    let after = bezier(
                        positions[i],
                        positions[d],
                        tangents[i],
                        tangents[d],
                        (t + 0.01).min(1.0),
                    );
                    let dir = normalize([after[0] - before[0], after[1] - before[1]]);
                    let bank = lerp(width(accumulation[i]), width(accumulation[d]), t) * 2.1 + 8.0;
                    for side in [-1.0, 0.0, 1.0] {
                        let q = [p[0] + dir[1] * bank * side, p[1] - dir[0] * bank * side];
                        lowest = lowest.min(raw_height(seed, q[0], q[1]) - 1.5);
                    }
                }
            }
            water_level[d] = water_level[d].min(lowest - EPS);
        }
        let mut segments = Vec::new();
        let mut maximum_incision = 0.0f32;
        let mut sum_incision = 0.0;
        for i in 0..size {
            if !active[i] || receiver[i] == NONE {
                continue;
            }
            let d = receiver[i] as usize;
            let mut last = positions[i];
            let mut last_level = water_level[i];
            let mut last_width = width(accumulation[i]);
            for n in 1..=SUBDIV {
                let t = n as f32 / SUBDIV as f32;
                let p = if n == SUBDIV {
                    positions[d]
                } else {
                    bezier(positions[i], positions[d], tangents[i], tangents[d], t)
                };
                let dir = normalize([p[0] - last[0], p[1] - last[1]]);
                let bank = lerp(width(accumulation[i]), width(accumulation[d]), t) * 2.1 + 8.0;
                let mut clearance = raw_height(seed, p[0], p[1]) - 1.5;
                for side in [-1.0, 1.0] {
                    clearance = clearance.min(
                        raw_height(
                            seed,
                            p[0] + dir[1] * bank * side,
                            p[1] - dir[0] * bank * side,
                        ) - 1.5,
                    );
                }
                let level = if n == SUBDIV {
                    water_level[d]
                } else {
                    lerp(water_level[i], water_level[d], t)
                        .min(clearance)
                        .min(last_level - EPS * 0.1)
                        .max(water_level[d])
                };
                let w = width(lerp(accumulation[i], accumulation[d], t));
                let middle = [(last[0] + p[0]) * 0.5, (last[1] + p[1]) * 0.5];
                let incision =
                    (raw_height(seed, middle[0], middle[1]) - (last_level + level) * 0.5).max(0.0);
                maximum_incision = maximum_incision.max(incision);
                sum_incision += incision;
                segments.push(Segment {
                    a: last,
                    b: p,
                    level_a: last_level,
                    level_b: level,
                    width_a: last_width,
                    width_b: w,
                    flow: accumulation[i],
                    influence: (340.0 + incision * 1.5 + w * 2.0).min(2200.0),
                });
                last = p;
                last_level = level;
                last_width = w;
            }
        }
        let mut buckets = vec![Vec::new(); BUCKETS * BUCKETS];
        let mut references = 0;
        for (i, s) in segments.iter().enumerate() {
            let pad = s.influence;
            for z in bucket(s.a[1].min(s.b[1]) - pad)..=bucket(s.a[1].max(s.b[1]) + pad) {
                for x in bucket(s.a[0].min(s.b[0]) - pad)..=bucket(s.a[0].max(s.b[0]) + pad) {
                    buckets[z * BUCKETS + x].push(i as u32);
                    references += 1;
                }
            }
        }
        let stats = Stats {
            grid_resolution: GRID,
            grid_spacing_m: CELL,
            drainage_cells: size,
            river_segments: segments.len(),
            headwaters: (0..size).filter(|&i| active[i] && incoming[i] == 0).count(),
            confluences: incoming.iter().filter(|&&n| n > 1).count(),
            outlets: (0..size)
                .filter(|&i| active[i] && receiver[i] == NONE)
                .count(),
            max_catchment_km2: catchment_cells.iter().copied().max().unwrap_or(0) as f32
                * CELL
                * CELL
                / 1_000_000.0,
            max_incision_m: maximum_incision,
            mean_incision_m: sum_incision / segments.len().max(1) as f32,
            index_references: references,
        };
        Self {
            segments,
            buckets,
            stats,
            #[cfg(test)]
            conditioned,
            #[cfg(test)]
            receiver,
            #[cfg(test)]
            accumulation,
            #[cfg(test)]
            water_level,
            #[cfg(test)]
            order,
            #[cfg(test)]
            basin,
        }
    }
    pub fn nearest(&self, x: f32, z: f32) -> Hit {
        let mut best = Hit::none();
        for &id in &self.buckets[bucket(z) * BUCKETS + bucket(x)] {
            let h = hit(&self.segments[id as usize], [x, z], id);
            if h.distance < best.distance {
                best = h;
            }
        }
        best
    }
    pub fn terrain(&self, x: f32, z: f32, raw: f32) -> TerrainWater {
        let mut result = TerrainWater {
            height: raw,
            water: NO_WATER,
            river: 0.0,
            nearest: Hit::none(),
        };
        let mut total = 0.0;
        let mut terrain_sum = 0.0;
        let mut level_sum = 0.0;
        let mut depth_sum = 0.0;
        let mut proximity = f32::MAX;
        for &id in &self.buckets[bucket(z) * BUCKETS + bucket(x)] {
            let segment = &self.segments[id as usize];
            let h = hit(segment, [x, z], id);
            if h.distance < result.nearest.distance {
                result.nearest = h;
            }
            if h.distance >= segment.influence {
                continue;
            }
            let depth = (2.2 + h.width * 0.09).min(9.0);
            let bank_end = h.width * 1.9;
            let bank = smooth(h.width * 0.70, bank_end, h.distance);
            let floor = h.level - depth;
            let valley_weight = 1.0 - smooth(bank_end + 20.0, segment.influence, h.distance);
            let bank_land =
                raw.min(h.level + 3.5 + (raw - h.level - 3.5).max(0.0) * (1.0 - valley_weight));
            let shaped = lerp(floor, bank_land, bank);
            // Smooth, local corridor ownership prevents a lower adjacent reach
            // from cutting the ground out beneath an upper river. Compact
            // support makes the spatial index invisible at bucket boundaries.
            let support = 1.0 - smooth(segment.influence * 0.72, segment.influence, h.distance);
            let inverse = 1.0 / (h.distance * h.distance + 4.0);
            let weight = support * inverse * inverse * inverse;
            total += weight;
            terrain_sum += weight * shaped;
            level_sum += weight * h.level;
            depth_sum += weight * depth;
            proximity = proximity.min(h.distance / h.width.max(1.0));
            result.river = result
                .river
                .max(1.0 - smooth(h.width, bank_end, h.distance));
        }
        if total > 0.0 {
            let level = level_sum / total;
            let depth = depth_sum / total;
            let shaped = terrain_sum / total;
            // The same continuous surface owns water, its bed and both banks.
            // In particular, overlapping tributaries cannot independently
            // choose a high water surface and a low neighboring valley floor.
            let channel = lerp(level - depth, level + 3.5, smooth(0.70, 1.90, proximity));
            let protection = 1.0 - smooth(1.90, 2.60, proximity);
            result.height = lerp(shaped, channel, protection);
            if proximity < 1.90 {
                result.water = level;
            }
        }
        result
    }
    pub fn crossings(&self, a: [f32; 2], b: [f32; 2]) -> Vec<Crossing> {
        let mut ids = HashSet::new();
        for z in bucket(a[1].min(b[1]))..=bucket(a[1].max(b[1])) {
            for x in bucket(a[0].min(b[0]))..=bucket(a[0].max(b[0])) {
                ids.extend(self.buckets[z * BUCKETS + x].iter().copied());
            }
        }
        let r = [b[0] - a[0], b[1] - a[1]];
        let mut found = Vec::new();
        for id in ids {
            let s = &self.segments[id as usize];
            let d = [s.b[0] - s.a[0], s.b[1] - s.a[1]];
            let det = r[0] * d[1] - r[1] * d[0];
            if det.abs() < 0.0001 {
                continue;
            }
            let q = [s.a[0] - a[0], s.a[1] - a[1]];
            let t = (q[0] * d[1] - q[1] * d[0]) / det;
            let u = (q[0] * r[1] - q[1] * r[0]) / det;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                let p = [a[0] + r[0] * t, a[1] + r[1] * t];
                found.push(Crossing {
                    hit: hit(s, p, id),
                    t,
                });
            }
        }
        found.sort_by(|a, b| {
            a.t.total_cmp(&b.t)
                .then_with(|| a.hit.segment.cmp(&b.hit.segment))
        });
        found.dedup_by(|a, b| distance2(a.hit.point, b.hit.point) < 35.0 * 35.0);
        found
    }
}
