//! Terrain-derived drainage. Priority-Flood finds spill routes; D8 receivers
//! accumulate runoff. River profiles breach depressions and stay below terrain.
//! See Barnes, Lehman & Mulla (2014), doi:10.1016/j.cageo.2013.04.024.
use super::{distance2, hash, lerp, noise, rand01, raw_height, smooth, SEA_LEVEL, WORLD_SIZE};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashSet},
};

pub const GRID: usize = 769;
pub const CELL: f32 = WORLD_SIZE / (GRID - 1) as f32;
const HALF: f32 = WORLD_SIZE * 0.5;
const BUCKET: f32 = 1000.0;
const BUCKETS: usize = (WORLD_SIZE / BUCKET) as usize + 1;
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
    pub deposition: f32,
    /// Signed curvature of the actual smoothed reach; positive bends left.
    pub bend: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterEdgeKind {
    None,
    RiverGravel,
    RiverCutBank,
    LakeMud,
    LakeReeds,
    LakeGravel,
    CoastalShelf,
    CoastalSand,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WaterEdge {
    pub kind: WaterEdgeKind,
    /// Continuous strength near the bank, never a wet/dry geometry classifier.
    pub influence: f32,
    pub sediment: f32,
    pub wetness: f32,
    pub water_level: f32,
    pub flow: [f32; 2],
}
impl WaterEdge {
    pub fn none() -> Self {
        Self {
            kind: WaterEdgeKind::None,
            influence: 0.,
            sediment: 0.,
            wetness: 0.,
            water_level: NO_WATER,
            flow: [0., 1.],
        }
    }
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
    pub land_cells: usize,
    pub ocean_cells: usize,
    pub coastal_outlets: usize,
    pub retained_bytes: usize,
    pub estimated_peak_generation_bytes: usize,
    pub river_segments: usize,
    pub retained_lakes: usize,
    pub lake_area_km2: f32,
    pub headwaters: usize,
    pub confluences: usize,
    pub outlets: usize,
    /// Actual contributing cell area; channel widths use rainfall-weighted runoff.
    pub max_catchment_km2: f32,
    pub max_incision_m: f32,
    pub mean_incision_m: f32,
    pub index_references: usize,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct LakeInfo {
    pub id: u32,
    pub center: [f32; 2],
    pub surface: f32,
    pub outlet: [f32; 2],
    /// min x, min z, max x, max z; includes a narrow conditioned shore margin.
    pub bounds: [f32; 4],
    pub area_km2: f32,
    pub max_depth: f32,
}
fn lake_margin(seed: u32, x: f32, z: f32) -> (f32, f32, f32) {
    let b = crate::regions::base(seed, x, z);
    let exposure =
        (b.exposure * 0.70 + noise(seed ^ 0x6c81, x / 850., z / 850.) * 0.30).clamp(0., 1.);
    let sediment = (b.fertility * 0.72 + (1. - exposure) * 0.28).clamp(0., 1.);
    (
        lerp(0.042, 0.145, (1. - sediment) * 0.70 + exposure * 0.30),
        sediment,
        exposure,
    )
}
fn retain_lakes(
    seed: u32,
    raw: &[f32],
    conditioned: &[f32],
    receiver: &[u32],
    flow: &[f32],
) -> (Vec<LakeInfo>, Vec<u16>, Vec<u8>, Vec<f32>) {
    let size = raw.len();
    let mut seen = vec![false; size];
    let mut candidates = Vec::new();
    let mut incoming = vec![NONE; size];
    let mut next = vec![NONE; size];
    for i in 0..size {
        if receiver[i] != NONE {
            let d = receiver[i] as usize;
            next[i] = incoming[d];
            incoming[d] = i as u32;
        }
    }
    for i in 0..size {
        if seen[i] || raw[i] <= 30. || conditioned[i] - raw[i] < 4. {
            continue;
        }
        let mut todo = vec![i];
        let mut cells = Vec::new();
        seen[i] = true;
        while let Some(j) = todo.pop() {
            cells.push(j);
            for (n, _) in neighbors(j) {
                if !seen[n] && raw[n] > 30. && conditioned[n] - raw[n] >= 4. {
                    seen[n] = true;
                    todo.push(n);
                }
            }
        }
        if cells.len() < 6 || cells.len() > 104 {
            continue;
        }
        let minimum = *cells
            .iter()
            .min_by(|&&a, &&b| raw[a].total_cmp(&raw[b]))
            .unwrap();
        let level = cells
            .iter()
            .map(|&j| conditioned[j])
            .fold(f32::MAX, f32::min)
            - 0.8;
        let top = cells.iter().map(|&j| conditioned[j]).fold(0., f32::max);
        let depth = level - raw[minimum];
        let maxflow = cells.iter().map(|&j| flow[j]).fold(0., f32::max);
        // Retain suitable perched basins as well as lowland water. Their same
        // bounded footprint, compatible ancestors and spillway checks prevent
        // high lakes from flooding unrelated catchments or creating water walls.
        let upland = smooth(450., 1100., level);
        let maximum_depth = lerp(55., 82., upland);
        if !(10.0..=maximum_depth).contains(&depth)
            || level > 1550.
            || top - level > 2.5
            || maxflow < 26.
        {
            continue;
        }
        let score = cells.len() as f32 * 0.20
            + depth * 0.12
            + upland * 10.
            + rand01(hash(seed ^ 0x6c11, minimum as i32, 0)) * 8.;
        candidates.push((score, minimum, level));
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut lakes: Vec<LakeInfo> = Vec::new();
    let mut owner = vec![0u16; size];
    for (_, minimum, level) in candidates {
        if lakes.len() >= 72 {
            break;
        }
        let center = node_position(minimum);
        if lakes
            .iter()
            .any(|l| distance2(center, l.center) < 7000. * 7000.)
        {
            continue;
        }
        let mut todo = vec![minimum];
        let mut cells = Vec::new();
        let mut accepted = HashSet::new();
        accepted.insert(minimum);
        while let Some(j) = todo.pop() {
            cells.push(j);
            if cells.len() > 180 {
                break;
            }
            for (n, _) in neighbors(j) {
                if raw[n] < level + 0.2 && raw[n] > 30. && accepted.insert(n) {
                    todo.push(n);
                }
            }
        }
        if cells.len() > 180
            || cells.iter().any(|&j| {
                let x = j % GRID;
                let z = j / GRID;
                (z.saturating_sub(8)..=(z + 8).min(GRID - 1)).any(|zz| {
                    (x.saturating_sub(8)..=(x + 8).min(GRID - 1))
                        .any(|xx| owner[zz * GRID + xx] != 0)
                })
            })
        {
            continue;
        }
        // Follow the same acyclic receiver graph to the first dry spill cell. The
        // retained surface is slightly below that saddle; its outlet is a small cut.
        let mut outlet = minimum;
        let mut visited = 0;
        while receiver[outlet] != NONE
            && accepted.contains(&(receiver[outlet] as usize))
            && visited < size
        {
            outlet = receiver[outlet] as usize;
            visited += 1;
        }
        if receiver[outlet] == NONE {
            continue;
        }
        let downstream = receiver[outlet] as usize;
        if raw[downstream] < level - 4. {
            continue;
        }
        // Retaining this basin must not raise an unrelated, deeper upstream pocket.
        // Every submerged ancestor must belong to this connected flooded footprint.
        let mut stack = vec![outlet];
        let mut compatible = true;
        while let Some(j) = stack.pop() {
            if raw[j] < level - 0.8 && !accepted.contains(&j) {
                compatible = false;
                break;
            }
            let mut child = incoming[j];
            while child != NONE {
                stack.push(child as usize);
                child = next[child as usize];
            }
        }
        if !compatible {
            continue;
        }
        let id = lakes.len() as u16 + 1;
        let mut bounds = [center[0], center[1], center[0], center[1]];
        for &j in &cells {
            owner[j] = id;
            let p = node_position(j);
            bounds[0] = bounds[0].min(p[0] - CELL * 3.);
            bounds[1] = bounds[1].min(p[1] - CELL * 3.);
            bounds[2] = bounds[2].max(p[0] + CELL * 3.);
            bounds[3] = bounds[3].max(p[1] + CELL * 3.);
        }
        lakes.push(LakeInfo {
            id: id as u32,
            center,
            surface: level,
            outlet: node_position(downstream),
            bounds,
            area_km2: cells.len() as f32 * CELL * CELL / 1e6,
            max_depth: level - raw[minimum],
        });
    }
    let mut minimum_level = vec![SEA_LEVEL; size];
    // Reverse dependency traversal: receivers always have lower flood rank, but
    // their indices need not be ordered. Memoized walks visit each cell once.
    let mut done = vec![false; size];
    for start in 0..size {
        if done[start] {
            continue;
        }
        let mut path = Vec::new();
        let mut i = start;
        while !done[i] {
            path.push(i);
            done[i] = true;
            if receiver[i] == NONE {
                break;
            }
            i = receiver[i] as usize;
        }
        for &j in path.iter().rev() {
            let below = if receiver[j] == NONE {
                SEA_LEVEL
            } else {
                minimum_level[receiver[j] as usize]
            };
            minimum_level[j] = if owner[j] > 0 {
                lakes[owner[j] as usize - 1].surface.max(below)
            } else {
                below
            };
        }
    }
    // Expand only the dry shore interpolation stencil. The actual water footprint
    // is still clipped by the terrain at the retained level, not by cell squares.
    let mut strength: Vec<u8> = owner.iter().map(|&i| if i > 0 { 255 } else { 0 }).collect();
    for value in [192u8, 128, 64] {
        let previous = owner.clone();
        for i in 0..size {
            if previous[i] > 0 {
                for (n, _) in neighbors(i) {
                    if owner[n] == 0 && raw[n] > lakes[previous[i] as usize - 1].surface - 3. {
                        owner[n] = previous[i];
                        strength[n] = value;
                    }
                }
            }
        }
    }
    (lakes, owner, strength, minimum_level)
}

#[derive(Clone, Debug)]
pub struct Hydrology {
    seed: u32,
    pub segments: Vec<Segment>,
    pub lakes: Vec<LakeInfo>,
    lake_grid: Vec<u16>,
    lake_strength: Vec<u8>,
    buckets: Vec<Vec<u32>>,
    pub stats: Stats,
    // Audit data is dropped after generation in browser/native builds.
    #[cfg(test)]
    pub ocean: Vec<bool>,
    #[cfg(test)]
    pub mouths: Vec<[f32; 2]>,
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
fn reach_width(flow: f32, level: f32) -> f32 {
    // A low-gradient tidal reach broadens smoothly as it approaches sea level.
    // Accumulation and downstream drop only increase this width, preserving
    // confluence continuity and the no-narrowing downstream invariant.
    width(flow) * (1. + smooth(180., 2200., flow) * (1. - smooth(2., 30., level)) * 0.85)
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

// Return the last land-side point before the first sea crossing of a refined
// reach. Interior probes also catch narrow coastal inlets between land endpoints.
fn coast_intersection(
    a: [f32; 2],
    b: [f32; 2],
    height_at: &impl Fn(f32, f32) -> f32,
) -> Option<[f32; 2]> {
    let mut previous = a;
    for n in 1..=8 {
        let t = n as f32 / 8.0;
        let point = [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
        if height_at(point[0], point[1]) <= SEA_LEVEL {
            let mut land = previous;
            let mut sea = point;
            for _ in 0..16 {
                let middle = [(land[0] + sea[0]) * 0.5, (land[1] + sea[1]) * 0.5];
                if height_at(middle[0], middle[1]) > SEA_LEVEL {
                    land = middle;
                } else {
                    sea = middle;
                }
            }
            return Some(land);
        }
        previous = point;
    }
    None
}

impl Hydrology {
    pub fn new(seed: u32) -> Self {
        Self::from_height(seed, |x, z| raw_height(seed, x, z))
    }
    fn from_height(seed: u32, height_at: impl Fn(f32, f32) -> f32) -> Self {
        let size = GRID * GRID;
        let raw: Vec<f32> = (0..size)
            .map(|i| {
                let p = node_position(i);
                height_at(p[0], p[1])
            })
            .collect();
        let ocean: Vec<bool> = raw.iter().map(|&h| h <= SEA_LEVEL).collect();
        let land_cells = ocean.iter().filter(|&&sea| !sea).count();
        let mut conditioned: Vec<f32> = raw.iter().map(|&h| h.max(SEA_LEVEL)).collect();
        let mut parent = vec![NONE; size];
        let mut visited = vec![false; size];
        let mut queue = BinaryHeap::new();
        for i in 0..size {
            let x = i % GRID;
            let z = i / GRID;
            if ocean[i] || x == 0 || z == 0 || x == GRID - 1 || z == GRID - 1 {
                visited[i] = true;
                queue.push(FloodNode {
                    height: conditioned[i],
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
                if ocean[i] {
                    return 0.0;
                }
                let p = node_position(i);
                0.24 + crate::regions::base(seed, p[0], p[1]).rainfall * 1.52
            })
            .collect();
        let mut catchment_cells: Vec<u32> = ocean.iter().map(|&sea| u32::from(!sea)).collect();
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
        // Sea cells receive inland runoff, but never generate rain or channels.
        let active: Vec<bool> = accumulation
            .iter()
            .enumerate()
            .map(|(i, &a)| !ocean[i] && a >= CHANNEL_THRESHOLD)
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
        let (mut lakes, lake_grid, lake_strength, minimum_level) =
            retain_lakes(seed, &raw, &conditioned, &receiver, &accumulation);
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
                let candidate = [
                    positions[i][0]
                        + (rand01(hash(seed ^ 0x6a31, x as i32, z as i32)) - 0.5) * 72.0,
                    positions[i][1]
                        + (rand01(hash(seed ^ 0x6a32, x as i32, z as i32)) - 0.5) * 72.0,
                ];
                if height_at(candidate[0], candidate[1]) > SEA_LEVEL {
                    positions[i] = candidate;
                }
            }
        }
        for lake in &mut lakes {
            let mut i = (((lake.outlet[1] + HALF) / CELL).round() as usize) * GRID
                + ((lake.outlet[0] + HALF) / CELL).round() as usize;
            for _ in 0..200 {
                if active[i] || receiver[i] == NONE {
                    break;
                }
                i = receiver[i] as usize;
            }
            lake.outlet = positions[i];
        }
        let mut tangents = vec![[0.0, 1.0]; size];
        for i in 0..size {
            if !active[i] && incoming[i] == 0 {
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
            .enumerate()
            .map(|(i, p)| (height_at(p[0], p[1]) - 1.5).max(minimum_level[i]))
            .collect();
        // Burn the lowest upstream basin level through its spill route. Unlike
        // rendering the flood-filled DEM directly, this never makes aqueducts.
        for &id in order.iter().rev() {
            let i = id as usize;
            let target = receiver[i];
            if target == NONE || !active[i] {
                continue;
            }
            let d = target as usize;
            let mut lowest = (water_level[i] - EPS).max(SEA_LEVEL);
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
                    let bank = lerp(
                        reach_width(accumulation[i], water_level[i]),
                        reach_width(accumulation[d], water_level[d]),
                        t,
                    ) * 2.1
                        + 8.0;
                    for side in [-1.0, 0.0, 1.0] {
                        let q = [p[0] + dir[1] * bank * side, p[1] - dir[0] * bank * side];
                        lowest = lowest.min((height_at(q[0], q[1]) - 1.5).max(SEA_LEVEL));
                    }
                }
            }
            water_level[d] = water_level[d].min((lowest - EPS).max(minimum_level[d]));
        }
        // Non-channel runoff uses the conditioned drainage surface. Small
        // closed pockets cannot burn a distant, deeply incised trunk river.
        // This virtual profile never renders water: only active reaches below
        // the real terrain enter the segment mesh below. Preserve downstream
        // ordering for every receiver, including these unrendered runoff cells.
        for &id in &order {
            let i = id as usize;
            if !active[i] && receiver[i] != NONE {
                water_level[i] = water_level[i].max(water_level[receiver[i] as usize] + EPS);
            }
        }
        let mut segments = Vec::new();
        let mut coastal_outlets = 0usize;
        #[cfg(test)]
        let mut mouths = Vec::new();
        let mut maximum_incision = 0.0f32;
        let mut sum_incision = 0.0;
        for i in 0..size {
            if !active[i] || receiver[i] == NONE {
                continue;
            }
            let d = receiver[i] as usize;
            let mut last = positions[i];
            let mut last_level = water_level[i];
            let mut last_width = reach_width(accumulation[i], water_level[i]);
            for n in 1..=SUBDIV {
                let t = n as f32 / SUBDIV as f32;
                let candidate = if n == SUBDIV {
                    positions[d]
                } else {
                    bezier(positions[i], positions[d], tangents[i], tangents[d], t)
                };
                // A D8 edge may enter a bay before its receiver cell. Stop at
                // the first actual shoreline, rather than drawing seabed rivers.
                let shore = coast_intersection(last, candidate, &height_at);
                let p = shore.unwrap_or(candidate);
                let mouth = shore.is_some();
                let dir = normalize([p[0] - last[0], p[1] - last[1]]);
                let bank = lerp(
                    reach_width(accumulation[i], water_level[i]),
                    reach_width(accumulation[d], water_level[d]),
                    t,
                ) * 2.1
                    + 8.0;
                let mut clearance = (height_at(p[0], p[1]) - 1.5).max(SEA_LEVEL);
                for side in [-1.0, 1.0] {
                    clearance = clearance.min(
                        height_at(p[0] + dir[1] * bank * side, p[1] - dir[0] * bank * side)
                            .max(SEA_LEVEL + 1.5)
                            - 1.5,
                    );
                }
                let level = if mouth {
                    SEA_LEVEL
                } else if n == SUBDIV {
                    water_level[d]
                } else {
                    lerp(water_level[i], water_level[d], t)
                        .min(clearance)
                        .min(last_level - EPS * 0.1)
                        .max(water_level[d])
                };
                let w = reach_width(lerp(accumulation[i], accumulation[d], t), level);
                let middle = [(last[0] + p[0]) * 0.5, (last[1] + p[1]) * 0.5];
                let incision =
                    (height_at(middle[0], middle[1]) - (last_level + level) * 0.5).max(0.0);
                maximum_incision = maximum_incision.max(incision);
                sum_incision += incision;
                let grade = (last_level - level).max(0.) / distance2(last, p).sqrt().max(1.);
                let deposition = (1. - smooth(0.002, 0.035, grade))
                    * smooth(35., 480., accumulation[i])
                    * (1. - smooth(70., 220., incision));
                if distance2(last, p) > 0.001 {
                    let mid_t = (n as f32 - 0.5) / SUBDIV as f32;
                    let left = bezier(
                        positions[i],
                        positions[d],
                        tangents[i],
                        tangents[d],
                        (mid_t - 0.07).max(0.),
                    );
                    let mid = bezier(positions[i], positions[d], tangents[i], tangents[d], mid_t);
                    let right = bezier(
                        positions[i],
                        positions[d],
                        tangents[i],
                        tangents[d],
                        (mid_t + 0.07).min(1.),
                    );
                    let ta = normalize([mid[0] - left[0], mid[1] - left[1]]);
                    let tb = normalize([right[0] - mid[0], right[1] - mid[1]]);
                    let bend = ((ta[0] * tb[1] - ta[1] * tb[0]) * 9.).clamp(-1., 1.);
                    segments.push(Segment {
                        a: last,
                        b: p,
                        level_a: last_level,
                        level_b: level,
                        width_a: last_width,
                        width_b: w,
                        flow: accumulation[i],
                        // Low-flow headwaters stay narrow. Large gentle reaches
                        // spread onto floodplains; deep spill cuts widen enough
                        // to read as valleys instead of thin vertical slits.
                        influence: (105.0
                            + smooth(24., 1700., accumulation[i]) * 330.
                            + deposition * 610.
                            + incision * 2.4
                            + w * 2.4)
                            .min(3000.0),
                        deposition,
                        bend,
                    });
                }
                if mouth {
                    coastal_outlets += 1;
                    #[cfg(test)]
                    mouths.push(p);
                    break;
                }
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
        let retained_bytes = lake_strength.capacity()
            + lake_grid.capacity() * std::mem::size_of::<u16>()
            + lakes.capacity() * std::mem::size_of::<LakeInfo>()
            + segments.capacity() * std::mem::size_of::<Segment>()
            + buckets.capacity() * std::mem::size_of::<Vec<u32>>()
            + buckets
                .iter()
                .map(|b| b.capacity() * std::mem::size_of::<u32>())
                .sum::<usize>();
        // Approximate simultaneously live generation vectors, excluding allocator
        // overhead and test-only diagnostic copies. The heap peak is bounded by
        // one FloodNode per cell, including the initially queued ocean outlets.
        let estimated_peak_generation_bytes = retained_bytes + size * (4 * 14 + 8 * 3 + 7);
        let stats = Stats {
            grid_resolution: GRID,
            grid_spacing_m: CELL,
            drainage_cells: size,
            land_cells,
            ocean_cells: size - land_cells,
            coastal_outlets,
            retained_bytes,
            estimated_peak_generation_bytes,
            river_segments: segments.len(),
            retained_lakes: lakes.len(),
            lake_area_km2: lakes.iter().map(|l| l.area_km2).sum(),
            headwaters: (0..size).filter(|&i| active[i] && incoming[i] == 0).count(),
            confluences: incoming.iter().filter(|&&n| n > 1).count(),
            outlets: (0..size)
                .filter(|&i| receiver[i] == NONE && (active[i] || incoming[i] > 0))
                .count(),
            max_catchment_km2: catchment_cells.iter().copied().max().unwrap_or(0) as f32
                * CELL
                * CELL
                / 1_000_000.0,
            max_incision_m: maximum_incision,
            mean_incision_m: sum_incision / segments.len().max(1) as f32,
            index_references: references,
        };
        let mut result = Self {
            seed,
            segments,
            lakes,
            lake_grid,
            lake_strength,
            buckets,
            stats,
            #[cfg(test)]
            ocean,
            #[cfg(test)]
            mouths,
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
        };
        for id in 0..result.lakes.len() {
            let lake = result.lakes[id];
            let at = |p: [f32; 2]| result.terrain(p[0], p[1], height_at(p[0], p[1]));
            let current = at(lake.center);
            if current.water > current.height && (current.water - lake.surface).abs() < 0.002 {
                continue;
            }
            let mut best = None;
            for z in 0..17 {
                for x in 0..17 {
                    let p = [
                        lake.center[0] + (x - 8) as f32 * 125.,
                        lake.center[1] + (z - 8) as f32 * 125.,
                    ];
                    let Some((owner, coverage)) = result.lake_membership(p[0], p[1]) else {
                        continue;
                    };
                    if owner != id || coverage < 0.8 {
                        continue;
                    }
                    let t = at(p);
                    if (t.water - lake.surface).abs() < 0.002 && t.water - t.height > 2. {
                        let score = t.water - t.height - distance2(p, lake.center).sqrt() * 0.002;
                        if best.is_none_or(|(old, _)| score > old) {
                            best = Some((score, p));
                        }
                    }
                }
            }
            if let Some((_, p)) = best {
                result.lakes[id].center = p;
            }
        }
        result
    }
    pub(super) fn lake_membership(&self, x: f32, z: f32) -> Option<(usize, f32)> {
        let fx = ((x + HALF) / CELL).clamp(0., (GRID - 1) as f32 - 0.001);
        let fz = ((z + HALF) / CELL).clamp(0., (GRID - 1) as f32 - 0.001);
        let ix = fx.floor() as usize;
        let iz = fz.floor() as usize;
        let u = fx - ix as f32;
        let v = fz - iz as f32;
        let corners = [
            (iz * GRID + ix, (1. - u) * (1. - v)),
            (iz * GRID + ix + 1, u * (1. - v)),
            ((iz + 1) * GRID + ix, (1. - u) * v),
            ((iz + 1) * GRID + ix + 1, u * v),
        ];
        let id = corners
            .iter()
            .filter(|(i, _)| self.lake_grid[*i] > 0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| self.lake_grid[*i])?;
        let weight = corners
            .iter()
            .filter(|(i, _)| self.lake_grid[*i] == id)
            .map(|(i, w)| *w * self.lake_strength[*i] as f32 / 255.)
            .sum();
        Some((id as usize - 1, weight))
    }
    pub fn lake_at(&self, x: f32, z: f32, height_at: impl FnOnce() -> f32) -> Option<&LakeInfo> {
        let (i, weight) = self.lake_membership(x, z)?;
        let lake = &self.lakes[i];
        if weight <= 0.20 {
            return None;
        }
        let t = self.terrain(x, z, height_at());
        (t.water > t.height && (t.water - lake.surface).abs() < 8.).then_some(lake)
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
        // Ocean bathymetry and its constant sea surface belong to World;
        // inland drainage must not carve a network into the ocean floor.
        if raw <= SEA_LEVEL {
            return result;
        }
        let mut total = 0.0;
        let mut terrain_sum = 0.0;
        let mut level_sum = 0.0;
        let mut depth_sum = 0.0;
        let mut proximity = f32::MAX;
        let mut inner_sum = 0.;
        let mut outer_sum = 0.;
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
            // A river has a cross-section hierarchy: headwaters form a V,
            // downstream alluvium opens into a broad floor with raised terraces.
            let floodplain_end =
                (bank_end + 20. + segment.deposition * 195.).min(segment.influence * 0.56);
            let valley_weight = 1.0 - smooth(floodplain_end, segment.influence, h.distance);
            let bank_land =
                raw.min(h.level + 3.5 + (raw - h.level - 3.5).max(0.0) * (1.0 - valley_weight));
            let terrace = segment.deposition
                * (2.2 * smooth(bank_end + 30., floodplain_end + 50., h.distance)
                    + 3.5 * smooth(floodplain_end + 85., floodplain_end + 155., h.distance))
                * (1. - smooth(segment.influence * 0.62, segment.influence, h.distance));
            let shaped = lerp(floor, bank_land + terrace, bank);
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
            let side = (h.tangent[0] * (z - h.point[1]) - h.tangent[1] * (x - h.point[0]))
                / h.distance.max(1.);
            let curvature = side * segment.bend;
            inner_sum += weight * smooth(0.05, 0.70, curvature) * segment.deposition;
            outer_sum +=
                weight * smooth(0.05, 0.70, -curvature) * (0.35 + segment.deposition * 0.65);
            proximity = proximity.min(h.distance / h.width.max(1.0));
            result.river = result
                .river
                .max(1.0 - smooth(h.width, bank_end, h.distance));
        }
        if total > 0.0 {
            let mut level = level_sum / total;
            // Mixed nearby reaches may otherwise leave a few centimetres of
            // elevated water at the coastline beside a sea-level river mouth.
            // A short tidal transition converges continuously to the sea plane.
            if raw < 24. && result.nearest.level < 8. {
                let coast = super::coast::info(self.seed, x, z);
                level *=
                    1. - smooth(0.25, 0.75, proximity) * (1. - smooth(0., 90., coast.distance));
            }
            let depth = depth_sum / total;
            let shaped = terrain_sum / total;
            // The same continuous surface owns water, its bed and both banks.
            // In particular, overlapping tributaries cannot independently
            // choose a high water surface and a low neighboring valley floor.
            let base_channel = lerp(level - depth, level + 3.5, smooth(0.70, 1.90, proximity));
            // Inner-bend sediment emerges as a low, broad gravel shelf; the
            // opposite bank has a short, steep face. The thalweg stays exactly
            // at its existing depth and level. Shared weighted ownership keeps
            // bends and confluences continuous, including where reach IDs change.
            let gravel = lerp(level - depth, level + 0.28, smooth(0.58, 1.20, proximity))
                + 3.22 * smooth(1.62, 2.45, proximity);
            let cut = lerp(level - depth, level + 3.5, smooth(0.98, 1.63, proximity));
            let channel = lerp(
                lerp(base_channel, gravel, inner_sum / total),
                cut,
                outer_sum / total,
            );
            let protection = 1.0 - smooth(1.90, 2.60, proximity);
            result.height = lerp(shaped, channel, protection);
            if proximity < 1.90 {
                result.water = level;
            }
        }
        if let Some((id, mut coverage)) = self.lake_membership(x, z) {
            let lake = &self.lakes[id];
            let limit = (lake.max_depth + 6.).min(100.);
            // A support stencil must not fill a lower neighbouring catchment or
            // borrow its river. Only compatible spill levels join this lake.
            let cut = (result.river * 8.)
                .min(1.)
                .max(smooth(3., 18., raw - result.height));
            let ownership_level = if total > 0. {
                level_sum / total
            } else {
                lake.surface
            };
            let compatibility = 1. - smooth(5., 16., (ownership_level - lake.surface).abs());
            coverage *= 1. - cut * (1. - compatibility);
            let shallow = 1. - smooth(8., 26., lake.surface - raw);
            coverage *= shallow.max(smooth(0.50, 0.65, coverage));
            let old_height = result.height;
            let old_water = result.water;
            let old_river = result.river;
            let support = smooth(0., 0.30, coverage);
            let signed = (coverage - 0.60) * 2000.;
            // Quiet, fertile margins become broad mud/reed shallows; exposed
            // hard substrate makes shorter gravel shores. No water level moves.
            let margin = lake_margin(self.seed, x, z);
            let floor = lake.surface - (signed * margin.0).clamp(-24., limit);
            let mut target = raw.min(old_height).max(floor);
            // A continuous shore berm closes unresolved sub-cell depressions.
            // The real receiver channel cuts through it, gradually joining the
            // retained plane. Its influence vanishes smoothly away from banks.
            let joining = smooth(0., 0.60, coverage);
            let channel_water = if old_water > NO_WATER {
                lerp(old_water, lake.surface, joining)
            } else {
                lake.surface
            };
            if old_water > NO_WATER {
                let cut = result.river * (1. - smooth(0.58, 0.82, coverage));
                target = lerp(target, raw.min(channel_water - 2.2), cut);
            }
            result.height = lerp(old_height, target, support);
            if old_water > NO_WATER {
                result.water = lerp(old_water, channel_water, support);
            }
            if coverage >= 0.56 && old_water <= NO_WATER {
                result.water = lake.surface;
            }
            if coverage >= 0.60 {
                result.water = lake.surface;
            }
            if old_water > NO_WATER {
                let core = smooth(0.72, 0.96, old_river);
                result.water = lerp(result.water, old_water, core);
                result.height = lerp(result.height, old_height, core);
            }
            if coverage > 0.60 && result.height < lake.surface {
                result.river = result.river.max(0.55 * smooth(0.60, 0.85, coverage));
            }
        }
        result
    }
    pub fn water_edge(&self, x: f32, z: f32, raw: f32) -> WaterEdge {
        let t = self.terrain(x, z, raw);
        if let Some((id, coverage)) = self.lake_membership(x, z) {
            let lake = &self.lakes[id];
            if coverage > 0.50 && (t.water - lake.surface).abs() < 1. {
                let (_, sediment, exposure) = lake_margin(self.seed, x, z);
                let clearance = t.height - lake.surface;
                let influence =
                    (1. - smooth(2., 9., clearance.abs())) * smooth(0.50, 0.61, coverage);
                let kind = if sediment > 0.57 && exposure < 0.55 {
                    WaterEdgeKind::LakeReeds
                } else if sediment > 0.40 {
                    WaterEdgeKind::LakeMud
                } else {
                    WaterEdgeKind::LakeGravel
                };
                return WaterEdge {
                    kind,
                    influence,
                    sediment,
                    wetness: (1. - smooth(0.2, 4., clearance)) * influence,
                    water_level: lake.surface,
                    flow: t.nearest.tangent,
                };
            }
        }
        let h = t.nearest;
        if h.segment == NONE || h.distance > h.width * 3.5 + 15. {
            return WaterEdge::none();
        }
        let segment = &self.segments[h.segment as usize];
        let side = (h.tangent[0] * (z - h.point[1]) - h.tangent[1] * (x - h.point[0]))
            / h.distance.max(1.);
        let inside = smooth(-0.15, 0.6, side * segment.bend);
        let influence = (1. - smooth(h.width * 1.8, h.width * 3.5 + 15., h.distance))
            * smooth(h.width * 0.65, h.width * 1.0, h.distance);
        WaterEdge {
            kind: if inside > 0.45 && segment.deposition > 0.15 {
                WaterEdgeKind::RiverGravel
            } else {
                WaterEdgeKind::RiverCutBank
            },
            influence,
            sediment: inside * segment.deposition,
            wetness: (1. - smooth(0.2, 4., t.height - h.level)) * influence,
            water_level: h.level,
            flow: h.tangent,
        }
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

#[cfg(test)]
mod coastal_tests {
    use super::*;
    fn archipelago_height(x: f32, z: f32) -> f32 {
        let main = (1.0 - (x + 20000.0).hypot(z) / 75000.0) * 900.0;
        let island = (1.0 - (x - 105000.0).hypot(z + 45000.0) / 12000.0) * 400.0;
        let outer_island = (1.0 - (x - 160000.0).hypot(z - 62000.0) / 14000.0) * 300.0;
        main.max(island).max(outer_island).max(-180.0)
    }
    #[test]
    fn retained_lakes_have_flat_centers_wet_outlets_and_bounded_shores() {
        for seed in [1337, 42, 2026] {
            let h = Hydrology::new(seed);
            assert!(h.lakes.len() >= 20);
            let mut tested = 0;
            for l in &h.lakes {
                let center = h.terrain(
                    l.center[0],
                    l.center[1],
                    raw_height(seed, l.center[0], l.center[1]),
                );
                assert!(
                    center.water > center.height,
                    "dry lake{} seed{}",
                    l.id,
                    seed
                );
                assert!(
                    (center.water - l.surface).abs() < 0.003,
                    "lake{} center {} vs{} seed{}",
                    l.id,
                    center.water,
                    l.surface,
                    seed
                );
                let outlet = h.terrain(
                    l.outlet[0],
                    l.outlet[1],
                    raw_height(seed, l.outlet[0], l.outlet[1]),
                );
                assert!(
                    outlet.water > outlet.height,
                    "dry routed outlet lake{} seed{} {:?}",
                    l.id,
                    seed,
                    l.outlet
                );
                assert!(
                    outlet.water <= l.surface + 0.05,
                    "uphill outlet lake{} seed{}",
                    l.id,
                    seed
                );
                for angle in 0..8 {
                    let a = angle as f32 * std::f32::consts::TAU / 8.;
                    let dir = [a.cos(), a.sin()];
                    let mut previous = l.center;
                    let mut wet = center;
                    for step in 1..100 {
                        let p = [
                            l.center[0] + dir[0] * step as f32 * 80.,
                            l.center[1] + dir[1] * step as f32 * 80.,
                        ];
                        let q = h.terrain(p[0], p[1], raw_height(seed, p[0], p[1]));
                        if wet.water > wet.height && q.water <= q.height {
                            let mut inside = previous;
                            let mut outside = p;
                            for _ in 0..16 {
                                let m = [
                                    (inside[0] + outside[0]) * 0.5,
                                    (inside[1] + outside[1]) * 0.5,
                                ];
                                let t = h.terrain(m[0], m[1], raw_height(seed, m[0], m[1]));
                                if t.water > t.height {
                                    inside = m
                                } else {
                                    outside = m
                                }
                            }
                            let a = h.terrain(
                                inside[0],
                                inside[1],
                                raw_height(seed, inside[0], inside[1]),
                            );
                            let b = h.terrain(
                                outside[0],
                                outside[1],
                                raw_height(seed, outside[0], outside[1]),
                            );
                            assert!(
                                a.water - b.height < 0.20,
                                "exposed retained shore lake{} seed{} gap{} at{:?}",
                                l.id,
                                seed,
                                a.water - b.height,
                                inside
                            );
                            tested += 1;
                            break;
                        }
                        previous = p;
                        wet = q;
                    }
                }
            }
            assert!(tested > 100);
        }
    }
    #[test]
    fn coastal_receivers_conserve_land_rainfall_and_stop_at_sea() {
        let start = std::time::Instant::now();
        let h = Hydrology::from_height(1337, archipelago_height);
        let mut rank = vec![0usize; GRID * GRID];
        for (index, &id) in h.order.iter().enumerate() {
            rank[id as usize] = index;
        }
        let mut expected_rain = 0.0f64;
        let mut outlet_rain = 0.0f64;
        for i in 0..GRID * GRID {
            if h.ocean[i] {
                assert_eq!(h.receiver[i], NONE, "ocean acquired a drainage receiver");
                assert_eq!(h.conditioned[i], SEA_LEVEL);
                assert_eq!(h.water_level[i], SEA_LEVEL);
            } else {
                let p = node_position(i);
                expected_rain +=
                    (0.24 + crate::regions::base(1337, p[0], p[1]).rainfall * 1.52) as f64;
                assert_ne!(
                    h.receiver[i], NONE,
                    "interior island drainage failed to reach a coast"
                );
                assert!(
                    h.ocean[h.basin[i] as usize],
                    "island catchment does not terminate in ocean"
                );
            }
            if h.receiver[i] == NONE {
                outlet_rain += h.accumulation[i] as f64;
            } else {
                let next = h.receiver[i] as usize;
                assert!(rank[next] < rank[i]);
                assert!(h.conditioned[next] <= h.conditioned[i]);
                assert!(h.water_level[next] <= h.water_level[i]);
            }
        }
        assert!((outlet_rain - expected_rain).abs() < expected_rain * 0.00001);
        assert!(h.stats.ocean_cells > h.stats.land_cells);
        assert!(h.stats.coastal_outlets > 20);
        println!(
            "archipelago generation {:?}; {:?}",
            start.elapsed(),
            h.stats
        );
    }
    #[test]
    fn coastal_segments_end_at_shore_without_burning_ocean_trenches() {
        let h = Hydrology::from_height(1337, archipelago_height);
        assert!(!h.mouths.is_empty());
        for &p in &h.mouths {
            assert!(
                archipelago_height(p[0], p[1]).abs() < 0.03,
                "mouth missed coast {p:?}"
            );
        }
        let mut outer_island_segments = 0;
        let mut terminal_segments = 0;
        for s in &h.segments {
            assert!(s.level_a >= SEA_LEVEL && s.level_b >= SEA_LEVEL);
            assert!(s.level_b <= s.level_a + 0.0001);
            if s.level_b == SEA_LEVEL && archipelago_height(s.b[0], s.b[1]).abs() < 0.03 {
                terminal_segments += 1;
            }
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let p = [lerp(s.a[0], s.b[0], t), lerp(s.a[1], s.b[1], t)];
                let raw = archipelago_height(p[0], p[1]);
                assert!(
                    raw >= SEA_LEVEL - 0.03,
                    "channel continued offshore {p:?}, raw {raw}"
                );
                let sample = h.terrain(p[0], p[1], raw);
                assert!(
                    sample.height >= SEA_LEVEL - 9.002,
                    "sea floor propagated a deep incision"
                );
            }
            if s.a[0] > 145000.0 {
                let p = [(s.a[0] + s.b[0]) * 0.5, (s.a[1] + s.b[1]) * 0.5];
                let hit = h.nearest(p[0], p[1]);
                assert!(hit.distance < 0.02, "spatial index lost outer island river");
                outer_island_segments += 1;
            }
        }
        assert!(terminal_segments >= h.mouths.len());
        assert!(
            outer_island_segments > 0,
            "small offshore catchment must retain visible drainage"
        );
        for x in [-180000.0, -96000.0, 60000.0, 185000.0] {
            let raw = archipelago_height(x, 0.0);
            assert!(raw < SEA_LEVEL);
            let sample = h.terrain(x, 0.0, raw);
            assert_eq!(sample.height, raw, "ocean bathymetry was modified");
            assert_eq!(sample.water, NO_WATER, "ocean sea surface belongs to World");
            assert_eq!(sample.river, 0.0);
        }
        assert_eq!(GRID, 769);
        assert!((CELL - 500.0).abs() < 0.001);
        assert_eq!(BUCKETS, 385);
        println!(
            "{} coast mouths, {} outer-island channel segments; no offshore channel geometry",
            h.mouths.len(),
            outer_island_segments
        );
    }
    #[test]
    fn coastal_all_ocean_has_no_rainfall_or_rivers() {
        let h = Hydrology::from_height(42, |_, _| -100.0);
        assert!(h.segments.is_empty());
        assert_eq!(h.stats.land_cells, 0);
        assert_eq!(h.stats.coastal_outlets, 0);
        assert!(h.accumulation.iter().all(|&a| a == 0.0));
        assert!(h.receiver.iter().all(|&r| r == NONE));
    }
    #[test]
    fn coastal_actual_world_profiles_mouths_and_memory_are_bounded() {
        for seed in [1337, 42] {
            let start = std::time::Instant::now();
            let h = Hydrology::new(seed);
            let elapsed = start.elapsed();
            let mut max_mouth_level = 0.0f32;
            let mut max_coast_bed = 0.0f32;
            let mut offshore = 0;
            assert!(h.stats.land_cells > 10000 && h.stats.ocean_cells > 10000);
            assert!(h.stats.coastal_outlets > 20);
            assert!(h.stats.retained_bytes < 90 * 1024 * 1024);
            assert!(h.stats.estimated_peak_generation_bytes < 140 * 1024 * 1024);
            for (i, s) in h.segments.iter().enumerate() {
                assert!(s.level_a >= SEA_LEVEL && s.level_b >= SEA_LEVEL);
                assert!(s.level_b <= s.level_a + 0.0001);
                if i % 7 == 0 {
                    for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        let p = [lerp(s.a[0], s.b[0], t), lerp(s.a[1], s.b[1], t)];
                        let raw = raw_height(seed, p[0], p[1]);
                        assert!(
                            raw >= SEA_LEVEL - 0.03,
                            "real-world river segment enters ocean at {p:?}, raw {raw}"
                        );
                        let hit = h.terrain(p[0], p[1], raw);
                        if raw < 35.0 {
                            assert!(hit.height >= SEA_LEVEL - 9.002);
                            max_coast_bed = max_coast_bed.max(SEA_LEVEL - hit.height);
                        }
                    }
                }
            }
            for p in &h.mouths {
                let raw = raw_height(seed, p[0], p[1]);
                assert!(
                    raw.abs() < 0.05,
                    "real-world mouth missed coast: {p:?}, raw {raw}"
                );
                let sample = h.terrain(p[0], p[1], raw);
                if sample.water > NO_WATER + 1.0 {
                    max_mouth_level = max_mouth_level.max(sample.water - SEA_LEVEL);
                }
            }
            for index in (0..GRID * GRID).step_by(97) {
                let p = node_position(index);
                let raw = raw_height(seed, p[0], p[1]);
                if raw <= SEA_LEVEL {
                    let sample = h.terrain(p[0], p[1], raw);
                    assert_eq!(sample.height, raw);
                    assert_eq!(sample.water, NO_WATER);
                    assert_eq!(sample.river, 0.0);
                    offshore += 1;
                }
            }
            assert!(offshore > 500);
            println!("seed{seed} actual coastal hydrology {elapsed:?}, max queried mouth level{max_mouth_level:.4}, max coast bed depth{max_coast_bed:.3}; {:?}", h.stats);
        }
    }
}

#[cfg(test)]
mod geography_hydrology_regressions {
    use super::*;
    #[test]
    fn perched_cirques_retain_flat_mountain_lakes_with_real_outlets() {
        for seed in [1337, 42, 2026] {
            let h = Hydrology::new(seed);
            let high: Vec<_> = h.lakes.iter().filter(|l| l.surface > 600.).collect();
            assert!(!high.is_empty(), "seed{seed} has no retained mountain lake");
            for lake in high {
                let center = h.terrain(
                    lake.center[0],
                    lake.center[1],
                    raw_height(seed, lake.center[0], lake.center[1]),
                );
                assert!((center.water - lake.surface).abs() < 0.002);
                assert!(center.water > center.height + 1.);
                let out = h.terrain(
                    lake.outlet[0],
                    lake.outlet[1],
                    raw_height(seed, lake.outlet[0], lake.outlet[1]),
                );
                assert!(out.water > out.height && out.water <= lake.surface + 0.01);
                assert!(lake.area_km2 > 0. && lake.area_km2 < 45. && lake.max_depth <= 82.);
            }
        }
    }
}
