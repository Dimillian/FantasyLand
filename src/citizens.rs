//! Deterministic identities and schedules; only nearby people become renderable actors.
use crate::{
    geometry::{MeshData, Vertex},
    settlement_mesh,
    settlements::{dist, Layout, Use},
    world::{hash, rand01, World},
};
use serde::Serialize;
use std::f32::consts::{PI, TAU};
use std::{collections::HashMap, rc::Rc};
pub const ROLES: [&str; 16] = [
    "citizen",
    "farmer",
    "guard",
    "fighter",
    "arcanist",
    "mage",
    "ranger",
    "hunter",
    "merchant",
    "smith",
    "healer",
    "innkeeper",
    "caravan guard",
    "travelling merchant",
    "scholar",
    "courier",
];
#[derive(Clone, Debug, Serialize)]
pub struct Appearance {
    pub body: u8,
    pub hair: u8,
    pub skin: u8,
    pub garment: u8,
    pub equipment: u8,
    pub palette: [f32; 3],
    pub height: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Person {
    pub id: u64,
    pub name: String,
    pub age: u32,
    pub role: u8,
    pub home: u32,
    pub work: u32,
    pub home_name: String,
    pub work_name: String,
    pub birthplace: String,
    pub trait_name: String,
    pub history: String,
    pub appearance: Appearance,
    pub household: String,
}
#[derive(Clone)]
struct Route {
    points: Vec<[f32; 3]>,
    lengths: Vec<f32>,
    length: f32,
}
impl Route {
    fn new(points: Vec<[f32; 3]>) -> Self {
        let mut lengths = vec![0.];
        let mut length = 0.;
        for s in points.windows(2) {
            length += dist([s[0][0], s[0][2]], [s[1][0], s[1][2]]);
        }
        let mut sum = 0.;
        for s in points.windows(2) {
            sum += dist([s[0][0], s[0][2]], [s[1][0], s[1][2]]);
            lengths.push(sum);
        }
        Self {
            points,
            lengths,
            length,
        }
    }
    fn at(&self, d: f32) -> ([f32; 3], f32) {
        if self.points.len() < 2 {
            return (self.points.first().copied().unwrap_or([0.; 3]), 0.);
        }
        let d = d.clamp(0., self.length);
        let i = self
            .lengths
            .partition_point(|&v| v <= d)
            .saturating_sub(1)
            .min(self.points.len() - 2);
        let a = self.points[i];
        let b = self.points[i + 1];
        let t = ((d - self.lengths[i]) / (self.lengths[i + 1] - self.lengths[i]).max(0.001))
            .clamp(0., 1.);
        (
            std::array::from_fn(|axis| a[axis] + (b[axis] - a[axis]) * t),
            (b[0] - a[0]).atan2(-(b[2] - a[2])),
        )
    }
}
struct Resident {
    person: Person,
    routes: [Route; 4],
    wander: Route,
}
fn resident_pose(r: &Resident, now: f32) -> ([f32; 3], f32, bool, usize, f32, f32, f32) {
    let phase_shift = (hash(r.person.id as u32, 41, 17) % 91) as f32;
    let t = (now - phase_shift).rem_euclid(2880.);
    let (phase, elapsed) = if t < 7. * 120. {
        (0, t + 2. * 120.)
    } else if t < 17. * 120. {
        (1, t - 7. * 120.)
    } else if t < 20. * 120. {
        (2, t - 17. * 120.)
    } else if t < 22. * 120. {
        (3, t - 20. * 120.)
    } else {
        (0, t - 22. * 120.)
    };
    let route = &r.routes[phase];
    let speed = 1.32_f32.max(route.length / [1050., 1180., 340., 220.][phase]);
    let mut walking = elapsed * speed < route.length;
    let (mut p, mut yaw) = route.at(elapsed * speed);
    if phase == 1 && !walking {
        let time = (elapsed - route.length / speed).max(0.);
        let rest = if matches!(r.person.role, 2 | 6 | 7 | 15) {
            3.0
        } else {
            80.0 + (r.person.id % 121) as f32
        };
        let cycle = rest + r.wander.length / 1.2;
        let phase = time.rem_euclid(cycle.max(1.));
        let loop_end = 7. * 120.
            + route.length / speed
            + (time / cycle.max(1.)).floor() * cycle
            + rest
            + r.wander.length / 1.2;
        if phase > rest && loop_end < 17. * 120. - 2. {
            let pose = r.wander.at((phase - rest) * 1.2);
            p = pose.0;
            yaw = pose.1;
            walking = true;
        }
    }

    (p, yaw, walking, phase, elapsed, speed, t)
}
#[derive(Clone, Copy)]
struct WalkObstacle {
    center: [f32; 2],
    half: [f32; 2],
    sin: f32,
    cos: f32,
}
#[derive(Default)]
struct WalkSpace {
    cells: HashMap<(i32, i32), Vec<WalkObstacle>>,
}
impl WalkSpace {
    fn new(l: &Layout) -> Self {
        let mut space = Self::default();
        let mut add = |b: &crate::settlements::Building, lo: [f32; 2], hi: [f32; 2]| {
            let p = b.point((lo[0] + hi[0]) * 0.5, 0., (lo[1] + hi[1]) * 0.5);
            let (sin, cos) = b.yaw.sin_cos();
            let o = WalkObstacle {
                center: [p[0], p[2]],
                half: [(hi[0] - lo[0]) * 0.5 + 0.25, (hi[1] - lo[1]) * 0.5 + 0.25],
                sin,
                cos,
            };
            let extent = [
                cos.abs() * o.half[0] + sin.abs() * o.half[1],
                sin.abs() * o.half[0] + cos.abs() * o.half[1],
            ];
            for x in
                ((p[0] - extent[0]) / 4.).floor() as i32..=((p[0] + extent[0]) / 4.).floor() as i32
            {
                for z in ((p[2] - extent[1]) / 4.).floor() as i32
                    ..=((p[2] + extent[1]) / 4.).floor() as i32
                {
                    space.cells.entry((x, z)).or_default().push(o);
                }
            }
        };
        for b in &l.buildings {
            let [w, d] = b.half;
            if !matches!(b.usage, Use::Tent | Use::Market | Use::Stable) {
                for sign in [-1., 1.] {
                    add(b, [sign * w - 0.14, -d], [sign * w + 0.14, d]);
                }
                add(b, [-w, d - 0.14], [w, d + 0.14]);
                add(b, [-w, -d - 0.14], [-0.88, -d + 0.14]);
                add(b, [0.88, -d - 0.14], [w, -d + 0.14]);
            }
            settlement_mesh::interior_parts(b, |lo, hi, _, _, solid| {
                if solid && lo[1] < 1.7 && hi[1] > 0.15 {
                    add(b, [lo[0], lo[2]], [hi[0], hi[2]]);
                }
            });
        }
        for b in &l.walls {
            add(b, [-b.half[0], -b.half[1]], [b.half[0], b.half[1]]);
        }
        space
    }
    fn clear(&self, p: [f32; 3]) -> bool {
        self.cells
            .get(&((p[0] / 4.).floor() as i32, (p[2] / 4.).floor() as i32))
            .is_none_or(|items| {
                items.iter().all(|o| {
                    let d = [p[0] - o.center[0], p[2] - o.center[1]];
                    (d[0] * o.cos - d[1] * o.sin).abs() >= o.half[0]
                        || (d[0] * o.sin + d[1] * o.cos).abs() >= o.half[1]
                })
            })
    }
    fn segment_clear(&self, a: [f32; 3], b: [f32; 3]) -> bool {
        let steps = (dist([a[0], a[2]], [b[0], b[2]]) / 0.35).ceil().max(1.) as usize;
        (0..=steps).all(|i| {
            self.clear(std::array::from_fn(|k| {
                a[k] + (b[k] - a[k]) * i as f32 / steps as f32
            }))
        })
    }
}
struct Community {
    layout: Rc<Layout>,
    residents: Vec<Resident>,
    walk_space: WalkSpace,
}
#[derive(Clone, Debug, Serialize)]
pub struct Actor {
    pub person: Person,
    pub position: [f32; 3],
    pub yaw: f32,
    pub walking: bool,
    pub activity: String,
    pub settlement: u32,
    pub destination: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Topic {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Conversation {
    pub appearance: Appearance,
    pub age: u32,
    pub name: String,
    pub role: String,
    pub detail: String,
    pub text: String,
    pub topics: Vec<Topic>,
}
#[derive(Default)]
pub struct Life {
    pub clock: f64,
    pub actors: Vec<Actor>,
    communities: HashMap<u32, Community>,
    pub talking: Option<u64>,
    pub weather: String,
    refresh: f32,
    pose_elapsed: f32,
    ground: HashMap<(i32, i32), [f32; 4]>,
    traffic_routes: HashMap<u64, Route>,
    traffic_destinations: HashMap<u64, [String; 2]>,
    traffic: Vec<crate::world::Road>,
    crowd_offsets: HashMap<u64, [f32; 2]>,
    previous_positions: HashMap<u64, [f32; 3]>,
}
fn choose<'a>(id: u32, values: &[&'a str]) -> &'a str {
    values[id as usize % values.len()]
}
fn identity(l: &Layout, home: usize, ordinal: usize, role: u8, work: usize) -> Person {
    let h = &l.buildings[home];
    let w = &l.buildings[work];
    let seed = hash(h.id, ordinal as i32, 491);
    let family = choose(
        hash(h.id, 31, 17),
        &[
            "Reed", "Moss", "Voss", "Hale", "Wren", "Ash", "Grey", "Alder", "Stone", "Vale",
            "Dane", "Finch", "Thorne", "Marsh", "Rowan", "Briar",
        ],
    );
    let first = choose(
        seed,
        &[
            "Elin", "Mara", "Ada", "Tessa", "Iris", "Nora", "Anwen", "Freya", "Bram", "Edric",
            "Tomas", "Oren", "Garrick", "Hugh", "Alden", "Jory", "Sera", "Mira", "Lena", "Ronan",
            "Cora", "Ivar", "Darin", "Willa",
        ],
    );
    let trait_name = choose(
        seed / 19,
        &[
            "cautious",
            "warm-hearted",
            "restless",
            "practical",
            "curious",
            "quiet",
            "proud",
            "patient",
        ],
    );
    let palettes = [
        [0.30, 0.43, 0.29],
        [0.55, 0.25, 0.18],
        [0.26, 0.34, 0.49],
        [0.55, 0.43, 0.24],
        [0.39, 0.29, 0.48],
        [0.35, 0.41, 0.43],
        [0.59, 0.48, 0.35],
        [0.27, 0.38, 0.32],
    ];
    let origin = if seed % 4 == 0 {
        format!("a small village in {}", l.entry.region)
    } else {
        l.entry.site.name.clone()
    };
    Person {
        id: l.entry.site.id as u64 * 4096 + ordinal as u64,
        name: format!("{first} {family}"),
        age: 20 + seed % 47,
        role,
        home: h.id,
        work: w.id,
        home_name: h.name.clone(),
        work_name: w.name.clone(),
        birthplace: origin,
        trait_name: trait_name.into(),
        history: choose(
            seed / 31,
            &[
                "I learned my trade from my mother. I still use the tools she gave me.",
                "A hard winter brought my household here. We made a new start.",
                "I spent several years on the roads before finding a place here.",
                "My family has lived in these parts for generations. I know these streets well.",
                "I came here as an apprentice and stayed when my teacher retired.",
                "I used to help with the harvests. These days I have different responsibilities.",
            ],
        )
        .into(),
        household: family.into(),
        appearance: Appearance {
            body: if 20 + seed % 47 > 52 {
                3
            } else {
                (seed % 3) as u8
            },
            hair: if 20 + seed % 47 > 52 {
                4 + (seed % 2) as u8 * 2
            } else {
                (seed / 7 % 8) as u8
            },
            skin: (seed / 17 % 5) as u8,
            garment: role,
            equipment: role,
            palette: palettes[(seed / 43 % 8) as usize],
            height: 1.63 + rand01(seed) * 0.28,
        },
    }
}
/// Reusable clear standing positions. Each spur is visible from the room's
/// navigation anchor and checked against the same solid recipe as the player.
fn room_activity_spots(l: &Layout) -> HashMap<usize, Vec<[f32; 3]>> {
    let mut out = HashMap::new();
    for b in &l.buildings {
        let anchor = l.nodes[b.room_node];
        let a = b.local(anchor[0], anchor[2]);
        let mut obstacles = Vec::new();
        settlement_mesh::interior_parts(b, |lo, hi, _, _, solid| {
            if solid && lo[1] < 1.8 && hi[1] > 0. {
                obstacles.push((
                    [(lo[0] + hi[0]) * 0.5, (lo[2] + hi[2]) * 0.5],
                    [(hi[0] - lo[0]) * 0.5 + 0.30, (hi[2] - lo[2]) * 0.5 + 0.30],
                ));
            }
        });
        let mut spots = Vec::new();
        for iz in 0..((b.half[1] * 2. / 0.82) as i32) {
            for ix in -6..=6 {
                let q = [ix as f32 * 0.82, -b.half[1] + 1.75 + iz as f32 * 0.82];
                if q[0].abs() < 0.72 || q[0].abs() > b.half[0] - 0.7 || q[1] > b.half[1] - 0.7 {
                    continue;
                }
                if obstacles.iter().any(|(c, h)| {
                    crate::settlements::segment_rect(
                        [a[0] - c[0], a[1] - c[1]],
                        [q[0] - c[0], q[1] - c[1]],
                        *h,
                    )
                }) {
                    continue;
                }
                spots.push((hash(b.id, ix, iz), b.point(q[0], 0., q[1])));
            }
        }
        spots.sort_by_key(|v| v.0);
        out.insert(
            b.room_node,
            if spots.is_empty() {
                vec![anchor]
            } else {
                spots.into_iter().map(|p| p.1).collect()
            },
        );
    }
    out
}
fn community(l: Rc<Layout>) -> Community {
    let room_spots = room_activity_spots(&l);
    let walk_space = WalkSpace::new(&l);
    let mut outdoor_occupancy: HashMap<usize, Vec<[f32; 3]>> = HashMap::new();
    let mut room_occupancy: HashMap<usize, usize> = HashMap::new();
    let services: Vec<_> = l
        .buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.usage.public())
        .map(|(i, _)| i)
        .collect();
    let mut residents = vec![];
    let mut ordinal = 0;
    for (home, h) in l.buildings.iter().enumerate() {
        let count = match h.usage {
            Use::Home => 3 + (h.id % 3) as usize,
            Use::Tent => 2 + (h.id % 3) as usize,
            Use::Barracks => 12,
            _ => 2 + (h.id % 3) as usize,
        };
        for member in 0..count {
            let seed = hash(h.id, member as i32, 391);
            let work = home;
            let role = match l.buildings[work].usage {
                Use::Inn => 11,
                Use::Smithy => 9,
                Use::Guild => 3,
                Use::Arcane => {
                    if seed % 2 == 0 {
                        4
                    } else {
                        5
                    }
                }
                Use::Barracks | Use::Watchtower => 2,
                Use::Temple => 10,
                Use::Hall => 14,
                Use::Market => 8,
                _ => [0, 1, 6, 7][(seed % 4) as usize],
            };
            let role = if h.usage == Use::Home || h.usage == Use::Tent {
                [0, 0, 0, 1, 1, 6, 7, 8, 14, 15][seed as usize % 10]
            } else {
                role
            };
            let work = if h.usage == Use::Home {
                let service = match role {
                    8 => Some(Use::Market),
                    14 => Some(Use::Hall),
                    1 => Some(Use::Stable),
                    _ => None,
                };
                service
                    .and_then(|usage| {
                        services
                            .iter()
                            .copied()
                            .find(|&i| l.buildings[i].usage == usage)
                    })
                    .or_else(|| {
                        (role == 0 && seed % 3 != 0 && !services.is_empty())
                            .then(|| services[seed as usize % services.len()])
                    })
                    .unwrap_or(home)
            } else {
                work
            };
            let mut person = identity(&l, home, ordinal, role, work);
            let leisure = services
                .iter()
                .find(|&&i| l.buildings[i].usage == Use::Inn)
                .copied()
                .unwrap_or(home);
            let work_node = if matches!(role, 2 | 6 | 7 | 1 | 15) {
                // Outdoor trades belong at the edge of the inhabited area.
                let outer = l
                    .buildings
                    .iter()
                    .filter(|b| {
                        dist([b.x, b.z], [l.entry.site.x, l.entry.site.z]) > l.entry.radius * 0.58
                    })
                    .collect::<Vec<_>>();
                if outer.is_empty() {
                    h.street_node
                } else {
                    outer[seed as usize % outer.len()].street_node
                }
            } else {
                l.buildings[work].room_node
            };
            let leisure_node = if seed % 5 == 0 {
                l.buildings[leisure].room_node
            } else {
                l.buildings[(seed as usize / 17) % l.buildings.len()].street_node
            };
            let mut anchors = [
                h.room_node,
                work_node,
                l.buildings[(seed as usize / 7) % l.buildings.len()].street_node,
                leisure_node,
            ];
            // Public rooms have finite standing capacity. Overflow visits wait
            // in distinct outdoor yards instead of wrapping onto occupied spots.
            for phase in 1..4 {
                let node = anchors[phase];
                if node != h.room_node
                    && room_spots.get(&node).is_some_and(|spots| {
                        room_occupancy.get(&node).copied().unwrap_or(0) >= spots.len()
                    })
                {
                    anchors[phase] = l.buildings
                        [(seed as usize / 29 + phase * 7) % l.buildings.len()]
                    .street_node;
                }
            }
            let work_node = anchors[1];
            if matches!(role, 1 | 6 | 7 | 15) && h.usage == Use::Home {
                person.work_name = match role {
                    1 => "the outer plots and grazing grounds",
                    6 | 7 => "the settlement's outskirts",
                    8 => {
                        if l.buildings.iter().any(|b| b.usage == Use::Market) {
                            "the neighbourhood market"
                        } else {
                            "the neighbourhood lanes"
                        }
                    }
                    15 => "the neighbourhood lanes",
                    _ => "the neighbourhood",
                }
                .into();
            }
            let nearest_street = l
                .buildings
                .iter()
                .filter(|b| b.street_node != work_node)
                .min_by(|a, b| {
                    dist(
                        [l.nodes[a.street_node][0], l.nodes[a.street_node][2]],
                        [l.nodes[work_node][0], l.nodes[work_node][2]],
                    )
                    .total_cmp(&dist(
                        [l.nodes[b.street_node][0], l.nodes[b.street_node][2]],
                        [l.nodes[work_node][0], l.nodes[work_node][2]],
                    ))
                })
                .map_or(0, |b| b.street_node);
            let mut walk = l.path(work_node, nearest_street);
            let back = l.path(nearest_street, work_node);
            walk.extend(back.into_iter().skip(1));
            let mut assigned = HashMap::new();
            for &node in &anchors {
                if assigned.contains_key(&node) {
                    continue;
                }
                if let Some(spots) = room_spots.get(&node) {
                    let used = room_occupancy.entry(node).or_default();
                    assigned.insert(node, spots[*used % spots.len()]);
                    *used += 1;
                } else {
                    // Stop beside the lane, never at a shared centerline node.
                    let p = l.nodes[node];
                    let next = l.links[node]
                        .iter()
                        .copied()
                        .find(|&n| {
                            !l.buildings
                                .iter()
                                .any(|b| [b.porch_node, b.room_node].contains(&n))
                        })
                        .unwrap_or(node);
                    let q = l.nodes[next];
                    let length = dist([p[0], p[2]], [q[0], q[2]]).max(0.001);
                    let tangent = [(q[0] - p[0]) / length, (q[2] - p[2]) / length];
                    let used = outdoor_occupancy.entry(node).or_default();
                    let mut spot = p;
                    for trial in 0..32 {
                        let side = if (seed + trial) % 2 == 0 { 1. } else { -1. };
                        let along = ((trial / 2) % 8) as f32 * 0.9 - 3.15;
                        let offset = side * (2.25 + (trial / 16) as f32 * 0.85);
                        let candidate = [
                            p[0] + tangent[0] * along - tangent[1] * offset,
                            p[1],
                            p[2] + tangent[1] * along + tangent[0] * offset,
                        ];
                        if used
                            .iter()
                            .all(|q| dist([q[0], q[2]], [candidate[0], candidate[2]]) > 0.85)
                            && l.buildings.iter().all(|b| {
                                let e = b.entrance();
                                dist([e[0], e[2]], [candidate[0], candidate[2]]) > 2.2
                            })
                            && walk_space.segment_clear(p, candidate)
                        {
                            spot = candidate;
                            break;
                        }
                    }
                    used.push(spot);
                    assigned.insert(node, spot);
                }
            }
            let spread = |points: Vec<[f32; 3]>| {
                let last = points.len().saturating_sub(1);
                let mut out = Vec::with_capacity(points.len() + 2);
                for (i, p) in points.iter().copied().enumerate() {
                    let endpoint = assigned
                        .iter()
                        .find(|(n, _)| l.nodes[**n] == p)
                        .map(|(_, p)| *p);
                    if i == 0 {
                        if let Some(spot) = endpoint {
                            out.push(spot);
                        }
                    }
                    let mut lane = p;
                    if !l.buildings.iter().any(|b| b.inside(p[0], p[2], 2.0)) {
                        let a = points[i.saturating_sub(1)];
                        let b = points[(i + 1).min(last)];
                        let length = dist([a[0], a[2]], [b[0], b[2]]).max(0.001);
                        let offset = 0.68 + (seed % 7) as f32 * 0.055;
                        let q = [
                            p[0] - (b[2] - a[2]) / length * offset,
                            p[1],
                            p[2] + (b[0] - a[0]) / length * offset,
                        ];
                        if walk_space.segment_clear(p, q) {
                            lane = q;
                        }
                    }
                    // Avoid cutting a corner into a porch or a building.
                    if let Some(&previous) = out.last() {
                        if !walk_space.segment_clear(previous, lane) {
                            if i > 0 {
                                out.push(points[i - 1]);
                            }
                            lane = p;
                        }
                    }
                    out.push(lane);
                    if i == last {
                        if let Some(spot) = endpoint {
                            if !walk_space.segment_clear(lane, spot) {
                                out.push(p);
                            }
                            out.push(spot);
                        }
                    }
                }
                out.dedup();
                out
            };
            let wander = Route::new(spread(walk));
            let routes = std::array::from_fn(|i| {
                Route::new(spread(l.path(anchors[(i + 3) % 4], anchors[i])))
            });
            residents.push(Resident {
                person,
                routes,
                wander,
            });
            ordinal += 1;
        }
    }
    Community {
        layout: l,
        residents,
        walk_space,
    }
}
impl Life {
    pub fn invalidate(&mut self) {
        self.ground.clear();
        self.traffic_routes.clear();
        self.traffic_destinations.clear();
        self.refresh = 0.;
        self.pose_elapsed = 1.;
        self.actors.clear();
        self.crowd_offsets.clear();
        self.previous_positions.clear();
    }
    pub fn new() -> Self {
        Self {
            clock: 1080.,
            ..Default::default()
        }
    }
    pub fn update(&mut self, world: &World, eye: [f32; 3], dt: f32) {
        if self.talking.is_some() {
            return;
        }
        self.clock += dt as f64;
        self.refresh -= dt;
        for d in world.doors.borrow_mut().values_mut() {
            let difference = d.target - d.angle;
            d.angle += (difference.signum() * dt * 1.8).clamp(-difference.abs(), difference.abs());
        }
        self.pose_elapsed += dt;
        if self.pose_elapsed < 0.075 {
            return;
        }
        let dt = self.pose_elapsed;
        self.pose_elapsed = 0.;
        let nearby = world.settlements.layouts_near(world, eye[0], eye[2], 180.);
        self.communities
            .retain(|id, _| nearby.iter().any(|l| l.entry.site.id == *id));
        for l in nearby {
            self.communities
                .entry(l.entry.site.id)
                .or_insert_with(|| community(l));
        }
        // Unloaded door animation states are transient; keep per-frame work local.
        world
            .doors
            .borrow_mut()
            .retain(|id, _| self.communities.contains_key(&(id / 512)));
        self.previous_positions = self
            .actors
            .iter()
            .map(|a| (a.person.id, a.position))
            .collect();
        self.actors.clear();
        let now = (self.clock % 2880.) as f32;
        for c in self.communities.values() {
            for r in &c.residents {
                let (mut p, yaw, walking, phase, elapsed, speed, t) = resident_pose(r, now);
                if dist([eye[0], eye[2]], [p[0], p[2]]) > 145. {
                    continue;
                }
                p[1] = resident_ground(world, &mut self.ground, p[0], p[2]);
                let key = ((p[0] / 16.).floor() as i32, (p[2] / 16.).floor() as i32);
                if let Some(ids) = c.layout.parcels.get(&key) {
                    for &i in ids {
                        let b = &c.layout.buildings[i];
                        if b.inside(p[0], p[2], 0.) {
                            p[1] = p[1].max(b.floor);
                        }
                    }
                }

                let activity = if walking {
                    match phase {
                        0 => "heading home",
                        1 => {
                            if elapsed * speed < r.routes[phase].length {
                                "walking to work"
                            } else {
                                "making the rounds"
                            }
                        }
                        2 => "visiting neighbours",
                        _ => "heading to an evening visit",
                    }
                } else {
                    match phase {
                        0 => {
                            if t < 6. * 120. || t >= 23. * 120. {
                                "sleeping at home"
                            } else {
                                "at home"
                            }
                        }
                        1 => {
                            if matches!(r.person.role, 2 | 6 | 7) {
                                "on patrol"
                            } else {
                                "at work"
                            }
                        }
                        2 => "taking a break in the neighbourhood",
                        _ => "relaxing after work",
                    }
                };

                self.actors.push(Actor {
                    person: r.person.clone(),
                    position: p,
                    yaw,
                    walking,
                    activity: activity.into(),
                    settlement: c.layout.entry.site.id,
                    destination: match phase {
                        0 => r.person.home_name.clone(),
                        1 => r.person.work_name.clone(),
                        2 => format!("the neighbourhood in {}", c.layout.entry.site.name),
                        _ => c
                            .layout
                            .buildings
                            .iter()
                            .find(|b| {
                                let end = *r.routes[3].points.last().unwrap();
                                b.inside(end[0], end[2], 0.)
                            })
                            .map_or_else(|| "the neighbourhood lanes".into(), |b| b.name.clone()),
                    },
                });
            }
        }
        if self.refresh <= 0. {
            self.traffic = world
                .road_routes_near(eye[0], eye[2], 1800.)
                .into_iter()
                .filter(|r| r.kind == crate::world::RoadKind::Main && r.id % 5 == 0)
                .take(18)
                .collect();
            self.refresh = 12.;
        }
        for road in &self.traffic {
            if road.points.len() < 2 {
                continue;
            }
            let route = self.traffic_routes.entry(road.id).or_insert_with(|| {
                Route::new(road.points.iter().map(|p| [p[0], 0., p[1]]).collect())
            });
            if route.length < 500. {
                continue;
            }
            let phase = (self.clock * 1.6 + (road.id % 10007) as f64)
                .rem_euclid(route.length as f64 * 2.) as f32;
            let distance = if phase < route.length {
                phase
            } else {
                2. * route.length - phase
            };
            for n in 0..3 {
                let (mut p, mut yaw) = route.at((distance - n as f32 * 3.4).max(0.));
                if phase >= route.length {
                    yaw += PI;
                }
                p[0] += yaw.cos() * 0.9;
                p[2] += yaw.sin() * 0.9;
                if dist([eye[0], eye[2]], [p[0], p[2]]) > 145. {
                    continue;
                }
                p[1] = crate::geometry::walk_height(world, p[0], p[2]);
                let destinations = self.traffic_destinations.entry(road.id).or_insert_with(|| {
                    std::array::from_fn(|i| {
                        let endpoint = road.points[if i == 0 { 0 } else { road.points.len() - 1 }];
                        world
                            .settlements
                            .near(endpoint[0], endpoint[1], 2000.)
                            .into_iter()
                            .min_by(|a, b| {
                                dist([a.site.x, a.site.z], endpoint)
                                    .total_cmp(&dist([b.site.x, b.site.z], endpoint))
                            })
                            .map_or("the next inhabited junction".into(), |e| {
                                e.site.name.clone()
                            })
                    })
                });
                let destination = destinations[if phase < route.length { 1 } else { 0 }].clone();
                let seed = hash(road.id as u32, n, 831);
                let person=Person{id:1_000_000_000+road.id*4+n as u64,name:format!("{} {}",choose(seed,&["Mara","Bram","Edric","Lena","Oren","Sera"]),choose(seed/7,&["Wayfarer","Reed","Hale","Finch","Dane"])),age:23+seed%39,role:if n==0{13}else{12},home:0,work:0,home_name:"a travelling camp".into(),work_name:"the caravan".into(),birthplace:"the border country".into(),trait_name:"watchful".into(),history:"I travel with this small company. We take provisions and letters between the towns.".into(),household:"travellers".into(),appearance:Appearance{body:(seed%4) as u8,hair:(seed%8) as u8,skin:(seed%5) as u8,garment:13,equipment:13,palette:[0.42,0.31,0.19],height:1.8}};
                self.actors.push(Actor {
                    person,
                    position: p,
                    yaw,
                    walking: true,
                    activity: "travelling with a caravan".into(),
                    settlement: 0,
                    destination: destination.clone(),
                });
            }
        }
        self.actors.sort_by(|a, b| {
            dist([a.position[0], a.position[2]], [eye[0], eye[2]])
                .total_cmp(&dist([b.position[0], b.position[2]], [eye[0], eye[2]]))
                .then_with(|| a.person.id.cmp(&b.person.id))
        });
        self.actors.truncate(320);
        self.resolve_crowds(world, dt);
        let mut doors = world.doors.borrow_mut();
        for c in self.communities.values() {
            for b in c.layout.buildings.iter().filter(|b| b.door) {
                let near = (doors.get(&b.id).is_some_and(|d| d.target > 0.5)
                    && dist([eye[0], eye[2]], [b.entrance()[0], b.entrance()[2]]) < 1.4)
                    || self.actors.iter().any(|a| {
                        a.walking
                            && dist(
                                [a.position[0], a.position[2]],
                                [b.entrance()[0], b.entrance()[2]],
                            ) < 2.6
                    });
                let d = doors.entry(b.id).or_default();
                if near {
                    d.target = 1.;
                    d.hold = 3.;
                } else {
                    d.hold = (d.hold - dt).max(0.);
                    if d.hold <= 0. {
                        d.target = 0.;
                    }
                }
            }
        }
    }
    fn resolve_crowds(&mut self, world: &World, dt: f32) {
        // Nearby actors only; spatial buckets keep this bounded instead of an
        // all-pairs scan. Carry offsets between ticks to avoid jittering back
        // into the same overlap every time the schedule is evaluated.
        let desired: Vec<_> = self.actors.iter().map(|a| a.position).collect();
        let clear = |p: [f32; 3]| self.communities.values().all(|c| c.walk_space.clear(p));
        for actor in &mut self.actors {
            if let Some(offset) = self.crowd_offsets.get(&actor.person.id) {
                let fade = (-dt * 1.6).exp();
                let p = [
                    actor.position[0] + offset[0] * fade,
                    actor.position[1],
                    actor.position[2] + offset[1] * fade,
                ];
                if clear(p) {
                    actor.position = p;
                }
            }
        }
        for _ in 0..5 {
            let mut cells: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
            for (i, a) in self.actors.iter().enumerate() {
                cells
                    .entry((
                        (a.position[0] / 1.2).floor() as i32,
                        (a.position[2] / 1.2).floor() as i32,
                    ))
                    .or_default()
                    .push(i);
            }
            let mut shifts = vec![[0f32; 2]; self.actors.len()];
            for (i, a) in self.actors.iter().enumerate() {
                let cell = (
                    (a.position[0] / 1.2).floor() as i32,
                    (a.position[2] / 1.2).floor() as i32,
                );
                for x in cell.0 - 1..=cell.0 + 1 {
                    for z in cell.1 - 1..=cell.1 + 1 {
                        if let Some(indices) = cells.get(&(x, z)) {
                            for &j in indices {
                                if j <= i {
                                    continue;
                                }
                                let b = &self.actors[j];
                                if (a.position[1] - b.position[1]).abs() > 1.2 {
                                    continue;
                                }
                                let mut dx = a.position[0] - b.position[0];
                                let mut dz = a.position[2] - b.position[2];
                                let distance = dx.hypot(dz);
                                if distance >= 0.82 {
                                    continue;
                                }
                                if distance < 0.001 {
                                    let angle = (a.person.id % 17) as f32 * 0.37;
                                    dx = angle.cos();
                                    dz = angle.sin();
                                } else {
                                    dx /= distance;
                                    dz /= distance;
                                }
                                let force = ((0.84 - distance) * 0.52).min(0.28);
                                shifts[i][0] += dx * force;
                                shifts[i][1] += dz * force;
                                shifts[j][0] -= dx * force;
                                shifts[j][1] -= dz * force;
                            }
                        }
                    }
                }
            }
            for (i, a) in self.actors.iter_mut().enumerate() {
                let p = [
                    a.position[0] + shifts[i][0],
                    a.position[1],
                    a.position[2] + shifts[i][1],
                ];
                if dist([p[0], p[2]], [desired[i][0], desired[i][2]]) < 2.2 && clear(p) {
                    a.position = p;
                }
            }
        }
        self.crowd_offsets.clear();
        for (i, a) in self.actors.iter_mut().enumerate() {
            self.crowd_offsets.insert(
                a.person.id,
                [a.position[0] - desired[i][0], a.position[2] - desired[i][2]],
            );
            a.position[1] = if a.settlement == 0 {
                // Road travellers must stay on bridge decks after avoidance.
                crate::geometry::walk_height(world, a.position[0], a.position[2])
            } else {
                resident_ground(world, &mut self.ground, a.position[0], a.position[2])
            };
            if let Some(c) = self.communities.get(&a.settlement) {
                let key = (
                    (a.position[0] / 16.).floor() as i32,
                    (a.position[2] / 16.).floor() as i32,
                );
                if let Some(ids) = c.layout.parcels.get(&key) {
                    for &id in ids {
                        let b = &c.layout.buildings[id];
                        if b.inside(a.position[0], a.position[2], 0.) {
                            a.position[1] = a.position[1].max(b.floor);
                        }
                    }
                }
            }
        }
    }
    pub fn home_occupied(&self, id: u32) -> bool {
        self.communities.values().any(|c| {
            let Some(home) = c.layout.buildings.iter().find(|b| b.id == id) else {
                return false;
            };
            c.residents.iter().filter(|r| r.person.home == id).any(|r| {
                let (p, _, _, _, _, _, _) = resident_pose(r, (self.clock % 2880.) as f32);
                home.inside(p[0], p[2], -0.2)
            })
        })
    }
    pub fn mesh(&self, world: &World, eye: [f32; 3], yaw: f32) -> MeshData {
        let mut mesh = MeshData::default();
        let right = [yaw.cos(), yaw.sin()];
        for a in &self.actors {
            let position =
                self.previous_positions
                    .get(&a.person.id)
                    .map_or(a.position, |previous| {
                        let t = (self.pose_elapsed / 0.075).clamp(0., 1.);
                        std::array::from_fn(|k| previous[k] + (a.position[k] - previous[k]) * t)
                    });
            let h = a.person.appearance.height;
            let width = h * 0.52;
            let view = (eye[0] - a.position[0]).atan2(-(eye[2] - a.position[2]));
            let dir =
                (((view - a.yaw + PI * 0.25).rem_euclid(TAU) / (PI * 0.5)).floor() as u32) % 4;
            let frame = if a.walking {
                ((self.clock * 7. + a.person.id as f64) % 4.) as u32
            } else {
                0
            };
            let packed = a.person.role as u32
                + dir * 16
                + frame * 64
                + a.person.appearance.skin as u32 * 256
                + a.person.appearance.hair as u32 * 2048
                + a.person.appearance.body as u32 * 16384;
            let start = mesh.vertices.len() as u32;
            for (x, y, uv) in [
                (-0.5, 0., [0., 1.]),
                (-0.5, 1., [0., 0.]),
                (0.5, 1., [1., 0.]),
                (0.5, 0., [1., 1.]),
            ] {
                mesh.vertices.push(Vertex {
                    wind: 0.,
                    position: [
                        position[0] + right[0] * x * width,
                        position[1] + (y * 96.0 - 4.0) / 92.0 * h,
                        position[2] + right[1] * x * width,
                    ],
                    normal: [-yaw.sin(), 0.12, yaw.cos()],
                    color: a.person.appearance.palette,
                    material: 11.,
                    uv,
                    texture: 1000. + packed as f32,
                });
            }
            mesh.indices
                .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
        }
        for l in world.settlements.layouts_near(world, eye[0], eye[2], 190.) {
            for b in &l.buildings {
                if b.door && dist([eye[0], eye[2]], [b.x, b.z]) < 190. {
                    let angle = world.doors.borrow().get(&b.id).map_or(0., |d| d.angle);
                    settlement_mesh::door_mesh(&mut mesh, b, angle);
                }
            }
        }
        mesh
    }
    pub fn target(
        &self,
        world: &World,
        eye: [f32; 3],
        yaw: f32,
        pitch: f32,
    ) -> Option<(String, u64)> {
        let direction = [
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        ];
        let mut candidates = vec![];
        for a in &self.actors {
            let p = [a.position[0], a.position[1] + 1.15, a.position[2]];
            if let Some(d) = aim(eye, direction, p, 3.8, 0.55) {
                candidates.push((d, "person".into(), a.person.id));
            }
        }
        for l in world.settlements.layouts_near(world, eye[0], eye[2], 5.) {
            for b in &l.buildings {
                if !b.door {
                    continue;
                }
                let p = b.point(0., 1.2, -b.half[1]);
                if let Some(d) = aim(eye, direction, p, 3.2, 1.0) {
                    candidates.push((d, "door".into(), b.id as u64));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        candidates
            .into_iter()
            .find(|(_, kind, id)| {
                if kind == "door" {
                    return true;
                }
                let Some(a) = self.actors.iter().find(|a| a.person.id == *id) else {
                    return false;
                };
                let d = dist([eye[0], eye[2]], [a.position[0], a.position[2]]);
                (1..(d / 0.25) as usize).all(|i| {
                    let t = i as f32 * 0.25 / d;
                    !settlement_mesh::blocked(
                        world,
                        eye[0] + (a.position[0] - eye[0]) * t,
                        eye[1] - 1.65,
                        eye[2] + (a.position[2] - eye[2]) * t,
                    )
                })
            })
            .map(|(_, kind, id)| (kind, id))
    }
    pub fn conversation(&self, world: &World, topic: &str) -> Option<Conversation> {
        let a = self
            .actors
            .iter()
            .find(|a| Some(a.person.id) == self.talking)?;
        let p = &a.person;
        let mut topics = vec![
            Topic {
                id: "work".into(),
                label: "What are you doing?".into(),
            },
            Topic {
                id: "life".into(),
                label: "Tell me about yourself.".into(),
            },
            Topic {
                id: "home".into(),
                label: "Do you live here?".into(),
            },
            Topic {
                id: "area".into(),
                label: "What can you tell me about this place?".into(),
            },
            Topic {
                id: "road".into(),
                label: "Where are you heading?".into(),
            },
            Topic {
                id: "weather".into(),
                label: "How are things today?".into(),
            },
        ];
        let layout = world.settlements.layout(world, a.settlement);
        if let Some(l) = &layout {
            for (key, label, usage) in [
                ("inn", "Where is the nearest inn?", Use::Inn),
                ("smith", "Where can I find a smith?", Use::Smithy),
                ("guild", "Where is the fighters' guild?", Use::Guild),
                ("arcane", "Where do the mages gather?", Use::Arcane),
                ("temple", "Where is the temple?", Use::Temple),
                ("market", "Where is the market?", Use::Market),
            ] {
                let _ = usage;
                topics.push(Topic {
                    id: key.into(),
                    label: label.into(),
                });
            }
            let _ = l;
        }
        let hour = (self.clock / 120. % 24.) as u32;
        let text=match topic {
            "work"=>format!("I'm {}. I'm a {} by trade. {}",a.activity,ROLES[p.role as usize],if a.walking{format!("I'm making my way to {}.",a.destination)}else{format!("You'll usually find me at {} during the working day.",p.work_name)}),
            "life"=>format!("I'm {}, {} years old. I was born in {}. {} People sometimes call me {}.",p.name,p.age,p.birthplace,p.history,p.trait_name),
            "home"=>format!("My household is the {} family. I live at {}. {}",p.household,p.home_name,if hour<7||hour>=22{"Most of us are home at this hour."}else{"Most of us are out during the day."}),
            "weather"=>format!("{} over {} at this hour. {}",if self.weather.is_empty(){"Quiet skies"}else{&self.weather},layout.as_ref().map_or("the road",|l|l.entry.site.name.as_str()),if self.weather.to_lowercase().contains("rain")||self.weather.to_lowercase().contains("storm"){ "I would keep close to shelter until it passes."}else{"A good time to see who is about."}),
            "road"=>format!("I'm {}. My destination is {}.",a.activity,a.destination),
            "area"=>layout.as_ref().map_or("Keep to the established roads between towns. Away from them, there are long stretches of wilderness.".into(),|l|format!("This is {}, a {} in {}. {}",l.entry.site.name,l.entry.kind.name(),l.entry.region,if l.entry.capital{"The region is governed from here; the civic hall and guilds stand near the market."}else{"The square is the best place to ask after a neighbour. Beyond our lanes, the wilderness takes over."})),
            "inn"|"smith"|"guild"|"arcane"|"temple"|"market"=>{let usage=match topic{"inn"=>Use::Inn,"smith"=>Use::Smithy,"guild"=>Use::Guild,"arcane"=>Use::Arcane,"temple"=>Use::Temple,_=>Use::Market};layout.as_ref().and_then(|l|l.buildings.iter().filter(|b|b.usage==usage).min_by(|b,c|dist([b.x,b.z],[a.position[0],a.position[2]]).total_cmp(&dist([c.x,c.z],[a.position[0],a.position[2]])))).map_or(format!("We don't have a {} here. You would need to ask in a larger settlement.",usage.name()),|b|{let bearing=(b.x-a.position[0]).atan2(-(b.z-a.position[2]));let direction=["north","northeast","east","southeast","south","southwest","west","northwest"][(((bearing+PI/8.).rem_euclid(TAU)/(PI/4.)) as usize)%8];let meters=dist([b.x,b.z],[a.position[0],a.position[2]]);format!("{} is to the {}, {}. Follow the lanes into the {}; look for the sign beside the entrance.",b.name,direction,if meters<25.{"just a few steps away".into()}else{format!("roughly {} paces from here",(meters/10.).round() as u32*10)},b.district.to_lowercase())})},
            _=>format!("{} I'm {}, the {}. {}",if hour<6||hour>=21{"Evening, traveller."}else{"Good day, traveller."},p.name,ROLES[p.role as usize],if a.walking{"I have a moment before I go."}else{"What would you like to know?"})};
        Some(Conversation {
            appearance: p.appearance.clone(),
            age: p.age,
            name: p.name.clone(),
            role: ROLES[p.role as usize].into(),
            detail: format!("{} · {}", p.trait_name, a.activity),
            text,
            topics,
        })
    }
}
fn aim(eye: [f32; 3], dir: [f32; 3], p: [f32; 3], range: f32, radius: f32) -> Option<f32> {
    let d = std::array::from_fn::<_, 3, _>(|i| p[i] - eye[i]);
    let along = d.iter().zip(dir).map(|(a, b)| a * b).sum::<f32>();
    let distance = d.iter().map(|a| a * a).sum::<f32>();
    (along > 0. && along < range && distance - along * along < radius * radius).then_some(along)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capital_crowds_have_clear_routes_and_personal_space() {
        let world = World::new(1337);
        let e = world
            .settlements
            .entries
            .iter()
            .filter(|e| e.kind == crate::settlements::Kind::City)
            .min_by(|a, b| {
                dist([a.site.x, a.site.z], [0., 0.])
                    .total_cmp(&dist([b.site.x, b.site.z], [0., 0.]))
            })
            .unwrap();
        let layout = world.settlements.layout(&world, e.site.id).unwrap();
        let c = community(layout.clone());
        let mut bad = 0;
        for resident in &c.residents {
            for route in resident
                .routes
                .iter()
                .chain(std::iter::once(&resident.wander))
            {
                for pair in route.points.windows(2) {
                    if !c.walk_space.segment_clear(pair[0], pair[1]) {
                        bad += 1;
                        if bad < 5 {
                            println!("blocked {} {:?}", resident.person.name, pair);
                        }
                    }
                }
            }
        }
        assert_eq!(bad, 0, "routes cut through solid fixtures");
        let eye = [e.site.x, world.height(e.site.x, e.site.z) + 1.7, e.site.z];
        let mut life = Life::new();
        for hour in [8., 12., 17.5, 20.5] {
            life.clock = hour * 120.;
            life.invalidate();
            for _ in 0..100 {
                life.update(&world, eye, 0.08);
            }
            let mut overlap = 0;
            let mut obstructed = 0;
            for (i, a) in life.actors.iter().enumerate() {
                if !c.walk_space.clear(a.position) {
                    obstructed += 1;
                }
                for b in &layout.buildings {
                    if b.inside(a.position[0], a.position[2], 0.) {
                        assert!(
                            a.position[1] >= b.floor - 0.01,
                            "crowd adjustment sank a resident below the floor"
                        );
                    }
                }
                for b in &life.actors[..i] {
                    if dist(
                        [a.position[0], a.position[2]],
                        [b.position[0], b.position[2]],
                    ) < 0.60
                        && (a.position[1] - b.position[1]).abs() < 1.2
                    {
                        overlap += 1;
                        if overlap < 6 {
                            println!(
                                "overlap {} {:?} {} with {} {:?} {} at {:?}",
                                a.person.name,
                                a.walking,
                                a.activity,
                                b.person.name,
                                b.walking,
                                b.activity,
                                a.position
                            );
                        }
                    }
                }
            }
            println!(
                "capital {hour}: {} actors, {overlap} overlaps, {obstructed} inside solids",
                life.actors.len()
            );
            assert_eq!(obstructed, 0);
            assert_eq!(overlap, 0);
        }
    }
    #[test]
    fn identities_schedules_dialogue_and_pause_share_actual_world_facts() {
        let world = World::new(1337);
        let e = world
            .settlements
            .entries
            .iter()
            .filter(|e| e.kind == crate::settlements::Kind::Village)
            .min_by(|a, b| {
                dist([a.site.x, a.site.z], [0., 0.])
                    .total_cmp(&dist([b.site.x, b.site.z], [0., 0.]))
            })
            .unwrap();
        let l = world.settlements.layout(&world, e.site.id).unwrap();
        let spots = room_activity_spots(&l);
        for building in &l.buildings {
            let points = &spots[&building.room_node];
            if building.usage == Use::Inn {
                assert!(
                    points.len() >= 6,
                    "inn must have distinct activity positions"
                );
            }
            for p in points {
                assert!(
                    !settlement_mesh::blocked(&world, p[0], building.floor + 0.01, p[2]),
                    "resident standing in furniture in {}",
                    building.name
                );
            }
        }
        let eye = [e.site.x, world.height(e.site.x, e.site.z) + 1.7, e.site.z];
        let c = community(l.clone());
        for resident in &c.residents {
            for route in resident
                .routes
                .iter()
                .chain(std::iter::once(&resident.wander))
            {
                for pair in route.points.windows(2) {
                    assert!(
                        c.walk_space.segment_clear(pair[0], pair[1]),
                        "route through building/furniture for {}: {:?}",
                        resident.person.name,
                        pair
                    );
                }
            }
        }
        for hour in [8., 12., 17.5, 20.5] {
            let mut crowd = Life::new();
            crowd.clock = hour * 120.;
            crowd.invalidate();
            for _ in 0..20 {
                crowd.update(&world, eye, 0.08);
            }
            let mut close = 0;
            let mut pairs = 0;
            for (i, a) in crowd.actors.iter().enumerate() {
                if a.settlement != l.entry.site.id {
                    continue;
                }
                assert!(
                    c.walk_space.clear(a.position),
                    "actor in solid geometry: {}",
                    a.person.name
                );
                for b in &crowd.actors[..i] {
                    let distance = dist(
                        [a.position[0], a.position[2]],
                        [b.position[0], b.position[2]],
                    );
                    if distance < 1.5 {
                        pairs += 1;
                    }
                    if distance < 0.60 && (a.position[1] - b.position[1]).abs() < 1.2 {
                        close += 1;
                    }
                }
            }
            println!(
                "crowd {hour}: {} actors, {close} overlaps / {pairs} neighbors",
                crowd.actors.len()
            );
            assert_eq!(close, 0, "overlapping residents at {hour}");
        }
        let mut a = Life::new();
        a.update(&world, eye, 0.1);
        assert!(!a.actors.is_empty());
        let first = a
            .actors
            .iter()
            .find(|a| a.settlement == e.site.id)
            .unwrap()
            .person
            .clone();
        a.talking = Some(first.id);
        let reply = a.conversation(&world, "inn").unwrap();
        assert!(reply.text.contains(
            &l.buildings
                .iter()
                .find(|b| b.usage == Use::Inn)
                .unwrap()
                .name
        ));
        let story = a.conversation(&world, "life").unwrap();
        assert!(story.text.contains(&first.birthplace));
        let clock = a.clock;
        let positions: Vec<_> = a.actors.iter().map(|a| a.position).collect();
        a.update(&world, eye, 3.);
        assert_eq!(a.clock, clock);
        assert_eq!(
            positions,
            a.actors.iter().map(|a| a.position).collect::<Vec<_>>()
        );
        let mut b = Life::new();
        b.clock = clock;
        b.invalidate();
        b.update(&world, eye, 0.);
        assert_eq!(
            first.name,
            b.actors
                .iter()
                .find(|a| a.person.id == first.id)
                .unwrap()
                .person
                .name
        );
        // World midnight must not warp residents still walking home.
        for time in [0., 7. * 120., 17. * 120., 20. * 120., 22. * 120.] {
            let mut before = Life::new();
            before.clock = 2880. + time - 0.01;
            before.invalidate();
            before.update(&world, eye, 0.);
            let mut after = Life::new();
            after.clock = 2880. + time + 0.01;
            after.invalidate();
            after.update(&world, eye, 0.);
            for p in before.actors.iter().filter(|p| p.settlement == e.site.id) {
                if let Some(q) = after.actors.iter().find(|q| q.person.id == p.person.id) {
                    assert!(
                        dist(
                            [p.position[0], p.position[2]],
                            [q.position[0], q.position[2]]
                        ) < 0.5,
                        "schedule teleport for {} at {time}",
                        p.person.name
                    );
                }
            }
        }
    }
}

fn resident_ground(
    world: &World,
    cache: &mut HashMap<(i32, i32), [f32; 4]>,
    x: f32,
    z: f32,
) -> f32 {
    let ix = (x / 6.).floor() as i32;
    let iz = (z / 6.).floor() as i32;
    let ox = ix as f32 * 6.;
    let oz = iz as f32 * 6.;
    if cache.len() > 8192 {
        cache.clear();
    }
    let [a, b, c, d] = *cache.entry((ix, iz)).or_insert_with(|| {
        [
            world.height(ox, oz),
            world.height(ox, oz + 6.),
            world.height(ox + 6., oz + 6.),
            world.height(ox + 6., oz),
        ]
    });
    let u = (x - ox) / 6.;
    let v = (z - oz) / 6.;
    if hash(world.seed, ix, iz) & 1 == 0 {
        if u + v <= 1. {
            a + (d - a) * u + (b - a) * v
        } else {
            c + (b - c) * (1. - u) + (d - c) * (1. - v)
        }
    } else if v >= u {
        a + (c - b) * u + (b - a) * v
    } else {
        a + (d - a) * u + (c - d) * v
    }
}
