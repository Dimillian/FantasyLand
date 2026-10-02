//! Authoritative, seeded settlements. Catalog -> road graph -> lazy parcels.
//! Generation only samples natural ground: never call collision or graded terrain here.
use crate::world::{hash, rand01, Site, World, WORLD_SIZE};
use serde::Serialize;
use std::f32::consts::{PI, TAU};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Camp,
    Hamlet,
    Fort,
    Village,
    Town,
    City,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Camp => "camp",
            Self::Hamlet => "hamlet",
            Self::Fort => "fort",
            Self::Village => "village",
            Self::Town => "town",
            Self::City => "city",
        }
    }
    pub fn radius(self) -> f32 {
        match self {
            Self::Camp => 34.,
            Self::Hamlet => 53.,
            Self::Fort => 83.,
            Self::Village => 90.,
            Self::Town => 160.,
            Self::City => 290.,
        }
    }
    fn count(self, id: u32) -> usize {
        match self {
            Self::Camp => 3 + (id % 3) as usize,
            Self::Hamlet => 4 + (id % 5) as usize,
            Self::Fort => 12 + (id % 7) as usize,
            Self::Village => 14 + (id % 12) as usize,
            Self::Town => 46 + (id % 30) as usize,
            Self::City => 170 + (id % 61) as usize,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub site: Site,
    pub cell: [i32; 2],
    pub kind: Kind,
    pub region: String,
    pub capital: bool,
    pub radius: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Use {
    Home,
    Inn,
    Smithy,
    Market,
    Guild,
    Arcane,
    Barracks,
    Temple,
    Hall,
    Stable,
    Tent,
    Watchtower,
}
impl Use {
    pub fn name(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Inn => "inn",
            Self::Smithy => "smithy",
            Self::Market => "market",
            Self::Guild => "fighters' guild",
            Self::Arcane => "arcanists' hall",
            Self::Barracks => "barracks",
            Self::Temple => "temple",
            Self::Hall => "civic hall",
            Self::Stable => "stable",
            Self::Tent => "tent",
            Self::Watchtower => "watch house",
        }
    }
    pub fn public(self) -> bool {
        !matches!(self, Self::Home | Self::Tent)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Building {
    pub id: u32,
    pub name: String,
    pub usage: Use,
    pub x: f32,
    pub z: f32,
    pub floor: f32,
    pub yaw: f32,
    pub half: [f32; 2],
    pub height: f32,
    pub door: bool,
    pub district: String,
    pub street_node: usize,
    pub porch_node: usize,
    pub room_node: usize,
}
impl Building {
    pub fn point(&self, x: f32, y: f32, z: f32) -> [f32; 3] {
        let (s, c) = self.yaw.sin_cos();
        [
            self.x + x * c + z * s,
            self.floor + y,
            self.z - x * s + z * c,
        ]
    }
    pub fn local(&self, x: f32, z: f32) -> [f32; 2] {
        let (s, c) = self.yaw.sin_cos();
        [
            (x - self.x) * c - (z - self.z) * s,
            (x - self.x) * s + (z - self.z) * c,
        ]
    }
    pub fn inside(&self, x: f32, z: f32, margin: f32) -> bool {
        let p = self.local(x, z);
        p[0].abs() < self.half[0] + margin && p[1].abs() < self.half[1] + margin
    }
    /// Separating-axis test includes eaves and a narrow maintenance gap.
    pub fn overlaps_plot(&self, other: &Building, margin: f32) -> bool {
        let (sa, ca) = self.yaw.sin_cos();
        let (sb, cb) = other.yaw.sin_cos();
        let axes = [[ca, -sa], [sa, ca], [cb, -sb], [sb, cb]];
        axes.into_iter().all(|axis| {
            let projected = |(s, c): (f32, f32), half: [f32; 2]| {
                (axis[0] * c - axis[1] * s).abs() * half[0]
                    + (axis[0] * s + axis[1] * c).abs() * half[1]
            };
            let gap = ((other.x - self.x) * axis[0] + (other.z - self.z) * axis[1]).abs();
            gap < projected((sa, ca), self.half) + projected((sb, cb), other.half) + margin
        })
    }
    pub fn entrance(&self) -> [f32; 3] {
        self.point(0., 0., -self.half[1] - 1.1)
    }
    /// One geometry contract for bedroom walls, navigation and lighting.
    pub fn partition(&self) -> Option<f32> {
        matches!(
            self.usage,
            Use::Home | Use::Inn | Use::Barracks | Use::Guild | Use::Arcane
        )
        .then_some((self.half[1] * 0.22).max(1.18))
    }
    pub fn wall_material(&self) -> (f32, [f32; 3]) {
        match self.id % 5 {
            0 => (20.0, [0.66, 0.37, 0.25]),
            1 => (21.0, [0.48, 0.33, 0.20]),
            2 => (17.0, [0.59, 0.58, 0.51]),
            3 => (18.0, [0.72, 0.65, 0.48]),
            _ => (18.0, [0.61, 0.65, 0.58]),
        }
    }
    pub fn roof_rise(&self) -> f32 {
        2.25 + (self.id % 4) as f32 * 0.35
    }
    pub fn hearth(&self) -> [f32; 4] {
        let p = if self.usage == Use::Tent {
            self.point(0.0, 0.5, self.half[1] + 0.85)
        } else {
            self.point(-self.half[0] + 1.0, 0.72, self.hearth_z())
        };
        [p[0], p[1], p[2], 7.5]
    }
    /// Keep the left window in the front living bay, away from the hearth/flue.
    /// The analytic indoor-light probe uses the same half-depth placement.
    pub fn window_z(&self, side: f32) -> f32 {
        if side < 0. {
            -self.half[1] * 0.5
        } else {
            0.
        }
    }
    pub fn hearth_z(&self) -> f32 {
        self.partition().map_or(self.half[1] - 0.8, |p| p - 1.45)
    }
    /// Shared hanging fixture position, safely above the central walking route.
    pub fn lamp_local(&self) -> [f32; 3] {
        [0., (self.height - 0.65).min(3.4), -self.half[1] * 0.38]
    }
    pub fn lamps(&self) -> Vec<[f32; 4]> {
        if matches!(self.usage, Use::Tent | Use::Stable | Use::Market) {
            return vec![];
        }
        let q = self.lamp_local();
        let p = self.point(q[0], q[1] + 0.08, q[2]);
        let mut lights = vec![[p[0], p[1], p[2], 6.0]];
        if self.partition().is_some() {
            for side in [-1., 1.] {
                let p = self.point(side * (self.half[0] - 2.24), 0.90, self.half[1] - 0.7);
                lights.push([p[0], p[1], p[2], 3.0]);
            }
        }
        lights
    }
    pub fn lights(&self) -> Vec<[f32; 4]> {
        if matches!(self.usage, Use::Stable | Use::Market) {
            return vec![];
        }
        let mut lights = vec![self.hearth()];
        lights.extend(self.torch());
        lights.extend(self.lamps());
        lights
    }
    pub fn torch(&self) -> Option<[f32; 4]> {
        if !matches!(
            self.usage,
            Use::Inn | Use::Hall | Use::Temple | Use::Barracks | Use::Arcane | Use::Guild
        ) {
            return None;
        }
        let p = self.point(-self.half[0] + 0.42, 2.15, -self.half[1] + 1.25);
        Some([p[0], p[1], p[2], 5.0])
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Street {
    pub points: Vec<[f32; 2]>,
    pub width: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Layout {
    pub entry: Entry,
    pub buildings: Vec<Building>,
    pub walls: Vec<Building>,
    pub streets: Vec<Street>,
    pub nodes: Vec<[f32; 3]>,
    pub links: Vec<Vec<usize>>,
    pub population: usize,
    #[serde(skip)]
    pub parcels: HashMap<(i32, i32), Vec<usize>>,
    #[serde(skip)]
    pub lanes: HashMap<(i32, i32), Vec<([f32; 2], [f32; 2], f32)>>,
}
impl Layout {
    pub fn path(&self, start: usize, end: usize) -> Vec<[f32; 3]> {
        if start >= self.nodes.len() || end >= self.nodes.len() {
            return vec![];
        }
        let mut prev = vec![usize::MAX; self.nodes.len()];
        let mut cost = vec![f32::INFINITY; self.nodes.len()];
        let mut q = std::collections::BinaryHeap::new();
        cost[start] = 0.;
        q.push(std::cmp::Reverse((0u32, start)));
        prev[start] = start;
        while let Some(std::cmp::Reverse((bits, a))) = q.pop() {
            let distance = f32::from_bits(bits);
            if distance > cost[a] {
                continue;
            }
            if a == end {
                break;
            }
            for &b in &self.links[a] {
                let p = self.nodes[a];
                let v = self.nodes[b];
                let next = distance + dist([p[0], p[2]], [v[0], v[2]]);
                if next < cost[b] {
                    cost[b] = next;
                    prev[b] = a;
                    q.push(std::cmp::Reverse((next.to_bits(), b)));
                }
            }
        }
        if prev[end] == usize::MAX {
            return vec![];
        }
        let mut ids = vec![end];
        while *ids.last().unwrap() != start {
            ids.push(prev[*ids.last().unwrap()]);
        }
        ids.reverse();
        ids.into_iter().map(|i| self.nodes[i]).collect()
    }
    pub fn closest_node(&self, p: [f32; 2]) -> usize {
        self.nodes
            .iter()
            .enumerate()
            .min_by(|a, b| dist([a.1[0], a.1[2]], p).total_cmp(&dist([b.1[0], b.1[2]], p)))
            .map_or(0, |a| a.0)
    }
}
#[derive(Debug, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    buckets: HashMap<(i32, i32), Vec<usize>>,
    layouts: RefCell<HashMap<u32, Rc<Layout>>>,
}
fn bucket(p: f32) -> i32 {
    (p / 2400.).floor() as i32
}
pub fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn r(id: u32, n: i32) -> f32 {
    rand01(hash(id, n, 731))
}
impl Catalog {
    pub fn new(world: &World) -> Self {
        let mut result = Self::default();
        let extent = (WORLD_SIZE * 0.5 / 2400.) as i32 + 1;
        for i in -extent..=extent {
            for j in -extent..=extent {
                let id = hash(world.seed ^ 0x4101, i, j);
                if hash(id, 2, 19) % 100 >= 58 {
                    continue;
                }
                let p = world.node(i, j);
                let s = world.natural_sample(p[0], p[1]);
                if p[0].abs() > WORLD_SIZE * 0.5 - 600.
                    || p[1].abs() > WORLD_SIZE * 0.5 - 600.
                    || s.ocean
                    || s.height < s.water_height + 2.
                    || world.coast_info(p[0], p[1]).distance < 150.
                {
                    continue;
                }
                let heights = [[28., 0.], [-28., 0.], [0., 28.], [0., -28.]]
                    .map(|d| world.natural_sample(p[0] + d[0], p[1] + d[1]).height);
                let relief = heights.into_iter().fold(s.height, f32::max)
                    - heights.into_iter().fold(s.height, f32::min);
                if relief > 15. || s.height > 2250. {
                    continue;
                }
                let n = hash(id ^ 0x4371, 0, 0) % 100;
                let kind = if n < 10 && relief < 7. {
                    Kind::Town
                } else if n < 35 {
                    Kind::Village
                } else if n < 41 {
                    Kind::Fort
                } else if n < 73 {
                    Kind::Hamlet
                } else {
                    Kind::Camp
                };
                let mut site = world.site(i, j);
                site.kind = kind.name().into();
                site.id = ((i + extent) * (extent * 2 + 1) + (j + extent)) as u32;
                let region_id = hash(
                    world.seed ^ 781,
                    (p[0] / 48000.).floor() as i32,
                    (p[1] / 48000.).floor() as i32,
                );
                let region = format!(
                    "{} {}",
                    ["Alder", "Crown", "Ember", "Silver", "Thorn", "Amber", "Grey", "Dawn"]
                        [(region_id % 8) as usize],
                    ["March", "Reach", "Vale", "Weald", "Province", "Highlands"]
                        [(region_id / 8 % 6) as usize]
                );
                result.entries.push(Entry {
                    site,
                    cell: [i, j],
                    kind,
                    region,
                    capital: false,
                    radius: 420.,
                });
            }
        }
        // One capital per broad inhabited province; not one giant city per site cell.
        let mut capitals: HashMap<(i32, i32), (usize, f32)> = HashMap::new();
        for (i, e) in result
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == Kind::Town)
        {
            let key = (
                (e.site.x / 48000.).floor() as i32,
                (e.site.z / 48000.).floor() as i32,
            );
            let mut low = f32::MAX;
            let mut high = f32::MIN;
            let mut dry = true;
            for radius in [80., 180., 270.] {
                for n in 0..12 {
                    let angle = n as f32 * TAU / 12.;
                    let s = world.natural_sample(
                        e.site.x + radius * angle.cos(),
                        e.site.z + radius * angle.sin(),
                    );
                    low = low.min(s.height);
                    high = high.max(s.height);
                    dry &= !s.ocean && s.height > s.water_height + 2.;
                }
            }
            let score = high - low + rand01(hash(e.site.id, 7, 9)) * 8.;
            if dry && score < 85. && capitals.get(&key).is_none_or(|&(_, old)| score < old) {
                capitals.insert(key, (i, score));
            }
        }
        for (i, _) in capitals.into_values() {
            let e = &mut result.entries[i];
            e.kind = Kind::City;
            e.site.kind = "city".into();
            e.capital = true;
            e.radius = 420.;
        }
        for (i, e) in result.entries.iter().enumerate() {
            result
                .buckets
                .entry((bucket(e.site.x), bucket(e.site.z)))
                .or_default()
                .push(i);
        }
        result
    }
    /// Allocation-free hinterland lookup; no lazy building generation.
    pub fn rural_owner(&self, x: f32, z: f32) -> Option<&Entry> {
        let mut best = None;
        let mut score = f32::MAX;
        for i in bucket(x - 1550.)..=bucket(x + 1550.) {
            for j in bucket(z - 1550.)..=bucket(z + 1550.) {
                if let Some(ids) = self.buckets.get(&(i, j)) {
                    for &id in ids {
                        let e = &self.entries[id];
                        let reach = crate::countryside::reach(e);
                        let d = dist([x, z], [e.site.x, e.site.z]);
                        if d < e.radius + 22. {
                            return None;
                        }
                        if d < reach && d > e.radius + 22. && d / reach < score {
                            score = d / reach;
                            best = Some(e);
                        }
                    }
                }
            }
        }
        best
    }
    pub fn near(&self, x: f32, z: f32, radius: f32) -> Vec<&Entry> {
        if !radius.is_finite() || radius < 0. {
            return vec![];
        }
        if radius > 20000. {
            return self
                .entries
                .iter()
                .filter(|e| dist([x, z], [e.site.x, e.site.z]) <= radius)
                .collect();
        }
        let mut out = vec![];
        for i in bucket(x - radius)..=bucket(x + radius) {
            for j in bucket(z - radius)..=bucket(z + radius) {
                if let Some(ids) = self.buckets.get(&(i, j)) {
                    for &id in ids {
                        let e = &self.entries[id];
                        if dist([x, z], [e.site.x, e.site.z]) <= radius {
                            out.push(e);
                        }
                    }
                }
            }
        }
        out
    }
    pub fn layout(&self, world: &World, id: u32) -> Option<Rc<Layout>> {
        if let Some(v) = self.layouts.borrow().get(&id) {
            return Some(v.clone());
        }
        let e = self.entries.iter().find(|e| e.site.id == id)?;
        let layout = Rc::new(generate(world, e.clone()));
        let mut cache = self.layouts.borrow_mut();
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(id, layout.clone());
        Some(layout)
    }
    pub fn layouts_near(&self, world: &World, x: f32, z: f32, radius: f32) -> Vec<Rc<Layout>> {
        self.near(x, z, radius + 440.)
            .into_iter()
            .filter(|e| dist([x, z], [e.site.x, e.site.z]) < e.radius + radius)
            .filter_map(|e| self.layout(world, e.site.id))
            .collect()
    }
    pub fn clears(&self, x: f32, z: f32, margin: f32) -> bool {
        self.near(x, z, 440. + margin)
            .iter()
            .any(|e| dist([x, z], [e.site.x, e.site.z]) < e.radius + margin)
    }
}
fn add_node(world: &World, l: &mut Layout, p: [f32; 2]) -> usize {
    let i = l.nodes.len();
    l.nodes
        .push([p[0], world.natural_sample(p[0], p[1]).height + 0.12, p[1]]);
    l.links.push(vec![]);
    i
}
fn link(l: &mut Layout, a: usize, b: usize) {
    l.links[a].push(b);
    l.links[b].push(a);
}
// Seeded, continuous ward deformation. Apply to lanes and parcels together:
// unlike per-building jitter it preserves the street-facing layout.
fn ward_point(id: u32, center: [f32; 2], radius: f32, angle: f32) -> [f32; 2] {
    let phase = r(id, 902) * TAU;
    let turn = r(id, 903) * TAU + 0.22 * (radius / 58. + phase).sin();
    let a = angle + turn;
    let radius = radius * (1. + 0.10 * (angle * 2. + phase).sin())
        + 3.0 * (angle * 3. + radius / 80. + phase).sin() * (radius / 40.).min(1.);
    [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
}
fn ward_axis_distance(id: u32, center: [f32; 2], p: [f32; 2], radius: f32) -> f32 {
    (0..4)
        .map(|axis| {
            let q = ward_point(id, center, radius, axis as f32 * PI * 0.5);
            dist(p, q)
        })
        .fold(f32::INFINITY, f32::min)
}
fn generate(world: &World, e: Entry) -> Layout {
    let mut l = Layout {
        entry: e.clone(),
        buildings: vec![],
        walls: vec![],
        streets: vec![],
        nodes: vec![],
        links: vec![],
        population: 0,
        parcels: HashMap::new(),
        lanes: HashMap::new(),
    };
    let id = e.site.id;
    let center = [e.site.x, e.site.z];
    add_node(world, &mut l, center);
    let wanted = e.kind.count(id);
    let routes = world.road_routes_near(center[0], center[1], e.radius + 80.);
    // Organic concentric neighbourhood lanes. Each ring has one shared walkable
    // lane and several radial links, avoiding roads through building parcels.
    let mut ring = 0usize;
    let mut previous: Vec<usize> = vec![0; 4];
    while l.buildings.len() < wanted && ring < 15 {
        let radius = if e.kind == Kind::Camp {
            14.
        } else {
            25. + ring as f32 * 26.
        };
        let count = ((TAU * radius / 18.) as usize).max(5);
        let mut current = vec![];
        let mut ring_points = vec![];
        for slot in 0..count {
            let a = slot as f32 / count as f32 * TAU + r(id, ring as i32 + 110) * 0.4;
            let radial = radius + 2. * (a * 3. + r(id, 4) * TAU).sin();
            let path_p = ward_point(id, center, radial - 10., a);
            let node = add_node(world, &mut l, path_p);
            current.push(node);
            ring_points.push(path_p);
            let p = ward_point(id, center, radial, a);
            if l.buildings.len() >= wanted {
                continue;
            }
            let bi = l.buildings.len();
            let bid = id * 512 + bi as u32;
            let usage = if e.kind == Kind::Camp {
                Use::Tent
            } else if e.kind == Kind::Hamlet {
                if bi == 0 && id % 3 == 0 {
                    Use::Inn
                } else {
                    Use::Home
                }
            } else if e.kind == Kind::Fort && bi < 4 {
                [Use::Barracks, Use::Watchtower, Use::Smithy, Use::Stable][bi]
            } else {
                match bi {
                    0 => Use::Inn,
                    1 => Use::Market,
                    2 => Use::Smithy,
                    3 if wanted > 10 => Use::Temple,
                    4 if wanted > 30 => Use::Hall,
                    5 if wanted > 30 => Use::Guild,
                    6 if wanted > 30 => Use::Arcane,
                    7 if wanted > 10 => Use::Stable,
                    8 if wanted > 30 => Use::Barracks,
                    _ => Use::Home,
                }
            };
            let half = if usage == Use::Tent {
                [2.8, 3.3]
            } else if matches!(
                usage,
                Use::Inn | Use::Hall | Use::Barracks | Use::Arcane | Use::Guild
            ) {
                [5.0, 6.0]
            } else {
                [3.4 + r(bid, 4) * 0.9, 4.2 + r(bid, 5) * 1.0]
            };
            let yaw = (p[0] - path_p[0]).atan2(p[1] - path_p[1]);
            let mut b = Building {
                id: bid,
                name: String::new(),
                usage,
                x: p[0],
                z: p[1],
                floor: 0.,
                yaw,
                half,
                height: match usage {
                    Use::Hall | Use::Temple => 6.4,
                    Use::Watchtower => 7.0,
                    _ => 3.3 + r(bid, 6) * 0.6,
                },
                door: !matches!(usage, Use::Market | Use::Stable | Use::Tent),
                district: if ring < 2 {
                    "Market quarter"
                } else if ring < 5 {
                    "Craftsmen's ward"
                } else {
                    "Outer ward"
                }
                .into(),
                street_node: node,
                porch_node: 0,
                room_node: 0,
            };
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            let mut dry = true;
            for x in [-half[0] - 1., 0., half[0] + 1.] {
                for z in [-half[1] - 1., 0., half[1] + 1.] {
                    let q = b.point(x, 0., z);
                    let s = world.natural_sample(q[0], q[2]);
                    lo = lo.min(s.height);
                    hi = hi.max(s.height);
                    dry &= !s.ocean && s.height > s.water_height + 0.6;
                }
            }
            if !dry
                || hi - lo > 4.
                || (e.kind != Kind::Camp && ward_axis_distance(id, center, p, radial) < 11.)
            {
                continue;
            }
            // Fine road geometry, not a distance to a settlement marker, owns the
            // approach corridor. Buildings may not straddle an existing road.
            if routes.iter().any(|route| {
                route.points.windows(2).any(|s| {
                    let a = b.local(s[0][0], s[0][1]);
                    let c = b.local(s[1][0], s[1][1]);
                    segment_rect(a, c, [half[0] + 5., half[1] + 5.])
                })
            }) {
                continue;
            }
            if l.buildings.iter().any(|other| b.overlaps_plot(other, 1.3)) {
                continue;
            }
            b.floor = hi + 0.16;
            b.name = match usage {
                Use::Home => format!("{} {}", bi + 1, b.district),
                Use::Inn => format!(
                    "The {} {}",
                    [
                        "Copper",
                        "Sleeping",
                        "Amber",
                        "Wandering",
                        "Silver",
                        "Crowned"
                    ][(bid % 6) as usize],
                    ["Fox", "Hart", "Lantern", "Oak", "Stag", "Hare"][(bid / 6 % 6) as usize]
                ),
                _ => format!("{} {}", e.site.name, usage.name()),
            };
            let porch = b.entrance();
            b.porch_node = l.nodes.len();
            l.nodes.push(porch);
            l.links.push(vec![node]);
            l.links[node].push(b.porch_node);
            b.room_node = l.nodes.len();
            l.nodes.push(b.point(
                0.,
                0.,
                if usage == Use::Home {
                    half[1] * 0.5
                } else {
                    -half[1] + 2.
                },
            ));
            l.links.push(vec![b.porch_node]);
            l.links[b.porch_node].push(b.room_node);
            l.streets.push(Street {
                points: vec![path_p, [porch[0], porch[2]]],
                width: 1.5,
            });
            l.buildings.push(b);
        }
        for n in 0..current.len() {
            link(&mut l, current[n], current[(n + 1) % current.len()]);
        }
        let mut spokes = vec![];
        for axis in 0..4 {
            let a = axis as f32 * PI * 0.5;
            let p = ward_point(id, center, radius - 10., a);
            let n = add_node(world, &mut l, p);
            let near = *current
                .iter()
                .min_by(|&&x, &&y| {
                    dist([l.nodes[x][0], l.nodes[x][2]], p)
                        .total_cmp(&dist([l.nodes[y][0], l.nodes[y][2]], p))
                })
                .unwrap();
            link(&mut l, n, near);
            link(&mut l, n, previous[axis]);
            l.streets.push(Street {
                points: vec![p, [l.nodes[previous[axis]][0], l.nodes[previous[axis]][2]]],
                width: 2.4,
            });
            l.streets.push(Street {
                points: vec![p, [l.nodes[near][0], l.nodes[near][2]]],
                width: 2.,
            });
            spokes.push(n);
        }
        ring_points.push(ring_points[0]);
        l.streets.push(Street {
            points: ring_points,
            width: if ring < 2 { 3.2 } else { 2.0 },
        });
        previous = spokes;
        ring += 1;
        if e.kind == Kind::Camp {
            break;
        }
    }
    // Hook actual country roads into the local navigation graph at the plaza.
    for route in routes {
        let mut pts: Vec<_> = route
            .points
            .into_iter()
            .filter(|p| dist(*p, center) < e.radius)
            .collect();
        if pts.is_empty() {
            continue;
        }
        pts.sort_by(|a, b| dist(*a, center).total_cmp(&dist(*b, center)));
        let p = pts[0];
        let n = add_node(world, &mut l, p);
        link(&mut l, 0, n);
        l.streets.push(Street {
            points: vec![center, p],
            width: 3.,
        });
    }
    validate_navigation(world, &mut l);
    l.entry.radius = l
        .buildings
        .iter()
        .map(|b| dist([b.x, b.z], center) + 14.)
        .fold(24., f32::max);
    if matches!(e.kind, Kind::Fort | Kind::City) {
        let radius = l.entry.radius + 9.;
        let count = (TAU * radius / 7.5).ceil() as usize;
        for i in 0..count {
            let a = (i as f32 + 0.5) / count as f32 * TAU;
            let x = center[0] + radius * a.cos();
            let z = center[1] + radius * a.sin();
            let gate_angle = r(id, 903) * TAU + 0.22 * (radius / 58. + r(id, 902) * TAU).sin();
            let local = a - gate_angle;
            if (radius * local.sin())
                .abs()
                .min((radius * local.cos()).abs())
                < 13.
            {
                continue;
            }
            let roads = world.road_routes_near(x, z, 12.);
            if roads.iter().any(|road| {
                road.points.windows(2).any(|s| {
                    let d = [s[1][0] - s[0][0], s[1][1] - s[0][1]];
                    let t = (((x - s[0][0]) * d[0] + (z - s[0][1]) * d[1])
                        / (d[0] * d[0] + d[1] * d[1]).max(0.001))
                    .clamp(0., 1.);
                    (x - s[0][0] - t * d[0]).hypot(z - s[0][1] - t * d[1]) < 9.
                })
            }) {
                continue;
            }
            let s = world.natural_sample(x, z);
            if s.ocean || s.height < s.water_height + 1. {
                continue;
            }
            l.walls.push(Building {
                id: 100_000_000 + id * 512 + i as u32,
                name: "perimeter wall".into(),
                usage: Use::Watchtower,
                x,
                z,
                floor: s.height,
                yaw: -PI * 0.5 - a,
                half: [TAU * radius / count as f32 * 0.5, 0.3],
                height: if e.kind == Kind::Fort { 3.6 } else { 4.2 },
                door: false,
                district: "Gates".into(),
                street_node: 0,
                porch_node: 0,
                room_node: 0,
            });
        }
    }
    for (i, b) in l.buildings.iter().enumerate() {
        let radius = b.half[0].hypot(b.half[1]) + 4.;
        for x in bucket16(b.x - radius)..=bucket16(b.x + radius) {
            for z in bucket16(b.z - radius)..=bucket16(b.z + radius) {
                l.parcels.entry((x, z)).or_default().push(i);
            }
        }
    }
    for street in &l.streets {
        for s in street.points.windows(2) {
            for x in bucket16(s[0][0].min(s[1][0]) - street.width - 1.)
                ..=bucket16(s[0][0].max(s[1][0]) + street.width + 1.)
            {
                for z in bucket16(s[0][1].min(s[1][1]) - street.width - 1.)
                    ..=bucket16(s[0][1].max(s[1][1]) + street.width + 1.)
                {
                    l.lanes
                        .entry((x, z))
                        .or_default()
                        .push((s[0], s[1], street.width));
                }
            }
        }
    }
    l.population = l
        .buildings
        .iter()
        .map(|b| match b.usage {
            Use::Home => 3 + (b.id % 3) as usize,
            Use::Tent => 2 + (b.id % 3) as usize,
            Use::Barracks => 12,
            _ => 2 + (b.id % 3) as usize,
        })
        .sum();
    l
}
pub fn segment_rect(a: [f32; 2], b: [f32; 2], half: [f32; 2]) -> bool {
    let mut lo = 0f32;
    let mut hi = 1f32;
    for axis in 0..2 {
        let d = b[axis] - a[axis];
        if d.abs() < 0.0001 {
            if a[axis].abs() > half[axis] {
                return false;
            }
        } else {
            let t0 = (-half[axis] - a[axis]) / d;
            let t1 = (half[axis] - a[axis]) / d;
            lo = lo.max(t0.min(t1));
            hi = hi.min(t0.max(t1));
        }
    }
    lo <= hi
}

/// Shared earthworks and vegetation mask. Foundations grade only their own lot;
/// lanes remain narrow and gardens retain their surrounding natural ground.
pub fn surface(world: &World, x: f32, z: f32, height: f32) -> (f32, f32) {
    let mut h = height;
    let mut cleared = 0f32;
    for l in world.settlements.layouts_near(world, x, z, 1.) {
        for wall in &l.walls {
            if wall.inside(x, z, 2.0) {
                cleared = 1.;
            }
        }
        let key = ((x / 16.).floor() as i32, (z / 16.).floor() as i32);
        if let Some(ids) = l.parcels.get(&key) {
            for &i in ids {
                let b = &l.buildings[i];
                let p = b.local(x, z);
                let edge = (p[0].abs() - b.half[0]).max(p[1].abs() - b.half[1]);
                if edge < 4. {
                    let t = (1. - edge.max(0.) / 4.).clamp(0., 1.);
                    h = h + (b.floor - 0.04 - h) * t;
                    cleared = cleared.max(t);
                }
            }
        }
        if let Some(segments) = l.lanes.get(&key) {
            for &(a, b, width) in segments {
                let d = [b[0] - a[0], b[1] - a[1]];
                let length = d[0] * d[0] + d[1] * d[1];
                let t =
                    (((x - a[0]) * d[0] + (z - a[1]) * d[1]) / length.max(0.0001)).clamp(0., 1.);
                let distance = (x - a[0] - t * d[0]).hypot(z - a[1] - t * d[1]);
                cleared = cleared.max((width + 1. - distance).clamp(0., 1.));
            }
        }
        if dist([x, z], [l.entry.site.x, l.entry.site.z]) < 8. {
            cleared = 1.;
        }
    }
    (h, cleared)
}

fn bucket16(x: f32) -> i32 {
    (x / 16.).floor() as i32
}

fn clear_edge(world: &World, l: &Layout, a: usize, b: usize) -> bool {
    let p = l.nodes[a];
    let q = l.nodes[b];
    for house in &l.buildings {
        if (a == house.room_node && b == house.porch_node)
            || (b == house.room_node && a == house.porch_node)
        {
            continue;
        }
        if segment_rect(
            house.local(p[0], p[2]),
            house.local(q[0], q[2]),
            [house.half[0] + 0.4, house.half[1] + 0.4],
        ) {
            return false;
        }
    }
    let length = dist([p[0], p[2]], [q[0], q[2]]);
    let steps = (length / 4.).ceil().max(1.) as usize;
    let mut previous: Option<f32> = None;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = p[0] + (q[0] - p[0]) * t;
        let z = p[2] + (q[2] - p[2]) * t;
        let s = world.natural_sample(x, z);
        if s.ocean || s.height < s.water_height + 0.2 {
            return false;
        }
        if let Some(h) = previous {
            if (s.height - h).abs() > length / steps as f32 * 0.75 + 0.4 {
                return false;
            }
        }
        previous = Some(s.height);
    }
    true
}
fn connected(l: &Layout) -> Vec<bool> {
    let mut seen = vec![false; l.nodes.len()];
    seen[0] = true;
    let mut stack = vec![0];
    while let Some(a) = stack.pop() {
        for &b in &l.links[a] {
            if !seen[b] {
                seen[b] = true;
                stack.push(b);
            }
        }
    }
    seen
}
fn validate_navigation(world: &World, l: &mut Layout) {
    let mut remove = vec![];
    for (a, links) in l.links.iter().enumerate() {
        for &b in links {
            if b > a && !clear_edge(world, l, a, b) {
                remove.push((a, b));
            }
        }
    }
    for (a, b) in remove {
        l.links[a].retain(|&n| n != b);
        l.links[b].retain(|&n| n != a);
    }
    // Join any accessible neighbourhood component through a verified local lane.
    for _ in 0..24 {
        let seen = connected(l);
        let mut candidates = vec![];
        for (a, p) in l.nodes.iter().enumerate() {
            if seen[a] || l.links[a].is_empty() {
                continue;
            }
            for (b, q) in l.nodes.iter().enumerate() {
                if !seen[b] {
                    continue;
                }
                let d = dist([p[0], p[2]], [q[0], q[2]]);
                if d < 42. {
                    candidates.push((d, a, b));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, a, b)) = candidates
            .into_iter()
            .take(256)
            .find(|&(_, a, b)| clear_edge(world, l, a, b))
        else {
            break;
        };
        link(l, a, b);
    }
    let seen = connected(l);
    l.buildings.retain(|b| seen[b.room_node]);
    curve_navigation(world, l);
    let seen = connected(l);
    l.streets.clear();
    for (a, links) in l.links.iter().enumerate() {
        for &b in links {
            if b < a
                || !seen[a]
                || !seen[b]
                || l.buildings
                    .iter()
                    .any(|h| a == h.room_node || b == h.room_node)
            {
                continue;
            }
            let p = l.nodes[a];
            let q = l.nodes[b];
            l.streets.push(Street {
                points: vec![[p[0], p[2]], [q[0], q[2]]],
                width: if l
                    .buildings
                    .iter()
                    .any(|h| a == h.porch_node || b == h.porch_node)
                {
                    1.05
                } else {
                    let center = [l.entry.site.x, l.entry.site.z];
                    if dist([p[0], p[2]], center).min(dist([q[0], q[2]], center)) < 55. {
                        2.6
                    } else {
                        1.7
                    }
                },
            });
        }
    }
}

/// Curve both the drawn street and the navigation graph along the same sampled
/// corridor. Candidate bends must remain dry, gentle and outside every house.
fn curve_navigation(world: &World, l: &mut Layout) {
    let edges: Vec<_> = l
        .links
        .iter()
        .enumerate()
        .flat_map(|(a, links)| links.iter().filter(move |&&b| b > a).map(move |&b| (a, b)))
        .collect();
    for (a, b) in edges {
        if l.buildings.iter().any(|h| {
            [h.room_node, h.porch_node].contains(&a) || [h.room_node, h.porch_node].contains(&b)
        }) {
            continue;
        }
        let p = l.nodes[a];
        let q = l.nodes[b];
        let length = dist([p[0], p[2]], [q[0], q[2]]);
        if length < 8. {
            continue;
        }
        let normal = [-(q[2] - p[2]) / length, (q[0] - p[0]) / length];
        let steps = (length / 4.).ceil() as usize;
        let preferred = if hash(l.entry.site.id, a as i32, b as i32) % 2 == 0 {
            1.
        } else {
            -1.
        };
        let first = l.nodes.len();
        let mut chosen = None;
        for bend in [preferred, -preferred, 0.35 * preferred, 0.] {
            l.nodes.truncate(first);
            l.links.truncate(first);
            let mut chain = vec![a];
            for i in 1..steps {
                let t = i as f32 / steps as f32;
                let offset = (PI * t).sin().powi(2) * (length * 0.13).min(3.2) * bend;
                let n = add_node(
                    world,
                    l,
                    [
                        p[0] + (q[0] - p[0]) * t + normal[0] * offset,
                        p[2] + (q[2] - p[2]) * t + normal[1] * offset,
                    ],
                );
                chain.push(n);
            }
            chain.push(b);
            if chain.windows(2).all(|s| clear_edge(world, l, s[0], s[1])) {
                chosen = Some(chain);
                break;
            }
        }
        if let Some(chain) = chosen {
            l.links[a].retain(|&n| n != b);
            l.links[b].retain(|&n| n != a);
            for pair in chain.windows(2) {
                link(l, pair[0], pair[1]);
            }
        } else {
            l.nodes.truncate(first);
            l.links.truncate(first);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settlements_have_sized_populations_clear_routes_and_grounded_doors() {
        let world = World::new(1337);
        let mut ids = std::collections::HashSet::new();
        for entry in &world.settlements.entries {
            assert!(ids.insert(entry.site.id));
        }
        for kind in [
            Kind::Camp,
            Kind::Hamlet,
            Kind::Fort,
            Kind::Village,
            Kind::Town,
            Kind::City,
        ] {
            let e = world
                .settlements
                .entries
                .iter()
                .filter(|e| e.kind == kind)
                .min_by(|a, b| {
                    dist([a.site.x, a.site.z], [0., 0.])
                        .total_cmp(&dist([b.site.x, b.site.z], [0., 0.]))
                })
                .unwrap();
            let l = world.settlements.layout(&world, e.site.id).unwrap();
            let (low, high) = match kind {
                Kind::Camp => (3, 5),
                Kind::Hamlet => (3, 8),
                Kind::Fort => (10, 22),
                Kind::Village => (10, 25),
                Kind::Town => (40, 80),
                Kind::City => (150, 250),
            };
            assert!(
                (low..=high).contains(&l.buildings.len()),
                "{kind:?}: {} buildings",
                l.buildings.len()
            );
            if kind == Kind::City {
                assert!((500..=1000).contains(&l.population), "{}", l.population);
            }
            assert!(l.entry.radius + 15. < e.radius);
            for (a, links) in l.links.iter().enumerate() {
                for &b in links {
                    assert!(
                        clear_edge(&world, &l, a, b),
                        "unclear {kind:?} link {a}->{b}"
                    );
                }
            }
            assert!(l
                .buildings
                .iter()
                .all(|b| !l.path(0, b.room_node).is_empty()));
            for (i, b) in l.buildings.iter().enumerate() {
                assert!(
                    l.buildings[..i].iter().all(|a| !b.overlaps_plot(a, 1.29)),
                    "overlapping plots in {kind:?}"
                );
            }
            if matches!(kind, Kind::Town | Kind::City) {
                assert!(
                    l.streets.iter().any(|s| s.width > 2.5)
                        && l.streets.iter().any(|s| s.width < 1.1)
                );
                let diagonal = l
                    .streets
                    .iter()
                    .filter(|s| {
                        let a = s.points[0];
                        let b = s.points[1];
                        (a[0] - b[0]).abs() > 0.5 && (a[1] - b[1]).abs() > 0.5
                    })
                    .count();
                assert!(
                    diagonal * 2 > l.streets.len(),
                    "streets collapsed into cardinal axes"
                );
            }
            for b in l.buildings.iter().filter(|b| b.door).take(3) {
                let p = b.point(0., 0., -b.half[1]);
                assert!(crate::settlement_mesh::blocked(&world, p[0], b.floor, p[2]));
                world.doors.borrow_mut().insert(
                    b.id,
                    crate::settlement_mesh::Door {
                        angle: 1.,
                        target: 1.,
                        hold: 5.,
                    },
                );
                assert!(
                    !crate::settlement_mesh::blocked(&world, p[0], b.floor, p[2]),
                    "open doorway blocked"
                );
                let p = b.point(0., 0., 0.);
                assert!((crate::geometry::walk_height(&world, p[0], p[2]) - b.floor).abs() < 0.06);
            }
        }
    }
}
