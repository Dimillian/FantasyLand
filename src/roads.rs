//! A sparse settlement graph with a connected town backbone and optional rural
//! branches. Geometry is routed lazily; natural geography never depends on roads.
use super::{
    distance2, hash, lerp, rand01, segment_hit, smooth, RoadHit, World, HALF_WORLD, WORLD_SIZE,
};
use serde::Serialize;
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
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
}
const GRAPH_CELL: f32 = 2048.;
const GRAPH_N: usize = 126;
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
    // Node indices are below 16384. Packed indices are collision-free and
    // remain exact JavaScript numbers; legacy Site.id hashes can collide.
    ((a as u64) << 16) | ((b as u64) << 2) | kind as u64
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
        for i in -54..=54 {
            for j in -54..=54 {
                let p = world.node(i, j);
                if p[0].abs() > HALF_WORLD || p[1].abs() > HALF_WORLD {
                    continue;
                }
                let id = hash(world.seed ^ 0x4101, i, j);
                cells.insert((i, j), result.nodes.len());
                result.nodes.push(Node {
                    id,
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
                        if n != center {
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
            if union.join(a, b) {
                chosen.insert((a, b));
                edges.push(Edge {
                    a,
                    b,
                    kind: RoadKind::Main,
                    id: edge_id(a as u32, b as u32, RoadKind::Main),
                    join: None,
                    ends: [nodes[a].p, nodes[b].p],
                });
            }
        }
        // The local nearest-neighbor graph is normally connected. This bounded
        // fallback joins any rare boundary component without axial chains.
        while let Some(&a) = towns
            .iter()
            .find(|&&a| union.root(a) != union.root(towns[0]))
        {
            let root = union.root(a);
            let mut best = (f32::INFINITY, 0, 0);
            for &b in &towns {
                if union.root(b) != root {
                    continue;
                }
                for &c in &towns {
                    if union.root(c) == root {
                        continue;
                    }
                    let d = distance2(nodes[b].p, nodes[c].p);
                    if d < best.0 {
                        best = (d, b, c);
                    }
                }
            }
            let (_, a, b) = best;
            union.join(a, b);
            chosen.insert((a.min(b), a.max(b)));
            edges.push(Edge {
                a,
                b,
                kind: RoadKind::Main,
                id: edge_id(a as u32, b as u32, RoadKind::Main),
                join: None,
                ends: [nodes[a].p, nodes[b].p],
            });
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
            edges.push(Edge {
                a,
                b,
                kind: RoadKind::Main,
                id: edge_id(a as u32, b as u32, RoadKind::Main),
                join: None,
                ends: [nodes[a].p, nodes[b].p],
            });
        }
        let main_count = edges.len();
        let mut connected = vec![false; nodes.len()];
        for &n in &towns {
            connected[n] = true;
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
            let Some(&b) = near.first() else { continue };
            let mut endpoint = nodes[b].p;
            let mut parent = None;
            let mut best = distance2(nodes[a].p, endpoint).sqrt();
            // A lane may terminate at a genuine T-junction on a main road,
            // rather than sending every village to a town-center star.
            for (index, e) in edges[..main_count].iter().enumerate() {
                if nodes[a].p[0] < e.ends[0][0].min(e.ends[1][0]) - best
                    || nodes[a].p[0] > e.ends[0][0].max(e.ends[1][0]) + best
                    || nodes[a].p[1] < e.ends[0][1].min(e.ends[1][1]) - best
                    || nodes[a].p[1] > e.ends[0][1].max(e.ends[1][1]) + best
                {
                    continue;
                }
                let hit = segment_hit(nodes[a].p, e.ends[0], e.ends[1]);
                let t = (distance2(e.ends[0], hit.point) / distance2(e.ends[0], e.ends[1]).max(1.))
                    .sqrt();
                if hit.distance < best * 0.80 && (0.12..0.88).contains(&t) {
                    best = hit.distance;
                    endpoint = hit.point;
                    parent = Some((index, t));
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
                id: edge_id(a as u32, b as u32, RoadKind::Lane),
                join: parent,
                ends: [nodes[a].p, endpoint],
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
            let Some(&b) = near.first() else { continue };
            if distance2(nodes[a].p, nodes[b].p) > 5200. * 5200. {
                continue;
            }
            connected[a] = true;
            edges.push(Edge {
                a,
                b,
                kind: RoadKind::Trail,
                id: edge_id(a as u32, b as u32, RoadKind::Trail),
                join: None,
                ends: [nodes[a].p, nodes[b].p],
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
                .map(|e| distance2(e.ends[0], e.ends[1]).sqrt() / 1000.)
                .sum(),
        };
        for (i, e) in edges.iter().enumerate() {
            for z in bucket(e.ends[0][1].min(e.ends[1][1]) - ROUTE_PAD)
                ..=bucket(e.ends[0][1].max(e.ends[1][1]) + ROUTE_PAD)
            {
                for x in bucket(e.ends[0][0].min(e.ends[1][0]) - ROUTE_PAD)
                    ..=bucket(e.ends[0][0].max(e.ends[1][0]) + ROUTE_PAD)
                {
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
        let end = if let Some((parent, t)) = e.join {
            point_at(&self.route(world, parent).points, t)
        } else {
            e.ends[1]
        };
        let road = Rc::new(Road {
            id: e.id,
            kind: e.kind,
            points: plan(
                world,
                e.ends[0],
                end,
                route_seed(self.nodes[e.a].id, self.nodes[e.b].id, e.kind),
                e.kind,
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
                    (segment_hit([cx, cz], e.ends[0], e.ends[1]).distance <= radius).then(|| Road {
                        id: e.id,
                        kind: e.kind,
                        points: e.ends.to_vec(),
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
fn plan(world: &World, a: [f32; 2], b: [f32; 2], seed: u32, kind: RoadKind) -> Vec<[f32; 2]> {
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
            let root = union.root(towns[0]);
            assert!(towns.iter().all(|&node| union.root(node) == root));
            assert!(network.stats.main_roads >= towns.len() - 1);
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
            let endpoint = if let Some((parent, t)) = edge.join {
                joins += 1;
                assert_eq!(network.edges[parent].kind, RoadKind::Main);
                assert_eq!(edge.kind, RoadKind::Lane);
                point_at(&network.route(&world, parent).points, t)
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
}
