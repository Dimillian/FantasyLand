//! A sparse settlement graph with a connected town backbone and optional rural
//! branches. Geometry is routed lazily; natural geography never depends on roads.
use super::{
    distance2, hash, lerp, rand01, segment_hit, smooth, RoadHit, World, HALF_WORLD, SITE_SPACING,
    WORLD_SIZE,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoadKind {
    Main,
    Lane,
    Trail,
}
impl RoadKind {
    pub fn half_width(self) -> f32 {
        match self {
            Self::Main => 3.3,
            Self::Lane => 2.1,
            Self::Trail => 0.85,
        }
    }
    /// Additional verge outside the travelled half-width.
    pub fn shoulder_width(self) -> f32 {
        match self {
            Self::Main => 1.0,
            Self::Lane => 0.65,
            Self::Trail => 0.35,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Main => "Main road",
            Self::Lane => "Country lane",
            Self::Trail => "Footpath",
        }
    }
    pub fn outer_width(self) -> f32 {
        self.half_width() + self.shoulder_width()
    }
    pub fn strength(self, distance: f32) -> f32 {
        1.0 - smooth(self.half_width() * 0.88, self.outer_width(), distance)
    }
    pub fn grading(self) -> f32 {
        match self {
            Self::Main => 0.90,
            Self::Lane => 0.68,
            Self::Trail => 0.22,
        }
    }
    fn visible(self, span: f32) -> bool {
        match self {
            Self::Main => true,
            Self::Lane => span <= 50000.,
            Self::Trail => span <= 18000.,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Road {
    pub id: u64,
    pub kind: RoadKind,
    pub points: Vec<[f32; 2]>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    pub sites: usize,
    pub towns: usize,
    pub main_roads: usize,
    pub lanes: usize,
    pub trails: usize,
    pub isolated_sites: usize,
    pub backbone_loops: usize,
    pub approximate_length_km: f32,
}
#[derive(Clone, Debug)]
struct Node {
    id: u32,
    key: u32,
    landmass: u32,
    p: [f32; 2],
    height: f32,
    cell: [i32; 2],
    kind: u8,
}
#[derive(Clone, Debug)]
struct Edge {
    a: usize,
    b: usize,
    kind: RoadKind,
    id: u64,
    join: Option<(usize, f32)>,
    ends: [[f32; 2]; 2],
    corridor: Vec<[f32; 2]>,
}
const GRAPH_CELL: f32 = 2048.;
const GRAPH_N: usize = (WORLD_SIZE / GRAPH_CELL) as usize + 2;
const SITE_EXTENT: i32 = (HALF_WORLD / SITE_SPACING) as i32 + 2;
const SITE_AXIS: u32 = (SITE_EXTENT * 2 + 1) as u32;
const ROAD_COAST_CLEARANCE: f32 = 16.;
const QUERY_CELL: f32 = 128.;
// Includes the route corridor, shifted crossing approaches and a branch's
// possible join displacement on a curved parent road.
const ROUTE_PAD: f32 = 2800.;
#[derive(Clone, Debug)]
pub struct Network {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    buckets: Vec<Vec<usize>>,
    pub stats: Stats,
    cache: RefCell<HashMap<usize, Rc<Road>>>,
    prepared: RefCell<HashSet<usize>>,
    segments: RefCell<HashMap<(i32, i32), Vec<(usize, usize)>>>,
}
fn bucket(v: f32) -> usize {
    (((v + HALF_WORLD) / GRAPH_CELL).floor() as i32).clamp(0, GRAPH_N as i32 - 1) as usize
}
fn query_cell(v: f32) -> i32 {
    (v / QUERY_CELL).floor() as i32
}
pub(super) fn site_kind(id: u32) -> &'static str {
    let n = hash(id ^ 0x4371, 0, 0) % 100;
    if n < 16 {
        "town"
    } else if n < 58 {
        "village"
    } else {
        "hamlet"
    }
}
fn kind_number(kind: &str) -> u8 {
    match kind {
        "town" => 0,
        "village" => 1,
        _ => 2,
    }
}
fn normalize(p: [f32; 2]) -> [f32; 2] {
    let d = (p[0] * p[0] + p[1] * p[1]).sqrt().max(0.001);
    [p[0] / d, p[1] / d]
}
fn edge_id(a: u32, b: u32, kind: RoadKind) -> u64 {
    let (a, b) = if a < b { (a, b) } else { (b, a) };
    // Cell-derived keys remain stable when coastal sites are excluded. The
    // 24-bit lower key field accommodates the larger world without overlap;
    // this world's IDs still fit exactly in JavaScript's 53 integer bits.
    ((a as u64) << 26) | ((b as u64) << 2) | kind as u64
}
fn route_seed(a: u32, b: u32, kind: RoadKind) -> u32 {
    let (a, b) = if a < b { (a, b) } else { (b, a) };
    // Keep established route geometry independent of the public route ID.
    (hash(a ^ 0x5281, b as i32, kind as i32) << 20)
        | (hash(b ^ 0x5282, a as i32, kind as i32) & 0xfffff)
}
#[derive(Clone)]
struct Union {
    parent: Vec<usize>,
}
impl Union {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn root(&mut self, i: usize) -> usize {
        if self.parent[i] != i {
            self.parent[i] = self.root(self.parent[i]);
        }
        self.parent[i]
    }
    fn join(&mut self, a: usize, b: usize) -> bool {
        let a = self.root(a);
        let b = self.root(b);
        if a == b {
            false
        } else {
            self.parent[b] = a;
            true
        }
    }
}
impl Network {
    pub fn empty() -> Self {
        Self {
            nodes: vec![],
            edges: vec![],
            buckets: vec![vec![]; GRAPH_N * GRAPH_N],
            stats: Stats {
                sites: 0,
                towns: 0,
                main_roads: 0,
                lanes: 0,
                trails: 0,
                isolated_sites: 0,
                backbone_loops: 0,
                approximate_length_km: 0.,
            },
            cache: RefCell::new(HashMap::new()),
            prepared: RefCell::new(HashSet::new()),
            segments: RefCell::new(HashMap::new()),
        }
    }
    pub fn new(world: &World) -> Self {
        let mut result = Self::empty();
        let mut cells = HashMap::new();
        for i in -SITE_EXTENT..=SITE_EXTENT {
            for j in -SITE_EXTENT..=SITE_EXTENT {
                let p = world.node(i, j);
                if p[0].abs() > HALF_WORLD || p[1].abs() > HALF_WORLD {
                    continue;
                }
                let coast = world.coast_info(p[0], p[1]);
                let Some(landmass) = coast.landmass_id else {
                    continue;
                };
                // Buildings and their approach roads need a dry, stable margin.
                let river = world.river(p[0], p[1]);
                if coast.distance < 100.
                    || world.ground(p[0], p[1]) < 3.
                    || river.distance < river.width * 2. + 35.
                {
                    continue;
                }
                let id = hash(world.seed ^ 0x4101, i, j);
                cells.insert((i, j), result.nodes.len());
                result.nodes.push(Node {
                    id,
                    key: (i + SITE_EXTENT) as u32 * SITE_AXIS + (j + SITE_EXTENT) as u32,
                    landmass,
                    p,
                    height: world.ground(p[0], p[1]),
                    cell: [i, j],
                    kind: kind_number(site_kind(id)),
                });
            }
        }
        let nodes = &result.nodes;
        let towns: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (n.kind == 0).then_some(i))
            .collect();
        let nearby = |center: usize, radius: i32| {
            let mut out = Vec::new();
            let c = nodes[center].cell;
            for i in c[0] - radius..=c[0] + radius {
                for j in c[1] - radius..=c[1] + radius {
                    if let Some(&n) = cells.get(&(i, j)) {
                        if n != center && nodes[n].landmass == nodes[center].landmass {
                            out.push(n);
                        }
                    }
                }
            }
            out
        };
        let mut pairs = BTreeSet::new();
        for &a in &towns {
            let mut near: Vec<usize> = nearby(a, 7)
                .into_iter()
                .filter(|&b| nodes[b].kind == 0)
                .collect();
            near.sort_by(|&b, &c| {
                distance2(nodes[a].p, nodes[b].p)
                    .total_cmp(&distance2(nodes[a].p, nodes[c].p))
                    .then_with(|| b.cmp(&c))
            });
            for &b in near.iter().take(9) {
                pairs.insert((a.min(b), a.max(b)));
            }
        }
        let mut candidates: Vec<(f32, usize, usize)> = pairs
            .into_iter()
            .map(|(a, b)| {
                let distance = distance2(nodes[a].p, nodes[b].p).sqrt();
                let mut rise = 0.;
                let mut previous = nodes[a].height;
                let mut wet = 0.;
                for n in 1..=5 {
                    let t = n as f32 / 5.;
                    let p = [
                        lerp(nodes[a].p[0], nodes[b].p[0], t),
                        lerp(nodes[a].p[1], nodes[b].p[1], t),
                    ];
                    let h = world.ground(p[0], p[1]);
                    let coast = world.coast_info(p[0], p[1]);
                    wet += if coast.landmass_id != Some(nodes[a].landmass) {
                        distance * 0.35
                    } else {
                        (300. - coast.distance).max(0.)
                    };
                    rise += (h - previous).abs();
                    previous = h;
                    let r = world.river(p[0], p[1]);
                    wet += if r.distance < r.width * 2. {
                        r.width * 5.
                    } else {
                        0.
                    };
                }
                (distance + rise * 3.5 + wet, a, b)
            })
            .collect();
        candidates.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        let mut union = Union::new(nodes.len());
        let mut chosen = BTreeSet::new();
        let mut edges = Vec::new();
        for &(_, a, b) in &candidates {
            if union.root(a) != union.root(b) {
                let Some(corridor) =
                    land_corridor(world, nodes[a].p, nodes[b].p, nodes[a].landmass)
                else {
                    continue;
                };
                union.join(a, b);
                chosen.insert((a, b));
                edges.push(Edge {
                    a,
                    b,
                    kind: RoadKind::Main,
                    id: edge_id(nodes[a].key, nodes[b].key, RoadKind::Main),
                    join: None,
                    ends: [nodes[a].p, nodes[b].p],
                    corridor,
                });
            }
        }
        // Repair only nearby components on the SAME landmass. A mainland and
        // an island can never acquire a fallback bridge. A genuinely unroutable
        // component remains independent instead of adding an ocean chord.
        let mut failed_repairs = BTreeSet::new();
        for _ in 0..towns.len().min(64) {
            let mut repairs = Vec::new();
            for &a in &towns {
                for b in nearby(a, 14) {
                    if b <= a
                        || nodes[b].kind != 0
                        || union.root(a) == union.root(b)
                        || failed_repairs.contains(&(a, b))
                    {
                        continue;
                    }
                    let distance = distance2(nodes[a].p, nodes[b].p);
                    if distance <= 32000. * 32000. {
                        repairs.push((distance, a, b));
                    }
                }
            }
            repairs.sort_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then_with(|| a.1.cmp(&b.1))
                    .then_with(|| a.2.cmp(&b.2))
            });
            let mut changed = false;
            for (_, a, b) in repairs.into_iter().take(96) {
                if union.root(a) == union.root(b) {
                    continue;
                }
                let Some(corridor) =
                    land_corridor(world, nodes[a].p, nodes[b].p, nodes[a].landmass)
                else {
                    failed_repairs.insert((a, b));
                    continue;
                };
                union.join(a, b);
                chosen.insert((a, b));
                changed = true;
                edges.push(Edge {
                    a,
                    b,
                    kind: RoadKind::Main,
                    id: edge_id(nodes[a].key, nodes[b].key, RoadKind::Main),
                    join: None,
                    ends: [nodes[a].p, nodes[b].p],
                    corridor,
                });
            }
            if !changed {
                break;
            }
        }
        let tree_edges = edges.len();
        let mut adjacency = vec![Vec::new(); nodes.len()];
        for e in &edges {
            let d = distance2(e.ends[0], e.ends[1]).sqrt();
            adjacency[e.a].push((e.b, d));
            adjacency[e.b].push((e.a, d));
        }
        let mut loops: Vec<_> = candidates
            .iter()
            .filter(|&&(_, a, b)| !chosen.contains(&(a, b)))
            .copied()
            .collect();
        loops.sort_by_key(|&(_, a, b)| {
            hash(world.seed ^ 0x5291, nodes[a].id as i32, nodes[b].id as i32)
        });
        for (_, a, b) in loops {
            if edges.len() >= tree_edges + towns.len() / 18 {
                break;
            }
            let direct = distance2(nodes[a].p, nodes[b].p).sqrt();
            if direct > 10500. {
                continue;
            }
            let mut queue = VecDeque::from([(a, 0., usize::MAX)]);
            let mut detour = 0.;
            while let Some((node, d, parent)) = queue.pop_front() {
                if node == b {
                    detour = d;
                    break;
                }
                for &(next, length) in &adjacency[node] {
                    if next != parent {
                        queue.push_back((next, d + length, node));
                    }
                }
            }
            if detour / direct < 1.85 {
                continue;
            }
            let Some(corridor) = land_corridor(world, nodes[a].p, nodes[b].p, nodes[a].landmass)
            else {
                continue;
            };
            edges.push(Edge {
                a,
                b,
                kind: RoadKind::Main,
                id: edge_id(nodes[a].key, nodes[b].key, RoadKind::Main),
                join: None,
                ends: [nodes[a].p, nodes[b].p],
                corridor,
            });
        }
        let main_count = edges.len();
        let mut connected = vec![false; nodes.len()];
        for edge in &edges {
            connected[edge.a] = true;
            connected[edge.b] = true;
        }
        let mut villages: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (n.kind == 1 && hash(n.id ^ 0x52a1, 0, 0) % 100 < 51).then_some(i))
            .collect();
        villages.sort_by_key(|&i| hash(nodes[i].id ^ 0x52a2, 0, 0));
        for a in villages {
            let mut near: Vec<_> = nearby(a, 6).into_iter().filter(|&b| connected[b]).collect();
            near.sort_by(|&b, &c| {
                distance2(nodes[a].p, nodes[b].p)
                    .total_cmp(&distance2(nodes[a].p, nodes[c].p))
                    .then_with(|| b.cmp(&c))
            });
            let Some((b, mut corridor)) = near.iter().take(6).find_map(|&b| {
                land_corridor(world, nodes[a].p, nodes[b].p, nodes[a].landmass)
                    .map(|path| (b, path))
            }) else {
                continue;
            };
            let mut endpoint = nodes[b].p;
            let mut parent = None;
            let mut best = distance2(nodes[a].p, endpoint).sqrt();
            // A lane may terminate at a genuine T-junction on a main road,
            // rather than sending every village to a town-center star.
            for (index, e) in edges[..main_count].iter().enumerate() {
                if nodes[e.a].landmass != nodes[a].landmass {
                    continue;
                }
                let total: f32 = e
                    .corridor
                    .windows(2)
                    .map(|p| distance2(p[0], p[1]).sqrt())
                    .sum();
                let mut covered = 0.;
                for pair in e.corridor.windows(2) {
                    let hit = segment_hit(nodes[a].p, pair[0], pair[1]);
                    let length = distance2(pair[0], pair[1]).sqrt();
                    let t = (covered + distance2(pair[0], hit.point).sqrt()) / total.max(1.);
                    covered += length;
                    if hit.distance < best * 0.80 && (0.12..0.88).contains(&t) {
                        if let Some(path) =
                            land_corridor(world, nodes[a].p, hit.point, nodes[a].landmass)
                        {
                            best = hit.distance;
                            endpoint = hit.point;
                            parent = Some((index, t));
                            corridor = path;
                        }
                    }
                }
            }
            if best > 10500. {
                continue;
            }
            connected[a] = true;
            edges.push(Edge {
                a,
                b,
                kind: RoadKind::Lane,
                id: edge_id(nodes[a].key, nodes[b].key, RoadKind::Lane),
                join: parent,
                ends: [nodes[a].p, endpoint],
                corridor,
            });
        }
        for a in 0..nodes.len() {
            if nodes[a].kind != 2 || hash(nodes[a].id ^ 0x52b1, 0, 0) % 100 >= 14 {
                continue;
            }
            let mut near: Vec<_> = nearby(a, 3)
                .into_iter()
                .filter(|&b| connected[b] && nodes[b].kind != 2)
                .collect();
            near.sort_by(|&b, &c| {
                distance2(nodes[a].p, nodes[b].p)
                    .total_cmp(&distance2(nodes[a].p, nodes[c].p))
                    .then_with(|| b.cmp(&c))
            });
            let Some((b, corridor)) = near.iter().take(4).find_map(|&b| {
                if distance2(nodes[a].p, nodes[b].p) > 5200. * 5200. {
                    return None;
                }
                land_corridor(world, nodes[a].p, nodes[b].p, nodes[a].landmass)
                    .map(|path| (b, path))
            }) else {
                continue;
            };
            connected[a] = true;
            edges.push(Edge {
                a,
                b,
                kind: RoadKind::Trail,
                id: edge_id(nodes[a].key, nodes[b].key, RoadKind::Trail),
                join: None,
                ends: [nodes[a].p, nodes[b].p],
                corridor,
            });
        }
        result.stats = Stats {
            sites: nodes.len(),
            towns: towns.len(),
            main_roads: main_count,
            lanes: edges.iter().filter(|e| e.kind == RoadKind::Lane).count(),
            trails: edges.iter().filter(|e| e.kind == RoadKind::Trail).count(),
            isolated_sites: connected.iter().filter(|&&v| !v).count(),
            backbone_loops: main_count - tree_edges,
            approximate_length_km: edges
                .iter()
                .map(|e| {
                    e.corridor
                        .windows(2)
                        .map(|p| distance2(p[0], p[1]).sqrt() / 1000.)
                        .sum::<f32>()
                })
                .sum(),
        };
        for (i, e) in edges.iter().enumerate() {
            let min_x = e
                .corridor
                .iter()
                .map(|p| p[0])
                .fold(f32::INFINITY, f32::min)
                - ROUTE_PAD;
            let max_x = e
                .corridor
                .iter()
                .map(|p| p[0])
                .fold(f32::NEG_INFINITY, f32::max)
                + ROUTE_PAD;
            let min_z = e
                .corridor
                .iter()
                .map(|p| p[1])
                .fold(f32::INFINITY, f32::min)
                - ROUTE_PAD;
            let max_z = e
                .corridor
                .iter()
                .map(|p| p[1])
                .fold(f32::NEG_INFINITY, f32::max)
                + ROUTE_PAD;
            for z in bucket(min_z)..=bucket(max_z) {
                for x in bucket(min_x)..=bucket(max_x) {
                    result.buckets[z * GRAPH_N + x].push(i);
                }
            }
        }
        result.edges = edges;
        result
    }
    fn route(&self, world: &World, index: usize) -> Rc<Road> {
        if let Some(route) = self.cache.borrow().get(&index) {
            return route.clone();
        }
        let e = &self.edges[index];
        let end = if let Some((parent, _)) = e.join {
            closest_path_point(&self.route(world, parent).points, e.ends[1])
        } else {
            e.ends[1]
        };
        let road = Rc::new(Road {
            id: e.id,
            kind: e.kind,
            points: plan_land_route(
                world,
                &e.corridor,
                end,
                route_seed(self.nodes[e.a].id, self.nodes[e.b].id, e.kind),
                e.kind,
                self.nodes[e.a].landmass,
            ),
        });
        self.cache.borrow_mut().insert(index, road.clone());
        let mut buckets = self.segments.borrow_mut();
        let pad = e.kind.outer_width() + 0.05;
        for (i, pair) in road.points.windows(2).enumerate() {
            for z in query_cell(pair[0][1].min(pair[1][1]) - pad)
                ..=query_cell(pair[0][1].max(pair[1][1]) + pad)
            {
                for x in query_cell(pair[0][0].min(pair[1][0]) - pad)
                    ..=query_cell(pair[0][0].max(pair[1][0]) + pad)
                {
                    buckets.entry((x, z)).or_default().push((index, i));
                }
            }
        }
        road
    }
    pub fn nearest(&self, world: &World, x: f32, z: f32) -> RoadHit {
        let cell = bucket(z) * GRAPH_N + bucket(x);
        if !self.prepared.borrow().contains(&cell) {
            for &id in &self.buckets[cell] {
                self.route(world, id);
            }
            self.prepared.borrow_mut().insert(cell);
        }
        let mut best = RoadHit {
            distance: f32::MAX,
            point: [x, z],
            kind: None,
        };
        let mut score = f32::INFINITY;
        let mut winner = (u8::MAX, u64::MAX, usize::MAX);
        if let Some(pieces) = self.segments.borrow().get(&(query_cell(x), query_cell(z))) {
            let cache = self.cache.borrow();
            for &(edge, segment) in pieces {
                let road = &cache[&edge];
                let p = &road.points;
                let mut hit = segment_hit([x, z], p[segment], p[segment + 1]);
                let relative = hit.distance / road.kind.outer_width();
                let key = (road.kind as u8, road.id, segment);
                if relative < score || (relative == score && key < winner) {
                    hit.kind = Some(road.kind);
                    best = hit;
                    score = relative;
                    winner = key;
                }
            }
        }
        best
    }
    fn candidates(&self, cx: f32, cz: f32, radius: f32) -> Vec<usize> {
        let mut found = BTreeSet::new();
        for z in bucket(cz - radius)..=bucket(cz + radius) {
            for x in bucket(cx - radius)..=bucket(cx + radius) {
                found.extend(self.buckets[z * GRAPH_N + x].iter().copied());
            }
        }
        found.into_iter().collect()
    }
    pub fn near(&self, world: &World, cx: f32, cz: f32, radius: f32) -> Vec<Road> {
        if !radius.is_finite() || radius < 0. {
            return vec![];
        }
        let radius = radius.min(WORLD_SIZE * 1.5);
        self.candidates(cx, cz, radius)
            .into_iter()
            .filter_map(|id| {
                let r = self.route(world, id);
                r.points
                    .windows(2)
                    .any(|p| {
                        segment_hit([cx, cz], p[0], p[1]).distance <= radius + r.kind.outer_width()
                    })
                    .then(|| r.as_ref().clone())
            })
            .collect()
    }
    pub fn map_routes(&self, world: &World, cx: f32, cz: f32, span: f32) -> Vec<Road> {
        if !span.is_finite() || span <= 0. {
            return vec![];
        }
        let radius = span * 0.72;
        self.candidates(cx, cz, radius)
            .into_iter()
            .filter_map(|id| {
                let e = &self.edges[id];
                if !e.kind.visible(span) {
                    return None;
                }
                if span > 90000. {
                    e.corridor
                        .windows(2)
                        .any(|p| segment_hit([cx, cz], p[0], p[1]).distance <= radius)
                        .then(|| Road {
                            id: e.id,
                            kind: e.kind,
                            points: e.corridor.clone(),
                        })
                } else {
                    let r = self.route(world, id);
                    r.points
                        .windows(2)
                        .any(|p| segment_hit([cx, cz], p[0], p[1]).distance <= radius)
                        .then(|| r.as_ref().clone())
                }
            })
            .collect()
    }
}
/// Test the travelled corridor using only the continental mask. Rivers are
/// deliberately not obstacles: bridge selection remains in the fine planner.
fn land_segment(world: &World, a: [f32; 2], b: [f32; 2], landmass: u32) -> bool {
    let length = distance2(a, b).sqrt();
    let mut along = 0.;
    loop {
        let t = (along / length.max(0.001)).min(1.);
        let p = [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
        let coast = world.coast_info(p[0], p[1]);
        if coast.landmass_id != Some(landmass)
            || coast.distance < ROAD_COAST_CLEARANCE
            || p[0].abs() > HALF_WORLD
            || p[1].abs() > HALF_WORLD
        {
            return false;
        }
        if along >= length {
            return true;
        }
        // Fine near a shore, coarse safely inland. Conservative coast distance
        // keeps the segment sampling independent of map or query resolution.
        along = (along + (coast.distance * 0.35).clamp(4., 192.)).min(length);
    }
}
fn land_path(world: &World, points: &[[f32; 2]], landmass: u32) -> bool {
    points.len() >= 2
        && points
            .windows(2)
            .all(|p| land_segment(world, p[0], p[1], landmass))
}

/// Cheap coast-aware graph routing. Usually the direct route is dry; a bounded
/// deterministic A* skirts an inlet when necessary. Failure means no road.
fn land_corridor(world: &World, a: [f32; 2], b: [f32; 2], landmass: u32) -> Option<Vec<[f32; 2]>> {
    if world.landmass_id(a[0], a[1]) != Some(landmass)
        || world.landmass_id(b[0], b[1]) != Some(landmass)
    {
        return None;
    }
    if land_segment(world, a, b, landmass) {
        return Some(vec![a, b]);
    }
    const CELL: f32 = 240.;
    let length = distance2(a, b).sqrt();
    let pad = (length * 0.42).clamp(900., 5200.);
    let min = [
        ((a[0].min(b[0]) - pad).max(-HALF_WORLD) / CELL).floor() * CELL,
        ((a[1].min(b[1]) - pad).max(-HALF_WORLD) / CELL).floor() * CELL,
    ];
    let max = [
        (a[0].max(b[0]) + pad).min(HALF_WORLD),
        (a[1].max(b[1]) + pad).min(HALF_WORLD),
    ];
    let nx = ((max[0] - min[0]) / CELL).ceil() as usize + 1;
    let nz = ((max[1] - min[1]) / CELL).ceil() as usize + 1;
    if nx * nz > 50000 {
        return None;
    }
    let index = |p: [f32; 2]| -> usize {
        let x = ((p[0] - min[0]) / CELL).round().clamp(0., (nx - 1) as f32) as usize;
        let z = ((p[1] - min[1]) / CELL).round().clamp(0., (nz - 1) as f32) as usize;
        z * nx + x
    };
    let first = index(a);
    let last = index(b);
    if first == last {
        return None;
    }
    let position = |id: usize| -> [f32; 2] {
        if id == first {
            a
        } else if id == last {
            b
        } else {
            [
                min[0] + (id % nx) as f32 * CELL,
                min[1] + (id / nx) as f32 * CELL,
            ]
        }
    };
    let mut costs = vec![f32::INFINITY; nx * nz];
    let mut prior = vec![usize::MAX; nx * nz];
    let mut closed = vec![false; nx * nz];
    let mut open = BinaryHeap::new();
    costs[first] = 0.;
    open.push(Reverse((0_u64, first)));
    let offsets = [
        (-1_i32, 0_i32),
        (0, -1),
        (1, 0),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ];
    let mut visited = 0;
    while let Some(Reverse((_, current))) = open.pop() {
        if closed[current] {
            continue;
        }
        if current == last {
            let mut path = vec![b];
            let mut cursor = last;
            while cursor != first {
                cursor = prior[cursor];
                if cursor == usize::MAX {
                    return None;
                }
                path.push(position(cursor));
            }
            path.reverse();
            // A few dry line-of-sight skips turn staircase cells into a sparse
            // geometric corridor; later terrain routing supplies organic bends.
            let mut compact = vec![a];
            let mut from = 0;
            while from + 1 < path.len() {
                let mut to = (from + 24).min(path.len() - 1);
                while to > from + 1 && !land_segment(world, path[from], path[to], landmass) {
                    to -= 1;
                }
                compact.push(path[to]);
                from = to;
            }
            return land_path(world, &compact, landmass).then_some(compact);
        }
        closed[current] = true;
        visited += 1;
        if visited > 14000 {
            break;
        }
        let x = (current % nx) as i32;
        let z = (current / nx) as i32;
        let p = position(current);
        for (dx, dz) in offsets {
            let xx = x + dx;
            let zz = z + dz;
            if xx < 0 || zz < 0 || xx >= nx as i32 || zz >= nz as i32 {
                continue;
            }
            let next = zz as usize * nx + xx as usize;
            if closed[next] {
                continue;
            }
            let q = position(next);
            let coast = world.coast_info(q[0], q[1]);
            if coast.landmass_id != Some(landmass) || coast.distance < ROAD_COAST_CLEARANCE {
                continue;
            }
            let step = distance2(p, q).sqrt();
            let coastal_cost = (1. - coast.distance / 700.).clamp(0., 1.) * 0.30;
            let cost = costs[current] + step * (1. + coastal_cost);
            if cost >= costs[next] || !land_segment(world, p, q, landmass) {
                continue;
            }
            costs[next] = cost;
            prior[next] = current;
            let priority = ((cost + distance2(q, b).sqrt()) * 16.) as u64;
            open.push(Reverse((priority, next)));
        }
    }
    None
}

fn closest_path_point(points: &[[f32; 2]], p: [f32; 2]) -> [f32; 2] {
    let mut closest = points.first().copied().unwrap_or(p);
    let mut best = f32::INFINITY;
    for pair in points.windows(2) {
        let hit = segment_hit(p, pair[0], pair[1]);
        if hit.distance < best {
            best = hit.distance;
            closest = hit.point;
        }
    }
    closest
}

fn plan_land_route(
    world: &World,
    corridor: &[[f32; 2]],
    end: [f32; 2],
    seed: u32,
    kind: RoadKind,
    landmass: u32,
) -> Vec<[f32; 2]> {
    let mut anchors = corridor.to_vec();
    if anchors.len() < 2 {
        return Vec::new();
    }
    let old_end = *anchors.last().unwrap();
    if distance2(old_end, end) > 0.01 {
        // Keep the chosen coastal corridor stable; only the short final link
        // changes when a lane joins the exact fine geometry of its parent.
        let Some(connector) = land_corridor(world, old_end, end, landmass) else {
            return Vec::new();
        };
        anchors.extend_from_slice(&connector[1..]);
    }
    let mut out = vec![anchors[0]];
    for (i, pair) in anchors.windows(2).enumerate() {
        let fine = plan_local(
            world,
            pair[0],
            pair[1],
            if i == 0 {
                seed
            } else {
                hash(seed, i as i32, 0)
            },
            kind,
        );
        if land_path(world, &fine, landmass) {
            out.extend_from_slice(&fine[1..]);
        } else {
            // A decorative bend or river-mouth approach may leave dry land.
            // Fall back to the already checked corridor, never an ocean bridge.
            let count = (distance2(pair[0], pair[1]).sqrt() / 38.).ceil().max(1.) as usize;
            for n in 1..=count {
                let t = n as f32 / count as f32;
                out.push([
                    lerp(pair[0][0], pair[1][0], t),
                    lerp(pair[0][1], pair[1][1], t),
                ]);
            }
        }
    }
    out.dedup_by(|a, b| distance2(*a, *b) < 0.01);
    if land_path(world, &out, landmass) {
        out
    } else {
        Vec::new()
    }
}

fn point_at(points: &[[f32; 2]], t: f32) -> [f32; 2] {
    let length: f32 = points
        .windows(2)
        .map(|p| distance2(p[0], p[1]).sqrt())
        .sum();
    let mut left = length * t.clamp(0., 1.);
    for p in points.windows(2) {
        let d = distance2(p[0], p[1]).sqrt();
        if left <= d {
            return [
                lerp(p[0][0], p[1][0], left / d.max(0.001)),
                lerp(p[0][1], p[1][1], left / d.max(0.001)),
            ];
        }
        left -= d;
    }
    *points.last().unwrap()
}
fn curve(
    out: &mut Vec<[f32; 2]>,
    a: [f32; 2],
    b: [f32; 2],
    ta: [f32; 2],
    tb: [f32; 2],
    spacing: f32,
) {
    let length = distance2(a, b).sqrt();
    let count = (length / spacing).ceil().max(1.) as usize;
    let c1 = [a[0] + ta[0] * length * 0.28, a[1] + ta[1] * length * 0.28];
    let c2 = [b[0] - tb[0] * length * 0.28, b[1] - tb[1] * length * 0.28];
    for n in 1..=count {
        let t = n as f32 / count as f32;
        let u = 1. - t;
        out.push([
            u * u * u * a[0] + 3. * u * u * t * c1[0] + 3. * u * t * t * c2[0] + t * t * t * b[0],
            u * u * u * a[1] + 3. * u * u * t * c1[1] + 3. * u * t * t * c2[1] + t * t * t * b[1],
        ]);
    }
}
fn plan_local(world: &World, a: [f32; 2], b: [f32; 2], seed: u32, kind: RoadKind) -> Vec<[f32; 2]> {
    let distance = distance2(a, b).sqrt();
    if distance < 20. {
        return vec![a, b];
    }
    let direction = normalize([b[0] - a[0], b[1] - a[1]]);
    let side = [-direction[1], direction[0]];
    let layers = (distance / 190.).ceil().clamp(3., 100.) as usize;
    const N: usize = 11;
    let corridor = (distance * 0.24).clamp(120., 1050.);
    let bend = (rand01(seed) - 0.5) * corridor * 0.42;
    let mut points = vec![[[0.; 2]; N]; layers + 1];
    let mut heights = vec![[0.; N]; layers + 1];
    let mut wet = vec![[0.; N]; layers + 1];
    for layer in 0..=layers {
        let t = layer as f32 / layers as f32;
        let arc = (std::f32::consts::PI * t).sin();
        for k in 0..N {
            let offset = ((k as f32 - 5.) / 5. * corridor + bend) * arc;
            let p = [
                (lerp(a[0], b[0], t) + side[0] * offset).clamp(-HALF_WORLD, HALF_WORLD),
                (lerp(a[1], b[1], t) + side[1] * offset).clamp(-HALF_WORLD, HALF_WORLD),
            ];
            points[layer][k] = p;
            heights[layer][k] = world.ground(p[0], p[1]);
            let r = world.river(p[0], p[1]);
            wet[layer][k] = if r.distance < r.width * 2. {
                1. + r.width * 0.02
            } else {
                0.
            };
        }
    }
    let mut costs = vec![[f32::INFINITY; N]; layers + 1];
    let mut previous = vec![[5usize; N]; layers + 1];
    costs[0][5] = 0.;
    for layer in 1..=layers {
        for k in 0..N {
            for last in k.saturating_sub(2)..=(k + 2).min(N - 1) {
                if layer == 1 && last != 5 {
                    continue;
                }
                let len = distance2(points[layer - 1][last], points[layer][k]).sqrt();
                let grade = (heights[layer][k] - heights[layer - 1][last]).abs() / len.max(1.);
                let steep = if kind == RoadKind::Trail { 0.40 } else { 0.22 };
                let grade_cost = grade * grade * 13. + (grade - steep).max(0.).powi(2) * 90.;
                let cost = costs[layer - 1][last]
                    + len * (1. + grade_cost + wet[layer][k] * 2.8)
                    + ((k as f32 - last as f32).abs() * 5.);
                if cost < costs[layer][k] {
                    costs[layer][k] = cost;
                    previous[layer][k] = last;
                }
            }
        }
    }
    let mut chosen = 5;
    let mut minimum = f32::INFINITY;
    for k in 0..N {
        if costs[layers][k] < minimum {
            minimum = costs[layers][k];
            chosen = k;
        }
    }
    let mut base = vec![[0.; 2]; layers + 1];
    for layer in (0..=layers).rev() {
        base[layer] = points[layer][chosen];
        chosen = previous[layer][chosen];
    }
    base[0] = a;
    base[layers] = b;
    let mut smooth_path = vec![a];
    for i in 0..layers {
        let before = base[i.saturating_sub(1)];
        let after = base[(i + 2).min(layers)];
        let ta = normalize([base[i + 1][0] - before[0], base[i + 1][1] - before[1]]);
        let tb = normalize([after[0] - base[i][0], after[1] - base[i][1]]);
        curve(&mut smooth_path, base[i], base[i + 1], ta, tb, 38.);
    }
    // Find real graph intersections, then replace a local neighborhood with a
    // short perpendicular crossing and curved approaches along its dry banks.
    let mut cumulative = vec![0.];
    for pair in smooth_path.windows(2) {
        cumulative.push(cumulative.last().unwrap() + distance2(pair[0], pair[1]).sqrt());
    }
    let total = *cumulative.last().unwrap();
    let mut crossings = Vec::new();
    for (i, pair) in smooth_path.windows(2).enumerate() {
        let len = cumulative[i + 1] - cumulative[i];
        for c in world.hydrology.crossings(pair[0], pair[1]) {
            let at = cumulative[i] + len * c.t;
            if crossings.last().is_some_and(|&(last, _)| at - last < 45.) {
                continue;
            }
            crossings.push((at, c.hit));
        }
    }
    let mut out = vec![a];
    let mut cursor = 0.;
    for (at, river) in crossings {
        let span = river.width * 2.15 + 12.;
        let approach = (span * 1.6 + 60.).max(125.);
        let from = (at - approach).max(0.);
        let to = (at + approach).min(total);
        if from < cursor || from < 15. || to > total - 15. {
            continue;
        }
        for (i, &d) in cumulative.iter().enumerate() {
            if d > cursor && d < from {
                out.push(smooth_path[i]);
            }
        }
        let start = point_at(&smooth_path, from / total);
        let end = point_at(&smooth_path, to / total);
        let travel = normalize([end[0] - start[0], end[1] - start[1]]);
        let mut normal = [river.tangent[1], -river.tangent[0]];
        if travel[0] * normal[0] + travel[1] * normal[1] < 0. {
            normal = [-normal[0], -normal[1]];
        }
        let before = [
            river.point[0] - normal[0] * span,
            river.point[1] - normal[1] * span,
        ];
        let after = [
            river.point[0] + normal[0] * span,
            river.point[1] + normal[1] * span,
        ];
        out.push(start);
        curve(&mut out, start, before, travel, normal, 30.);
        out.push(after);
        curve(&mut out, after, end, normal, travel, 30.);
        cursor = to;
    }
    for (i, &d) in cumulative.iter().enumerate() {
        if d > cursor {
            out.push(smooth_path[i]);
        }
    }
    out.dedup_by(|a, b| distance2(*a, *b) < 0.01);
    for p in &mut out {
        p[0] = p[0].clamp(-HALF_WORLD, HALF_WORLD);
        p[1] = p[1].clamp(-HALF_WORLD, HALF_WORLD);
    }
    out
}
// Append this module to roads.rs.
#[cfg(test)]
mod road_network_tests {
    use super::*;
    #[test]
    fn town_backbone_is_connected_sparse_and_non_cardinal() {
        for seed in [1337, 42, 2026] {
            let world = World::new(seed);
            let network = &world.roads;
            let mut union = Union::new(network.nodes.len());
            let mut ids = HashSet::new();
            let mut non_cardinal = 0;
            let mut degree = vec![0usize; network.nodes.len()];
            for edge in &network.edges {
                assert!(ids.insert(edge.id));
                assert!(edge.id < (1_u64 << 53));
                assert_eq!(
                    network.nodes[edge.a].landmass, network.nodes[edge.b].landmass,
                    "inter-island road"
                );
                assert!(
                    land_path(&world, &edge.corridor, network.nodes[edge.a].landmass),
                    "coarse ocean shortcut"
                );
                if edge.kind == RoadKind::Main {
                    union.join(edge.a, edge.b);
                }
                degree[edge.a] += 1;
                if edge.join.is_none() {
                    degree[edge.b] += 1;
                }
                let dx = (edge.ends[1][0] - edge.ends[0][0]).abs();
                let dz = (edge.ends[1][1] - edge.ends[0][1]).abs();
                if dx.min(dz) / dx.hypot(dz) > 0.208 {
                    non_cardinal += 1;
                }
            }
            let towns: Vec<_> = network
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| (n.kind == 0).then_some(i))
                .collect();
            let mut by_landmass: HashMap<u32, Vec<usize>> = HashMap::new();
            for &town in &towns {
                by_landmass
                    .entry(network.nodes[town].landmass)
                    .or_default()
                    .push(town);
            }
            assert!(
                by_landmass.len() >= 2,
                "offshore regions need their own settlement networks"
            );
            for (landmass, regional_towns) in &by_landmass {
                let mut components = HashMap::new();
                for &town in regional_towns {
                    *components.entry(union.root(town)).or_insert(0usize) += 1;
                }
                let largest = *components.values().max().unwrap();
                assert!(
                    largest * 100 >= regional_towns.len() * 94,
                    "seed{seed} landmass{landmass}: only {largest}/{} towns connected",
                    regional_towns.len()
                );
            }
            assert!(
                network.stats.main_roads + by_landmass.len()
                    >= towns.len().saturating_sub(towns.len() / 20)
            );
            assert!(network.stats.main_roads <= towns.len() + towns.len() / 16);
            assert!(
                network.edges.len() * 2 < network.nodes.len(),
                "network is too dense"
            );
            let isolated = degree.iter().filter(|&&n| n == 0).count();
            assert_eq!(isolated, network.stats.isolated_sites);
            assert!(isolated as f32 / network.nodes.len() as f32 > 0.45);
            let organic = non_cardinal as f32 / network.edges.len() as f32;
            println!("seed{seed} {:?}; non-cardinal{organic:.3}", network.stats);
            assert!(organic > 0.50, "too many roads still follow cardinal axes");
        }
    }

    #[test]
    fn rural_lanes_join_exact_main_routes_and_queries_cover_route_vertices() {
        let world = World::new(1337);
        let network = &world.roads;
        let mut joins = 0;
        let mut checked = 0;
        for index in network.candidates(-16500., -12400., 14000.) {
            let edge = &network.edges[index];
            let route = network.route(&world, index);
            assert_eq!(route.points[0], edge.ends[0]);
            let endpoint = if let Some((parent, _)) = edge.join {
                joins += 1;
                assert_eq!(network.edges[parent].kind, RoadKind::Main);
                assert_eq!(edge.kind, RoadKind::Lane);
                closest_path_point(&network.route(&world, parent).points, edge.ends[1])
            } else {
                edge.ends[1]
            };
            assert!(distance2(*route.points.last().unwrap(), endpoint) < 0.001);
            for &p in route.points.iter().step_by(11) {
                assert!(p.iter().all(|v| v.is_finite() && v.abs() <= HALF_WORLD));
                assert!(
                    network.buckets[bucket(p[1]) * GRAPH_N + bucket(p[0])].contains(&index),
                    "route left spatial envelope"
                );
                assert!(
                    world.sample(p[0], p[1]).road > 0.999,
                    "route not found at its own vertex {p:?}"
                );
                checked += 1;
            }
        }
        println!("lane joins{} routevertexchecks{}", joins, checked);
        assert!(joins >= 2 && checked > 300);
    }

    #[test]
    fn sample_is_independent_of_route_query_order() {
        let a = World::new(1337);
        let b = World::new(1337);
        let mut points = Vec::new();
        for route in a.road_routes_near(-16500., -12400., 10000.) {
            for pair in route.points.windows(2).step_by(9) {
                let direction = normalize([pair[1][0] - pair[0][0], pair[1][1] - pair[0][1]]);
                for offset in [
                    0.0,
                    route.kind.half_width(),
                    route.kind.outer_width() + 0.15,
                ] {
                    points.push([
                        (pair[0][0] + pair[1][0]) * 0.5 + direction[1] * offset,
                        (pair[0][1] + pair[1][1]) * 0.5 - direction[0] * offset,
                    ]);
                }
            }
        }
        let mut reversed: Vec<_> = points.iter().rev().map(|p| b.sample(p[0], p[1])).collect();
        reversed.reverse();
        for (p, expected) in points.iter().zip(reversed) {
            let actual = a.sample(p[0], p[1]);
            assert_eq!(
                actual.height.to_bits(),
                expected.height.to_bits(),
                "query order changed height at {p:?}"
            );
            assert_eq!(
                actual.road.to_bits(),
                expected.road.to_bits(),
                "query order changed road mask at {p:?}"
            );
            assert_eq!(actual.road_kind, expected.road_kind);
        }
        println!("queryorderpoints{}", points.len());
        assert!(points.len() > 300);
    }

    #[test]
    fn atlas_background_and_continental_summaries_do_not_route_the_world() {
        let world = World::new(42);
        assert!(world.roads.cache.borrow().is_empty());
        assert!(!world.road_map_routes(0., 0., WORLD_SIZE).is_empty());
        assert!(world.roads.cache.borrow().is_empty());
        assert_eq!(
            world.map_background_rgba(0., 0., WORLD_SIZE, 128).len(),
            128 * 128 * 4
        );
        assert!(world.roads.cache.borrow().is_empty());
        assert!(world
            .road_map_routes(0., 0., 60000.)
            .iter()
            .all(|r| r.kind == RoadKind::Main));
        assert!(world
            .road_map_routes(0., 0., 24000.)
            .iter()
            .all(|r| r.kind != RoadKind::Trail));
    }
    #[test]
    fn ocean_boundaries_hold_for_coarse_and_fine_routes() {
        let world = World::new(1337);
        let network = &world.roads;
        assert_eq!(GRAPH_N, (WORLD_SIZE / GRAPH_CELL) as usize + 2);
        assert!(
            network
                .nodes
                .iter()
                .any(|n| n.cell[0].abs() > 54 || n.cell[1].abs() > 54),
            "larger-world nodes were clipped to old bounds"
        );
        let mut detours = 0;
        let mut coastal = Vec::new();
        for (i, edge) in network.edges.iter().enumerate() {
            let landmass = network.nodes[edge.a].landmass;
            assert!(land_path(&world, &edge.corridor, landmass));
            if edge.corridor.len() > 2 {
                detours += 1;
                coastal.push(i);
            } else if edge
                .corridor
                .iter()
                .any(|p| world.coast_info(p[0], p[1]).distance < 1800.)
            {
                coastal.push(i);
            }
        }
        let mut fine_points = 0;
        for &index in coastal.iter().take(32) {
            let edge = &network.edges[index];
            let route = network.route(&world, index);
            assert!(route.points.len() >= 2, "coastal road lost its endpoint");
            assert!(land_path(
                &world,
                &route.points,
                network.nodes[edge.a].landmass
            ));
            for pair in route.points.windows(2) {
                let steps = (distance2(pair[0], pair[1]).sqrt() / 8.).ceil().max(1.) as usize;
                for n in 0..=steps {
                    let t = n as f32 / steps as f32;
                    let p = [
                        lerp(pair[0][0], pair[1][0], t),
                        lerp(pair[0][1], pair[1][1], t),
                    ];
                    assert!(
                        world.is_land(p[0], p[1]),
                        "fine road crosses ocean at {p:?}"
                    );
                    fine_points += 1;
                }
            }
        }
        assert!(fine_points > 100, "coastal routing was not exercised");
        let a = &network.nodes[0];
        let b = network
            .nodes
            .iter()
            .find(|b| b.landmass != a.landmass)
            .unwrap();
        assert!(
            land_corridor(&world, a.p, b.p, a.landmass).is_none(),
            "separate islands must not be joined"
        );
        let summary = world.road_map_routes(0., 0., WORLD_SIZE);
        for road in summary {
            let mass = world
                .landmass_id(road.points[0][0], road.points[0][1])
                .unwrap();
            assert!(
                land_path(&world, &road.points, mass),
                "atlas summary crossed ocean"
            );
        }
        println!(
            "coastal detours{detours} routed{} finechecks{fine_points}",
            coastal.len().min(32)
        );
    }

    #[test]
    fn expanded_cell_ids_are_exact_and_collision_free() {
        let mut seen = HashSet::new();
        for a in [0_u32, 16000, 16384, 24000, SITE_AXIS * SITE_AXIS - 2] {
            for b in [1_u32, 16383, 16385, 26000, SITE_AXIS * SITE_AXIS - 1] {
                if a >= b {
                    continue;
                }
                for kind in [RoadKind::Main, RoadKind::Lane, RoadKind::Trail] {
                    let id = edge_id(a, b, kind);
                    assert!(id < (1_u64 << 53));
                    assert!(seen.insert(id));
                    assert_eq!(id, edge_id(b, a, kind));
                }
            }
        }
    }
    #[test]
    fn coastal_astar_skirts_a_bay_instead_of_crossing_water() {
        let world = World::new(1337);
        let network = &world.roads;
        for node in network
            .nodes
            .iter()
            .filter(|n| world.coast_info(n.p[0], n.p[1]).distance < 2600.)
        {
            for distance in [3600., 6000., 8400.] {
                for direction in 0..16 {
                    let angle = direction as f32 * std::f32::consts::TAU / 16.;
                    let end = [
                        node.p[0] + angle.cos() * distance,
                        node.p[1] + angle.sin() * distance,
                    ];
                    let coast = world.coast_info(end[0], end[1]);
                    if coast.landmass_id != Some(node.landmass)
                        || coast.distance < 120.
                        || land_segment(&world, node.p, end, node.landmass)
                    {
                        continue;
                    }
                    if let Some(path) = land_corridor(&world, node.p, end, node.landmass) {
                        assert!(path.len() > 2);
                        assert!(land_path(&world, &path, node.landmass));
                        assert_eq!(path.first(), Some(&node.p));
                        assert_eq!(path.last(), Some(&end));
                        assert_eq!(
                            path,
                            land_corridor(&world, node.p, end, node.landmass).unwrap(),
                            "A* must be deterministic"
                        );
                        println!(
                            "A* coastal detour {:?} -> {:?}, {} anchors",
                            node.p,
                            end,
                            path.len()
                        );
                        return;
                    }
                }
            }
        }
        panic!("no coast-skirt case exercised A* on the generated mainland/islands");
    }
}
