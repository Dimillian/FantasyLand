//! Sparse natural monuments built entirely from shared deterministic solid recipes.
//! Every opening is genuinely empty geometry. Collision tests individual solids,
//! including their actual sloped underside, rather than a landmark-wide cylinder.
use crate::geometry::{terrain_surface_height_lod, MeshData, CHUNK_SIZE};
use crate::regions::{Geology, LandscapeKind};
use crate::world::{hash, rand01, ShoreKind, World};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};

pub const CELL_SIZE: f32 = 768.;
pub const MAX_RADIUS: f32 = 112.;
const EMBED: f32 = 0.65;
thread_local! {
    static CACHE: RefCell<BTreeMap<(u32,i32,i32), Option<NaturalLandmark>>> = RefCell::new(BTreeMap::new());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalKind {
    Arch,
    StoneWindow,
    Pinnacles,
    GraniteTor,
    BoulderRing,
    BrokenEscarpment,
    BasaltPipes,
    SplitMonolith,
    CoastalStacks,
    Hoodoos,
}
impl NaturalKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Arch => "Wind-carved arch",
            Self::StoneWindow => "Great stone window",
            Self::Pinnacles => "The stone needles",
            Self::GraniteTor => "Crown of granite",
            Self::BoulderRing => "Broken stone amphitheatre",
            Self::BrokenEscarpment => "Fractured escarpment",
            Self::BasaltPipes => "The basalt organ",
            Self::SplitMonolith => "Cleft monolith",
            Self::CoastalStacks => "Sentinels of the tide",
            Self::Hoodoos => "The weathered spires",
        }
    }
}

/// Rarity changes structure and footprint, not just a display label. These are
/// candidate proportions; broad terrain/road/water rejection changes the census.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NaturalRarity {
    Common,
    Rare,
    Monumental,
}
impl NaturalRarity {
    pub fn from_id(id: u32) -> Self {
        let p = random(id, 3201);
        if p < 0.86 {
            Self::Common
        } else if p < 0.98 {
            Self::Rare
        } else {
            Self::Monumental
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Common => 0,
            Self::Rare => 1,
            Self::Monumental => 2,
        }
    }
    fn scale(self, id: u32) -> f32 {
        let (low, span) = match self {
            Self::Common => (0.64, 0.34),
            Self::Rare => (1.04, 0.32),
            Self::Monumental => (1.42, 0.28),
        };
        low + random(id, 100) * span
    }
    fn count(self, id: u32, salt: u32, ranges: [(u32, u32); 3]) -> u32 {
        let (lo, hi) = ranges[self.index()];
        lo + (random(id, salt) * (hi - lo + 1) as f32).floor() as u32
    }
}
impl NaturalLandmark {
    pub fn rarity(&self) -> NaturalRarity {
        NaturalRarity::from_id(self.id)
    }
}

/// One convex vertical solid with independently sloping upper and lower surfaces.
/// Footprints are counterclockwise in X/Z. Ring triangulation and collision share
/// the same triangles, including bridge-like stones suspended over a clear opening.
#[derive(Clone, Debug)]
pub struct Solid {
    pub footprint: Vec<[f32; 2]>,
    pub bottom: Vec<f32>,
    pub top: Vec<f32>,
    pub grounded: bool,
    pub detail: bool,
    pub color: [f32; 3],
}
#[derive(Clone, Debug, Serialize)]
pub struct NaturalLandmark {
    pub id: u32,
    pub name: String,
    pub kind: NaturalKind,
    pub x: f32,
    pub z: f32,
    pub ground: f32,
    pub radius: f32,
    pub top: f32,
    /// Local +X geological strike, radians around world Y.
    pub yaw: f32,
    #[serde(skip)]
    pub solids: Vec<Solid>,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct NaturalView {
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct NaturalDestination {
    pub name: String,
    pub kind: NaturalKind,
    pub x: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub landmark_x: f32,
    pub landmark_z: f32,
}

/// Bounded world-wide sample selects real monuments, without changing the world.
/// Called only when the landscape tour or native photographer requests it.
pub fn destinations(world: &World) -> Vec<NaturalDestination> {
    let mut selected: [Option<NaturalDestination>; 10] = std::array::from_fn(|_| None);
    for i in 0..4600 {
        let gx = (hash(world.seed ^ 0x735c, i, 17) % 480) as i32 - 240;
        let gz = (hash(world.seed ^ 0x735c, i, 39) % 480) as i32 - 240;
        let Some(n) = landmark_at(world, gx, gz) else {
            continue;
        };
        let k = n.kind as usize;
        if selected[k].is_some() {
            continue;
        }
        let Some(v) = scenic_view(world, &n) else {
            continue;
        };
        selected[k] = Some(NaturalDestination {
            name: format!("{} · {}", n.name, n.kind.name()),
            kind: n.kind,
            x: v.x,
            z: v.z,
            yaw: v.yaw,
            pitch: v.pitch,
            landmark_x: n.x,
            landmark_z: n.z,
        });
        if selected.iter().all(Option::is_some) {
            break;
        }
    }
    selected.into_iter().flatten().collect()
}
fn random(seed: u32, k: u32) -> f32 {
    rand01(hash(seed ^ 0xaec41937, k as i32, (seed >> 12) as i32))
}
fn ground(w: &World, x: f32, z: f32) -> f32 {
    terrain_surface_height_lod(w, x, z, 0)
}
fn rotated(x: f32, z: f32, yaw: f32) -> [f32; 2] {
    [x * yaw.cos() - z * yaw.sin(), x * yaw.sin() + z * yaw.cos()]
}
fn tint(c: [f32; 3], v: f32) -> [f32; 3] {
    c.map(|x| x * v)
}

/// A sparse candidate is jittered inside the cell and selected before expensive
/// terrain queries. All consumers call this same eligibility path.
fn candidate(seed: u32, gx: i32, gz: i32) -> Option<(u32, f32, f32)> {
    let id = hash(seed ^ 0x4e415455, gx, gz);
    if random(id, 1) > 0.34 {
        return None;
    }
    Some((
        id,
        (gx as f32 + 0.16 + random(id, 2) * 0.68) * CELL_SIZE,
        (gz as f32 + 0.16 + random(id, 3) * 0.68) * CELL_SIZE,
    ))
}
pub fn landmark_at(world: &World, gx: i32, gz: i32) -> Option<NaturalLandmark> {
    let key = (world.seed, gx, gz);
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let result = build_at(world, gx, gz);
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Deterministic generation is independent of this bounded acceleration.
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(key, result.clone());
    });
    result
}
fn build_at(world: &World, gx: i32, gz: i32) -> Option<NaturalLandmark> {
    let (seed, x, z) = candidate(world.seed, gx, gz)?;
    let sample = world.sample(x, z);
    let terrain = ground(world, x, z);
    if sample.ocean || sample.road > 0.025 || terrain < sample.water_height + 0.65 {
        return None;
    }
    let region = crate::regions::sample(world.seed, x, z, &sample);
    if region.slope > 0.78 {
        return None;
    }
    let pick = random(seed, 4);
    let kind = if sample.shore != ShoreKind::None || region.kind == LandscapeKind::WindsweptCoast {
        if region.rockiness < 0.36 || region.slope > 0.58 {
            return None;
        }
        NaturalKind::CoastalStacks
    } else {
        match region.geology {
            Geology::Sandstone => match (pick * 5.) as u32 {
                0 => NaturalKind::Arch,
                1 => NaturalKind::StoneWindow,
                2 => NaturalKind::Hoodoos,
                3 => NaturalKind::BrokenEscarpment,
                _ => NaturalKind::Pinnacles,
            },
            Geology::Chalk => {
                if pick < 0.33 {
                    NaturalKind::StoneWindow
                } else if pick < 0.67 {
                    NaturalKind::BrokenEscarpment
                } else {
                    NaturalKind::Pinnacles
                }
            }
            Geology::Basalt => {
                if pick < 0.65 {
                    NaturalKind::BasaltPipes
                } else {
                    NaturalKind::SplitMonolith
                }
            }
            Geology::Granite => {
                if pick < 0.44 {
                    NaturalKind::GraniteTor
                } else if pick < 0.68 {
                    NaturalKind::BoulderRing
                } else if pick < 0.90 {
                    NaturalKind::SplitMonolith
                } else {
                    NaturalKind::Pinnacles
                }
            }
            // Glacial erratics can remain in meadow soils, but not fertile mud.
            Geology::Loam if region.rockiness > 0.38 && pick < 0.24 => NaturalKind::BoulderRing,
            _ => return None,
        }
    };
    if matches!(
        kind,
        NaturalKind::Arch | NaturalKind::StoneWindow | NaturalKind::BoulderRing
    ) && region.slope > 0.38
    {
        return None;
    }
    let yaw =
        region.formation_axis[1].atan2(region.formation_axis[0]) + (random(seed, 5) - 0.5) * 0.65;
    let monument = recipe(world, seed, kind, x, z, yaw, region.rock_color);
    if monument.radius > MAX_RADIUS {
        return None;
    }
    if !eligible_footprint(world, &monument) {
        return None;
    }
    Some(monument)
}

fn eligible_footprint(world: &World, n: &NaturalLandmark) -> bool {
    // Roads and existing cultural markers keep their actual travel clearances.
    if world
        .sites_near(n.x, n.z, n.radius + 65.)
        .iter()
        .any(|s| (s.x - n.x).hypot(s.z - n.z) < n.radius + 55.)
        || world
            .landmarks_near(n.x, n.z, n.radius + 35.)
            .iter()
            .any(|s| (s.x - n.x).hypot(s.z - n.z) < n.radius + 25.)
    {
        return false;
    }
    for road in world.road_routes_near(n.x, n.z, n.radius + 14.) {
        for segment in road.points.windows(2) {
            let p = closest_on_segment([n.x, n.z], segment[0], segment[1]);
            if (p[0] - n.x).hypot(p[1] - n.z) < n.radius + road.kind.half_width() + 7. {
                return false;
            }
        }
    }
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    // Sample a bounded 6 m lattice through each support, not merely the center.
    for solid in n.solids.iter().filter(|s| s.grounded) {
        let (min, max) = solid.bounds();
        let nx = ((max[0] - min[0]) / 6.).ceil().max(1.) as usize;
        let nz = ((max[1] - min[1]) / 6.).ceil().max(1.) as usize;
        for iz in 0..=nz {
            for ix in 0..=nx {
                let x = min[0] + (max[0] - min[0]) * ix as f32 / nx as f32;
                let z = min[1] + (max[1] - min[1]) * iz as f32 / nz as f32;
                if !solid.contains_xz(x, z, 0.25) {
                    continue;
                }
                let s = world.natural_sample(x, z);
                let h = ground(world, x, z);
                if s.ocean || h < s.water_height + 0.5 {
                    return false;
                }
                lo = lo.min(h);
                hi = hi.max(h);
            }
        }
        for p in &solid.footprint {
            let s = world.natural_sample(p[0], p[1]);
            let h = ground(world, p[0], p[1]);
            if s.ocean || h < s.water_height + 0.5 {
                return false;
            }
        }
    }
    let max_relief = if matches!(
        n.kind,
        NaturalKind::Arch | NaturalKind::StoneWindow | NaturalKind::BoulderRing
    ) {
        10.
    } else {
        24.
    };
    if hi - lo > max_relief {
        return false;
    }
    // Passage families must admit an actual walking-height corridor across their
    // opening, even on sloping terrain. No invisible wall and no low rock roof.
    if matches!(
        n.kind,
        NaturalKind::Arch
            | NaturalKind::StoneWindow
            | NaturalKind::SplitMonolith
            | NaturalKind::BrokenEscarpment
            | NaturalKind::BoulderRing
    ) {
        for step in -4..=4 {
            let q = rotated(0., step as f32 * 2.2, n.yaw);
            let x = n.x + q[0];
            let z = n.z + q[1];
            let s = world.natural_sample(x, z);
            let feet = ground(world, x, z);
            if s.ocean || feet < s.water_height + 0.5 || n.blocks(x, feet, z, 0.40, 2.15) {
                return false;
            }
        }
    }
    true
}

/// Query may cross cell boundaries; candidates use the same ownership everywhere.
pub fn landmarks_near(world: &World, x: f32, z: f32, radius: f32) -> Vec<NaturalLandmark> {
    let mut result = Vec::new();
    for gz in ((z - radius - MAX_RADIUS) / CELL_SIZE).floor() as i32
        ..=((z + radius + MAX_RADIUS) / CELL_SIZE).floor() as i32
    {
        for gx in ((x - radius - MAX_RADIUS) / CELL_SIZE).floor() as i32
            ..=((x + radius + MAX_RADIUS) / CELL_SIZE).floor() as i32
        {
            let Some((_, px, pz)) = candidate(world.seed, gx, gz) else {
                continue;
            };
            if (px - x).hypot(pz - z) > radius + MAX_RADIUS {
                continue;
            }
            if let Some(n) = landmark_at(world, gx, gz) {
                if (n.x - x).hypot(n.z - z) <= radius + n.radius {
                    result.push(n);
                }
            }
        }
    }
    result
}
/// Call once per prop chunk; the owning center prevents duplication at boundaries.
/// Far geometry keeps every major piece and every actual opening. It drops only
/// loose rubble and decorative seam subdivisions, never a structural support.
pub fn append_chunk(world: &World, mesh: &mut MeshData, cx: i32, cz: i32, far: bool, lod: u32) {
    let ox = cx as f32 * CHUNK_SIZE;
    let oz = cz as f32 * CHUNK_SIZE;
    for gz in (oz / CELL_SIZE).floor() as i32..=((oz + CHUNK_SIZE) / CELL_SIZE).floor() as i32 {
        for gx in (ox / CELL_SIZE).floor() as i32..=((ox + CHUNK_SIZE) / CELL_SIZE).floor() as i32 {
            let Some((_, px, pz)) = candidate(world.seed, gx, gz) else {
                continue;
            };
            if px < ox || px >= ox + CHUNK_SIZE || pz < oz || pz >= oz + CHUNK_SIZE {
                continue;
            }
            let Some(n) = landmark_at(world, gx, gz) else {
                continue;
            };
            n.append_mesh(world, mesh, far, lod);
        }
    }
}
pub fn blocks_player(world: &World, x: f32, feet: f32, z: f32, radius: f32, height: f32) -> bool {
    landmarks_near(world, x, z, radius)
        .iter()
        .any(|n| n.blocks(x, feet, z, radius, height))
}
impl NaturalLandmark {
    pub fn blocks(&self, x: f32, feet: f32, z: f32, radius: f32, height: f32) -> bool {
        (x - self.x).hypot(z - self.z) < self.radius + radius
            && self
                .solids
                .iter()
                .any(|s| s.blocks(x, feet, z, radius, height))
    }
    /// Use to keep tree trunks and legacy boulders out of natural rock masses and
    /// the important through-passage. Ground cover can still grow in their shade.
    pub fn clears_props(&self, x: f32, z: f32, padding: f32) -> bool {
        if (x - self.x).hypot(z - self.z) > self.radius + padding {
            return false;
        }
        let d = rotated(x - self.x, z - self.z, -self.yaw);
        let passage = matches!(
            self.kind,
            NaturalKind::Arch
                | NaturalKind::StoneWindow
                | NaturalKind::SplitMonolith
                | NaturalKind::BrokenEscarpment
                | NaturalKind::BoulderRing
        ) && d[0].abs() < 3.0 + padding
            && d[1].abs() < self.radius;
        passage || self.solids.iter().any(|s| s.contains_xz(x, z, padding))
    }
    pub fn append_mesh(&self, world: &World, mesh: &mut MeshData, far: bool, lod: u32) {
        for s in self.solids.iter().filter(|s| !far || !s.detail) {
            s.append_mesh(world, mesh, far, lod);
        }
    }
}
impl Solid {
    fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for p in &self.footprint {
            for k in 0..2 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        (min, max)
    }
    fn closest(&self, x: f32, z: f32) -> [f32; 2] {
        let p = [x, z];
        let inside = (0..self.footprint.len()).all(|i| {
            let a = self.footprint[i];
            let b = self.footprint[(i + 1) % self.footprint.len()];
            (b[0] - a[0]) * (z - a[1]) - (b[1] - a[1]) * (x - a[0]) >= -0.001
        });
        if inside {
            return p;
        }
        let mut nearest = self.footprint[0];
        let mut distance = f32::INFINITY;
        for i in 0..self.footprint.len() {
            let q = closest_on_segment(
                p,
                self.footprint[i],
                self.footprint[(i + 1) % self.footprint.len()],
            );
            let d = (q[0] - x).powi(2) + (q[1] - z).powi(2);
            if d < distance {
                distance = d;
                nearest = q;
            }
        }
        nearest
    }
    pub fn contains_xz(&self, x: f32, z: f32, padding: f32) -> bool {
        let q = self.closest(x, z);
        (q[0] - x).powi(2) + (q[1] - z).powi(2) <= padding * padding + 0.00001
    }
    fn height_at(&self, p: [f32; 2], ring: &[f32]) -> f32 {
        for i in 1..self.footprint.len() - 1 {
            let a = self.footprint[0];
            let b = self.footprint[i];
            let c = self.footprint[i + 1];
            let den = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
            if den.abs() < 0.00001 {
                continue;
            }
            let u = ((b[1] - c[1]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[1] - c[1])) / den;
            let v = ((c[1] - a[1]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[1] - c[1])) / den;
            if u >= -0.003 && v >= -0.003 && u + v <= 1.003 {
                return ring[0] * u + ring[i] * v + ring[i + 1] * (1. - u - v);
            }
        }
        ring.iter().sum::<f32>() / ring.len() as f32
    }
    pub fn blocks(&self, x: f32, feet: f32, z: f32, radius: f32, height: f32) -> bool {
        let p = self.closest(x, z);
        if (p[0] - x).powi(2) + (p[1] - z).powi(2) > radius * radius + 0.00001 {
            return false;
        }
        // A small vertical overlap allowance makes tangential contacts stable.
        let bottom = self.height_at(p, &self.bottom);
        let top = self.height_at(p, &self.top);
        feet + height > bottom + 0.02 && feet < top - 0.04
    }
    fn append_mesh(&self, world: &World, mesh: &mut MeshData, far: bool, lod: u32) {
        let n = self.footprint.len();
        let bottom: Vec<_> = self
            .footprint
            .iter()
            .zip(&self.bottom)
            .map(|(p, y)| {
                let y = if self.grounded && lod > 0 {
                    y.min(terrain_surface_height_lod(world, p[0], p[1], lod) - EMBED)
                } else {
                    *y
                };
                [p[0], y, p[1]]
            })
            .collect();
        let top: Vec<_> = self
            .footprint
            .iter()
            .zip(&self.top)
            .map(|(p, y)| [p[0], *y, p[1]])
            .collect();
        let bands = if far { 1 } else { 3 };
        for i in 0..n {
            let j = (i + 1) % n;
            for k in 0..bands {
                let f = k as f32 / bands as f32;
                let t = (k + 1) as f32 / bands as f32;
                let lerp = |a: [f32; 3], b: [f32; 3], t: f32| {
                    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
                };
                // Vertical sediment color seams, no repeated texture or hidden mesh.
                mesh.quad(
                    lerp(bottom[i], top[i], f),
                    lerp(bottom[i], top[i], t),
                    lerp(bottom[j], top[j], t),
                    lerp(bottom[j], top[j], f),
                    tint(
                        self.color,
                        0.88 + (i % 3) as f32 * 0.045 + (k % 2) as f32 * 0.05,
                    ),
                    2.,
                );
            }
        }
        for i in 1..n - 1 {
            mesh.triangle(top[0], top[i + 1], top[i], tint(self.color, 1.06), 2.);
            if !self.grounded {
                mesh.triangle(
                    bottom[0],
                    bottom[i],
                    bottom[i + 1],
                    tint(self.color, 0.76),
                    2.,
                );
            }
        }
    }
}
fn closest_on_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    let dx = b[0] - a[0];
    let dz = b[1] - a[1];
    let t =
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / (dx * dx + dz * dz).max(0.0001)).clamp(0., 1.);
    [a[0] + dx * t, a[1] + dz * t]
}

struct Builder<'a> {
    w: &'a World,
    seed: u32,
    x: f32,
    z: f32,
    yaw: f32,
    color: [f32; 3],
    solids: Vec<Solid>,
}
impl Builder<'_> {
    fn world(&self, p: [f32; 2]) -> [f32; 2] {
        let q = rotated(p[0], p[1], self.yaw);
        [self.x + q[0], self.z + q[1]]
    }
    fn local_ground(&self, p: [f32; 2]) -> f32 {
        let q = self.world(p);
        ground(self.w, q[0], q[1])
    }
    /// Six sides retain a strong irregular lithic silhouette at both detail levels.
    fn footprint(
        &self,
        center: [f32; 2],
        size: [f32; 2],
        angle: f32,
        variant: u32,
        sides: usize,
    ) -> Vec<[f32; 2]> {
        (0..sides)
            .map(|i| {
                let a = i as f32 * TAU / sides as f32;
                let p = rotated(a.cos() * size[0], a.sin() * size[1], angle);
                let wobble = 0.90 + random(self.seed, variant + i as u32) * 0.16;
                self.world([center[0] + p[0] * wobble, center[1] + p[1] * wobble])
            })
            .collect()
    }
    fn column(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        height: f32,
        angle: f32,
        variant: u32,
        detail: bool,
    ) {
        let footprint = self.footprint(center, size, angle, variant, 6);
        let bottom: Vec<_> = footprint
            .iter()
            .map(|p| ground(self.w, p[0], p[1]) - EMBED)
            .collect();
        let base = bottom.iter().copied().fold(f32::NEG_INFINITY, f32::max) + EMBED;
        let top = footprint
            .iter()
            .enumerate()
            .map(|(i, _)| {
                base + height * (0.90 + random(self.seed, variant + 40 + i as u32) * 0.13)
            })
            .collect();
        self.solids.push(Solid {
            footprint,
            bottom,
            top,
            grounded: true,
            detail,
            color: tint(self.color, 0.92 + random(self.seed, variant + 70) * 0.16),
        });
    }
    /// A bevel-sided eroded slab, with unequal depth and fractured upper edge.
    /// It is still the same shared convex solid used by body collision.
    fn beam(&mut self, x0: f32, x1: f32, z: f32, depth: f32, y0: f32, y1: f32, thickness: f32) {
        let salt = 2400 + self.solids.len() as u32 * 19;
        let middle = (x0 + x1) * 0.5;
        let d0 = depth * (0.70 + random(self.seed, salt) * 0.30);
        let d1 = depth * (0.75 + random(self.seed, salt + 1) * 0.30);
        let dm = depth * (1.03 + random(self.seed, salt + 2) * 0.18);
        let local = vec![
            [x0, z - d0],
            [middle, z - dm],
            [x1, z - d1],
            [x1, z + d1],
            [middle, z + dm],
            [x0, z + d0],
        ];
        let footprint = local.iter().map(|p| self.world(*p)).collect();
        let bottom: Vec<f32> = local
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let t = ((p[0] - x0) / (x1 - x0)).clamp(0., 1.);
                y0 + (y1 - y0) * t + (random(self.seed, salt + 3 + i as u32) - 0.5) * 0.26 * depth
            })
            .collect();
        let top = bottom
            .iter()
            .enumerate()
            .map(|(i, y)| y + thickness * (0.70 + random(self.seed, salt + 11 + i as u32) * 0.80))
            .collect();
        self.solids.push(Solid {
            footprint,
            bottom,
            top,
            grounded: false,
            detail: false,
            color: self.color,
        });
    }
    /// Attached scree buttress: its triangulated top rises to a broken off-center
    /// crest and falls back into the ground. No square pedestal or hidden cone.
    fn talus(&mut self, center: [f32; 2], size: [f32; 2], height: f32, angle: f32, variant: u32) {
        let footprint = self.footprint(center, size, angle, variant, 6);
        let bottom: Vec<_> = footprint
            .iter()
            .map(|p| ground(self.w, p[0], p[1]) - EMBED)
            .collect();
        let crest = bottom.iter().copied().fold(f32::NEG_INFINITY, f32::max) + EMBED;
        let profile = [0.12, 0.46, 1.0, 0.75, 0.18, 0.05];
        let top = bottom
            .iter()
            .enumerate()
            .map(|(i, y)| {
                (*y + 0.15).max(
                    crest
                        + height
                            * profile[i]
                            * (0.85 + random(self.seed, variant + 30 + i as u32) * 0.22),
                )
            })
            .collect();
        self.solids.push(Solid {
            footprint,
            bottom,
            top,
            grounded: true,
            detail: false,
            color: tint(self.color, 0.91),
        });
    }
    fn cap(
        &mut self,
        center: [f32; 2],
        size: [f32; 2],
        y: f32,
        thickness: f32,
        angle: f32,
        variant: u32,
    ) {
        let footprint = self.footprint(center, size, angle, variant, 6);
        self.solids.push(Solid {
            bottom: vec![y; 6],
            top: (0..6)
                .map(|i| y + thickness * (1.0 + random(self.seed, variant + 81 + i) * 0.40))
                .collect(),
            footprint,
            grounded: false,
            detail: false,
            color: tint(self.color, 0.94),
        });
    }
}
impl Builder<'_> {
    /// Offset, narrowing upper courses make a leaning silhouette while retaining
    /// exact individual solids. Broken variants end in a sloping fracture face.
    fn spire(
        &mut self,
        c: [f32; 2],
        size: [f32; 2],
        height: f32,
        angle: f32,
        variant: u32,
        broken: bool,
    ) {
        let body = height
            * if broken {
                0.55 + random(self.seed, variant + 201) * 0.24
            } else {
                0.67
            };
        self.column(c, size, body, angle, variant, false);
        let lower = self.solids.last_mut().unwrap();
        let base = lower
            .bottom
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
            + EMBED;
        if broken {
            for (i, y) in lower.top.iter_mut().enumerate() {
                *y = base
                    + body
                        * (0.73
                            + (i as f32 * 0.77).sin() * 0.22
                            + random(self.seed, variant + 210 + i as u32) * 0.21);
            }
        } else {
            let contact = lower.top.iter().copied().fold(f32::INFINITY, f32::min) - 0.38;
            let tilt = rotated(
                size[0] * (0.09 + random(self.seed, variant + 220) * 0.10),
                size[1] * (random(self.seed, variant + 221) - 0.5) * 0.15,
                angle,
            );
            self.cap(
                [c[0] + tilt[0], c[1] + tilt[1]],
                [size[0] * 0.66, size[1] * 0.65],
                contact,
                height * 0.32,
                angle,
                variant + 230,
            );
        }
    }
    fn arch_span(
        &mut self,
        cx: f32,
        half: f32,
        depth: f32,
        scale: f32,
        baseline: f32,
        rise: f32,
        spring: f32,
        window: bool,
        collapsed: bool,
        skip_support: i32,
        variant: u32,
    ) {
        let leg = (2.6 + random(self.seed, variant + 1) * 0.7) * scale;
        for side in [-1., 1.] {
            if side as i32 == skip_support {
                continue;
            }
            let salt = variant + (side + 1.) as u32 * 53;
            let c = [cx + side * half, side * 0.22 * depth];
            let h = (baseline + spring + 4.6 * scale - self.local_ground(c)).max(5.);
            self.column(c, [leg, depth * 1.14], h, side * 0.17, salt + 10, false);
            self.talus(
                [c[0] + side * 1.8 * scale, c[1]],
                [5.8 * scale, depth * 1.85],
                h * 0.83,
                side * 0.18,
                salt + 40,
            );
            self.talus(
                [c[0] + side * 4.7 * scale, c[1] - depth * 0.65],
                [5.7 * scale, depth * 1.52],
                h * 0.39,
                side * 0.35,
                salt + 70,
            );
        }
        let slabs = 5 + (random(self.seed, variant + 130) * 4.) as usize;
        let weights: Vec<f32> = (0..slabs)
            .map(|i| 0.6 + random(self.seed, variant + 140 + i as u32) * 0.9)
            .collect();
        let total: f32 = weights.iter().sum();
        let mut left = -1.;
        let profile = |u: f32| baseline + spring + rise * (1. - u * u).max(0.).sqrt() + u * scale;
        for (i, w) in weights.iter().enumerate() {
            let right = if i + 1 == slabs {
                1.
            } else {
                left + 2. * w / total
            };
            if !collapsed || !(i == slabs / 2 || i + 1 == slabs / 2) {
                self.beam(
                    cx + left * half * 0.95 - 0.30 * scale,
                    cx + right * half * 0.95 + 0.30 * scale,
                    (random(self.seed, variant + 160 + i as u32) - 0.5) * 0.30 * depth,
                    depth * (0.90 + random(self.seed, variant + 180 + i as u32) * 0.28),
                    profile(left),
                    profile(right),
                    (2.3 + random(self.seed, variant + 200 + i as u32) * 1.9) * scale,
                );
            }
            left = right;
        }
        if window {
            let parts = if random(self.seed, variant + 240) < 0.45 {
                1
            } else {
                2
            };
            for i in 0..parts {
                let x0 = -half + 2. * half * i as f32 / parts as f32;
                let x1 = -half + 2. * half * (i + 1) as f32 / parts as f32;
                self.beam(
                    cx + x0 - 0.2 * scale,
                    cx + x1 + 0.2 * scale,
                    0.,
                    depth,
                    baseline + (5.1 + 1.0 * i as f32) * scale,
                    baseline + (5.9 - 0.25 * i as f32) * scale,
                    3.0 * scale,
                );
            }
        }
        if collapsed {
            // Fallen pieces collect beside the cleft; neither entrance becomes a dam.
            for i in 0..2 {
                self.talus(
                    [cx + half + 3. * scale, (i as f32 - 0.5) * 6. * scale],
                    [4.5 * scale, 3.5 * scale],
                    (2.2 + i as f32) * scale,
                    0.3 + i as f32,
                    variant + 260 + i * 23,
                );
            }
        }
    }
    /// Extend the parent rock fabric along geological strike. These connected low
    /// shelves and broken fins bridge the main mass into its actual hillside.
    fn landform_skirt(&mut self, rarity: NaturalRarity, scale: f32, kind: NaturalKind) {
        let mut left = (f32::INFINITY, 0.);
        let mut right = (f32::NEG_INFINITY, 0.);
        for solid in self.solids.iter().filter(|s| !s.detail) {
            for p in &solid.footprint {
                let local = rotated(p[0] - self.x, p[1] - self.z, -self.yaw);
                if local[0] < left.0 {
                    left = (local[0], local[1]);
                }
                if local[0] > right.0 {
                    right = (local[0], local[1]);
                }
            }
        }
        let layers = match rarity {
            NaturalRarity::Common => 2,
            NaturalRarity::Rare => 3,
            NaturalRarity::Monumental => 4,
        };
        let reach = scale.min(1.35);
        for (side, edge) in [(-1., left), (1., right)] {
            for i in 0..layers {
                let salt = 3700 + (side + 1.) as u32 * 80 + i * 19;
                let decay = 1. - i as f32 * 0.16;
                let center = [
                    edge.0 + side * (i as f32 * 5.5 - 2.) * reach,
                    edge.1 + (random(self.seed, salt) - 0.5) * 5. * reach,
                ];
                let width = (9. + random(self.seed, salt + 1) * 4.) * reach * decay;
                let depth = (5. + random(self.seed, salt + 2) * 3.) * reach * decay;
                let h = (3.1 + random(self.seed, salt + 3) * 3.8) * scale * decay;
                self.talus(
                    center,
                    [width, depth],
                    h,
                    if side > 0. { 0.12 } else { PI - 0.12 },
                    salt + 4,
                );
                if i == 0
                    && rarity != NaturalRarity::Common
                    && matches!(
                        kind,
                        NaturalKind::GraniteTor
                            | NaturalKind::BrokenEscarpment
                            | NaturalKind::CoastalStacks
                    )
                {
                    self.column(
                        [center[0], center[1] + depth * 0.1],
                        [width * 0.55, depth * 0.32],
                        h * 1.25,
                        0.08,
                        salt + 14,
                        false,
                    );
                }
            }
        }
    }
}

fn recipe(
    world: &World,
    seed: u32,
    kind: NaturalKind,
    x: f32,
    z: f32,
    yaw: f32,
    color: [f32; 3],
) -> NaturalLandmark {
    let rarity = NaturalRarity::from_id(seed);
    let scale = rarity.scale(seed);
    let mut b = Builder {
        w: world,
        seed,
        x,
        z,
        yaw,
        color,
        solids: Vec::new(),
    };
    match kind {
        NaturalKind::Arch | NaturalKind::StoneWindow => {
            let half = (9. + random(seed, 101) * 4.) * scale;
            let depth = (2.7 + random(seed, 102) * 1.6) * scale;
            let rise = (8. + random(seed, 103) * 6.) * scale;
            let spring = (if kind == NaturalKind::StoneWindow {
                12.7
            } else {
                6.5
            }) * scale;
            let extra = match rarity {
                NaturalRarity::Common => 0,
                NaturalRarity::Rare => usize::from(random(seed, 3210) < 0.62),
                NaturalRarity::Monumental => 1 + usize::from(random(seed, 3210) < 0.35),
            };
            let mut spans: Vec<(f32, f32, bool, i32)> = vec![(0., half, false, 0)];
            for i in 0..extra {
                let side = if i == 0 {
                    if random(seed, 3211) < 0.5 {
                        -1.
                    } else {
                        1.
                    }
                } else {
                    -spans[1].0.signum()
                };
                let secondary = half * (0.57 + random(seed, 3212 + i as u32) * 0.28);
                spans.push((
                    side * (half + secondary),
                    secondary,
                    random(seed, 3215 + i as u32) < 0.38,
                    -side as i32,
                ));
            }
            let baseline = spans
                .iter()
                .flat_map(|&(cx, h, _, _)| [[cx - h, 0.], [cx + h, 0.], [cx, 0.]])
                .map(|p| b.local_ground(p))
                .fold(f32::NEG_INFINITY, f32::max);
            for (i, &(cx, h, collapsed, skip)) in spans.iter().enumerate() {
                b.arch_span(
                    cx,
                    h,
                    depth * (if i == 0 { 1. } else { 0.85 }),
                    scale,
                    baseline,
                    rise * (h / half),
                    spring,
                    kind == NaturalKind::StoneWindow
                        && (i == 0 || random(seed, 3220 + i as u32) < 0.5),
                    collapsed,
                    skip,
                    1000 + i as u32 * 400,
                );
            }
            // Even a common arch can retain a short detached shoulder of an older
            // collapsed span; the main opening is always an intact usable passage.
            if extra == 0 && random(seed, 3229) < 0.30 {
                let side = if random(seed, 3230) < 0.5 { -1. } else { 1. };
                b.spire(
                    [side * (half + 9. * scale), depth * 0.4],
                    [3.0 * scale, depth],
                    9. * scale,
                    0.2,
                    3240,
                    true,
                );
            }
        }
        NaturalKind::Pinnacles => {
            let count = rarity.count(seed, 3301, [(3, 6), (5, 9), (8, 12)]);
            let pattern = random(seed, 3302);
            for i in 0..count {
                let a = i as f32 * 2.39996 + random(seed, 201 + i) * 0.42;
                let spread = (6. + (i as f32).sqrt() * 6.) * scale;
                let c = if pattern < 0.48 {
                    [
                        (i as f32 - (count - 1) as f32 * 0.5) * 5.6 * scale,
                        (random(seed, 221 + i) - 0.5) * 12. * scale,
                    ]
                } else {
                    [a.cos() * spread, a.sin() * spread * 0.66]
                };
                let height = (15. + random(seed, 241 + i) * 25.) * scale;
                let size = (2.0 + random(seed, 261 + i) * 2.3) * scale;
                b.spire(
                    c,
                    [size * 0.86, size * 0.65],
                    height,
                    a,
                    4000 + i * 300,
                    random(seed, 281 + i) < 0.28,
                );
                if i % 2 == 0 {
                    b.talus(
                        [c[0] + size * 0.2, c[1]],
                        [size * 1.65, size * 1.35],
                        height * 0.32,
                        a + 0.35,
                        4200 + i * 300,
                    );
                }
            }
        }
        NaturalKind::GraniteTor => {
            let count = rarity.count(seed, 3310, [(3, 5), (5, 7), (6, 9)]);
            let curved = random(seed, 3311) > 0.45;
            for i in 0..count {
                let along = (i as f32 - (count - 1) as f32 * 0.5) * 8.3 * scale;
                let c = [
                    along,
                    if curved {
                        (along / 17. / scale).sin() * 7. * scale
                    } else {
                        (random(seed, 301 + i) - 0.5) * 9. * scale
                    },
                ];
                let h = (7. + random(seed, 321 + i) * 12.) * scale;
                let size = [
                    (4.6 + random(seed, 341 + i) * 1.4) * scale,
                    (3.7 + random(seed, 361 + i)) * scale,
                ];
                b.column(
                    c,
                    size,
                    h,
                    0.2 * (random(seed, 381 + i) - 0.5),
                    5000 + i * 100,
                    false,
                );
                if random(seed, 391 + i) < 0.72 {
                    let contact = b
                        .solids
                        .last()
                        .unwrap()
                        .top
                        .iter()
                        .copied()
                        .fold(f32::INFINITY, f32::min)
                        - 0.7;
                    b.cap(
                        [c[0] + scale * 0.25, c[1]],
                        [size[0] * 1.04, size[1] * 0.98],
                        contact,
                        (2.4 + random(seed, 401 + i) * 3.5) * scale,
                        0.17,
                        5060 + i * 100,
                    );
                }
            }
        }
        NaturalKind::BoulderRing => {
            let count = rarity.count(seed, 3320, [(6, 8), (8, 11), (11, 14)]);
            let open = (0.20 + random(seed, 3321) * 0.14) * PI;
            for i in 0..count {
                let a = TAU * i as f32 / count as f32 + (random(seed, 601 + i) - 0.5) * 0.13;
                // Both N/S approaches remain open, even when the broken ring is asymmetric.
                if a.cos().abs() < open.sin() {
                    continue;
                }
                let r = (15. + random(seed, 621 + i) * 8.) * scale;
                let c = [
                    a.cos() * r,
                    a.sin() * r * (0.72 + random(seed, 641 + i) * 0.34),
                ];
                b.spire(
                    c,
                    [(3.4 + random(seed, 661 + i) * 1.5) * scale, 3.0 * scale],
                    (5. + random(seed, 681 + i) * 8.) * scale,
                    a,
                    6000 + i * 100,
                    random(seed, 691 + i) < 0.66,
                );
            }
        }
        NaturalKind::BrokenEscarpment => {
            for side in [-1., 1.] {
                let ss = (side + 1.) as u32 * 81;
                let count = rarity.count(seed, 3330 + ss, [(2, 3), (3, 4), (4, 5)]);
                for i in 0..count {
                    let c = [
                        side * (8.6 + i as f32 * 8.0) * scale,
                        (random(seed, 701 + i + ss) - 0.5) * 8. * scale,
                    ];
                    let h = (10. + i as f32 * 2. + random(seed, 721 + i + ss) * 8.) * scale;
                    let size = [
                        (4.7 + random(seed, 741 + i + ss)) * scale,
                        (2.5 + random(seed, 761 + i + ss) * 1.6) * scale,
                    ];
                    b.column(c, size, h, side * 0.08, 7000 + i * 200 + ss, false);
                    if random(seed, 781 + i + ss) < 0.63 {
                        let contact = b
                            .solids
                            .last()
                            .unwrap()
                            .top
                            .iter()
                            .copied()
                            .fold(f32::INFINITY, f32::min)
                            - 0.6;
                        b.cap(
                            c,
                            [size[0] * 1.07, size[1] * 1.13],
                            contact,
                            2.0 * scale,
                            0.1,
                            7060 + i * 200 + ss,
                        );
                    }
                    if i % 2 == 0 {
                        b.talus(
                            [c[0] + side * scale, c[1] - size[1]],
                            [size[0] * 1.16, size[1] * 1.8],
                            h * 0.38,
                            side * 0.22,
                            7100 + i * 200 + ss,
                        );
                    }
                }
            }
        }
        NaturalKind::BasaltPipes => {
            let count = rarity.count(seed, 3340, [(9, 16), (16, 24), (24, 34)]);
            let lobes = 1 + (random(seed, 3341) * (rarity.index() + 2) as f32) as u32;
            let sweep = (0.8 + random(seed, 3342) * 1.5) * PI;
            for i in 0..count {
                let lobe = i % lobes;
                let k = i / lobes;
                let a = k as f32 * 2.39996 + random(seed, 901 + i) * 0.45;
                let r = (1.5 + (k as f32).sqrt() * 2.25) * scale;
                let bend =
                    (lobe as f32 - (lobes - 1) as f32 * 0.5) * sweep / (lobes as f32).max(1.);
                let c = [
                    bend.sin() * 10. * scale + a.cos() * r,
                    bend.cos() * 5. * scale + a.sin() * r * 0.83,
                ];
                let h = (9. + random(seed, 921 + i) * 15. + (1. - k as f32 / count as f32) * 5.)
                    * scale;
                let width = (1.50 + random(seed, 941 + i) * 0.57) * scale;
                b.column(
                    c,
                    [width, width * 0.91],
                    h,
                    PI / 6. + random(seed, 961 + i) * 0.18,
                    8000 + i * 41,
                    false,
                );
                // Different broken roof planes retain true basalt columns while
                // removing the old identical row/column silhouette.
                if random(seed, 981 + i) < 0.32 {
                    let p = b.solids.last_mut().unwrap();
                    let base = p.bottom.iter().copied().fold(f32::NEG_INFINITY, f32::max) + EMBED;
                    for (j, y) in p.top.iter_mut().enumerate() {
                        *y = base + h * (0.49 + j as f32 * 0.045);
                    }
                }
            }
        }
        NaturalKind::SplitMonolith => {
            let h = (22. + random(seed, 1201) * 14.) * scale;
            for side in [-1., 1.] {
                let c = [
                    side * (6.4 + random(seed, 1202 + (side + 1.) as u32) * 0.8) * scale,
                    side * 1.3 * scale,
                ];
                b.spire(
                    c,
                    [3.6 * scale, 5.5 * scale],
                    h * (if side < 0. { 1. } else { 0.82 }),
                    side * 0.06,
                    10000 + (side + 1.) as u32 * 200,
                    false,
                );
                if random(seed, 1207 + (side + 1.) as u32) < 0.66 {
                    b.talus(
                        [c[0] + side * 4. * scale, c[1]],
                        [5.6 * scale, 4.8 * scale],
                        h * 0.34,
                        side * 0.2,
                        10400 + (side + 1.) as u32 * 80,
                    );
                }
            }
        }
        NaturalKind::CoastalStacks => {
            let count = rarity.count(seed, 3350, [(2, 4), (3, 5), (5, 7)]);
            let curve = (random(seed, 3351) - 0.5) * 1.9;
            for i in 0..count {
                let t = i as f32 - (count - 1) as f32 * 0.5;
                let c = [t * 10.8 * scale, (t * 0.57 + curve).sin() * 7. * scale];
                let h = (14. + random(seed, 1401 + i) * 19.) * scale;
                let size = [
                    (3.4 + random(seed, 1421 + i) * 1.5) * scale,
                    (2.8 + random(seed, 1441 + i)) * scale,
                ];
                b.spire(
                    c,
                    size,
                    h,
                    0.15 * i as f32,
                    11000 + i * 300,
                    random(seed, 1461 + i) < 0.24,
                );
            }
        }
        NaturalKind::Hoodoos => {
            let count = rarity.count(seed, 3360, [(3, 6), (6, 9), (8, 12)]);
            let wandering = random(seed, 3361) < 0.45;
            for i in 0..count {
                let a = i as f32 * 2.39996 + random(seed, 1601 + i) * 0.3;
                let r = (5. + (i as f32).sqrt() * 5.0) * scale;
                let c = if wandering {
                    [
                        (i as f32 - (count - 1) as f32 * 0.5) * 4.4 * scale,
                        (i as f32 * 0.9).sin() * 7. * scale,
                    ]
                } else {
                    [a.cos() * r, a.sin() * r * 0.77]
                };
                let width = (1.7 + random(seed, 1621 + i)) * scale;
                let h = (8. + random(seed, 1641 + i) * 13.) * scale;
                b.column(c, [width, width * 0.83], h, a, 12000 + i * 90, false);
                if random(seed, 1661 + i) > 0.18 {
                    let contact = b
                        .solids
                        .last()
                        .unwrap()
                        .top
                        .iter()
                        .copied()
                        .fold(f32::INFINITY, f32::min)
                        - 0.5;
                    b.cap(
                        [c[0] + width * 0.14, c[1]],
                        [width * (1.25 + random(seed, 1681 + i) * 0.55), width * 1.34],
                        contact,
                        2.2 * scale,
                        a + 0.13,
                        12050 + i * 90,
                    );
                }
            }
        }
    }
    b.landform_skirt(rarity, scale, kind);
    let debris = rarity.count(seed, 3380, [(3, 6), (5, 8), (7, 10)]);
    // Scattered chips stay close to actual supports and terminate the parent rock
    // fabric, rather than describing the same arbitrary ring around every family.
    let anchors: Vec<[f32; 2]> = b
        .solids
        .iter()
        .filter(|s| s.grounded)
        .map(|s| {
            let p = s
                .footprint
                .iter()
                .fold([0.; 2], |a, p| [a[0] + p[0], a[1] + p[1]]);
            rotated(
                p[0] / s.footprint.len() as f32 - x,
                p[1] / s.footprint.len() as f32 - z,
                -yaw,
            )
        })
        .collect();
    for i in 0..debris {
        let anchor =
            anchors[(random(seed, 2001 + i) * anchors.len() as f32) as usize % anchors.len()];
        let a = random(seed, 2021 + i) * TAU;
        let c = [
            anchor[0] + a.cos() * (2. + random(seed, 2041 + i) * 4.) * scale,
            anchor[1] + a.sin() * (2. + random(seed, 2061 + i) * 4.) * scale,
        ];
        // Central passages remain intentionally free of loose collider debris.
        if c[0].abs() < 3.5 * scale
            && matches!(
                kind,
                NaturalKind::Arch
                    | NaturalKind::StoneWindow
                    | NaturalKind::BoulderRing
                    | NaturalKind::BrokenEscarpment
                    | NaturalKind::SplitMonolith
            )
        {
            continue;
        }
        b.column(
            c,
            [(0.8 + random(seed, 2081 + i)) * scale, 0.9 * scale],
            (0.5 + random(seed, 2101 + i)) * scale,
            a,
            13000 + i * 23,
            true,
        );
    }
    let radius = b
        .solids
        .iter()
        .flat_map(|s| s.footprint.iter())
        .map(|p| (p[0] - x).hypot(p[1] - z))
        .fold(0., f32::max)
        + 0.5;
    let top = b
        .solids
        .iter()
        .flat_map(|s| s.top.iter().copied())
        .fold(f32::NEG_INFINITY, f32::max);
    NaturalLandmark {
        id: seed,
        name: format!(
            "{} {}",
            [
                "Elder", "Cinder", "Storm", "Pale", "Moon", "Hollow", "Whisper", "Sable", "Copper",
                "Frost", "Amber", "Mist"
            ][(seed as usize >> 8) % 12],
            match kind {
                NaturalKind::Arch => "Arch",
                NaturalKind::StoneWindow => "Window",
                NaturalKind::Pinnacles => "Needles",
                NaturalKind::GraniteTor => "Crown",
                NaturalKind::BoulderRing => "Amphitheatre",
                NaturalKind::BrokenEscarpment => "Gates",
                NaturalKind::BasaltPipes => "Organ",
                NaturalKind::SplitMonolith => "Cleft",
                NaturalKind::CoastalStacks => "Sentinels",
                NaturalKind::Hoodoos => "Spires",
            }
        ),
        kind,
        x,
        z,
        ground: ground(world, x, z),
        radius,
        top,
        yaw,
        solids: b.solids,
    }
}

/// Ground-height camera chosen outside all monument solids and freshwater. It
/// favors a three-quarter view so openings read clearly without moving scenery.
pub fn scenic_view(world: &World, n: &NaturalLandmark) -> Option<NaturalView> {
    // First rank cheap ground/sightline checks. Only the eight best candidates
    // need the more expensive shared prop collision queries along the approach.
    let mut candidates = Vec::with_capacity(36);
    for distance in [1.35, 1.55, 2.0] {
        for side in [-1., 1.] {
            for offset in [-0.70, -0.43, -0.18, 0.18, 0.43, 0.70] {
                let angle = n.yaw + side * PI * 0.5 + offset;
                let x = n.x + angle.cos() * n.radius * distance;
                let z = n.z + angle.sin() * n.radius * distance;
                let s = world.sample(x, z);
                let h = ground(world, x, z);
                if s.ocean
                    || h < s.water_height + 0.5
                    || crate::geometry::blocks_player(world, x, z)
                {
                    continue;
                }
                let grade = ((ground(world, x + 2., z) - ground(world, x - 2., z)).powi(2)
                    + (ground(world, x, z + 2.) - ground(world, x, z - 2.)).powi(2))
                .sqrt()
                    / 4.;
                if grade > 0.60 {
                    continue;
                }
                let target = n.ground + (n.top - n.ground) * 0.48;
                let mut visible = 0;
                for fraction in [0.2, 0.4, 0.6, 0.8] {
                    let tx = x + (n.x - x) * fraction;
                    let tz = z + (n.z - z) * fraction;
                    if ground(world, tx, tz) < h + 1.72 + (target - h - 1.72) * fraction - 0.4 {
                        visible += 1;
                    }
                }
                if visible < 3 {
                    continue;
                }
                let horizontal = (n.x - x).hypot(n.z - z);
                let pitch = ((target - h - 1.72) / horizontal).atan().clamp(-0.30, 0.38);
                // Leave a little frame above the crest even at the closer view.
                if ((n.top - h - 1.72) / horizontal).atan() - pitch > 0.58 {
                    continue;
                }
                let quality = visible as f32 * 5.
                    - grade * 6.
                    - (distance - 1.55).abs() * 1.2
                    - (offset.abs() - 0.43).abs() * 0.30;
                candidates.push((
                    quality,
                    NaturalView {
                        x,
                        z,
                        yaw: (n.x - x).atan2(z - n.z),
                        pitch,
                    },
                ));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut best = None;
    let mut score = f32::NEG_INFINITY;
    'candidate: for (quality, view) in candidates.into_iter().take(8) {
        // Obstructions can only lower the score. A clear leading view wins
        // immediately, and later candidates stop as soon as they cannot beat it.
        if quality <= score {
            break;
        }
        let length = (n.x - view.x).hypot(n.z - view.z);
        let across = [(n.z - view.z) / length, -(n.x - view.x) / length];
        let mut obstructions = 0;
        let mut near_blocked = false;
        for step in 1..=12 {
            let fraction = step as f32 / 15.;
            let x = view.x + (n.x - view.x) * fraction;
            let z = view.z + (n.z - view.z) * fraction;
            // A narrow pair of rays catches foreground trunks without treating
            // every tree beside a good view as a reason to reject woodland.
            for side in [-0.55, 0.55] {
                let x = x + across[0] * side;
                let z = z + across[1] * side;
                let floor = ground(world, x, z);
                // Ignore this monument's own supports. We are evaluating the
                // approach to it, not demanding that solid rock be transparent.
                if n.blocks(x, floor, z, 0.4, 1.8) {
                    continue;
                }
                if crate::geometry::blocks_player(world, x, z) {
                    obstructions += 1;
                    near_blocked |= fraction * length < 5.;
                    if near_blocked
                        || obstructions >= 6
                        || quality - obstructions as f32 * 3. <= score
                    {
                        continue 'candidate;
                    }
                }
            }
        }
        if near_blocked || obstructions >= 6 {
            continue;
        }
        let value = quality - obstructions as f32 * 3.;
        if value > score {
            score = value;
            best = Some(view);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    const KINDS: [NaturalKind; 10] = [
        NaturalKind::Arch,
        NaturalKind::StoneWindow,
        NaturalKind::Pinnacles,
        NaturalKind::GraniteTor,
        NaturalKind::BoulderRing,
        NaturalKind::BrokenEscarpment,
        NaturalKind::BasaltPipes,
        NaturalKind::SplitMonolith,
        NaturalKind::CoastalStacks,
        NaturalKind::Hoodoos,
    ];
    #[test]
    fn every_family_is_finite_grounded_bounded_and_preserves_major_lod_envelopes() {
        let w = World::new(1337);
        let p = w.spawn();
        let mut max_near = [0; 3];
        let mut max_far = [0; 3];
        let mut max_radius = [0.0_f32; 3];
        let mut seeds = [Vec::new(), Vec::new(), Vec::new()];
        for seed in 0..10000 {
            let k = NaturalRarity::from_id(seed).index();
            if seeds[k].len() < 8 {
                seeds[k].push(seed);
            }
        }
        for kind in KINDS {
            for &seed in seeds.iter().flatten() {
                let rarity = NaturalRarity::from_id(seed);
                let k = rarity.index();
                let n = recipe(&w, seed, kind, p[0], p[1], 0.47, [0.5, 0.45, 0.35]);
                assert!(n.radius <= MAX_RADIUS, "{:?} {}", kind, n.radius);
                assert!(n.solids.len() <= 80);
                let mut near = MeshData::default();
                let mut far = MeshData::default();
                n.append_mesh(&w, &mut near, false, 0);
                n.append_mesh(&w, &mut far, true, 0);
                assert!(!near.indices.is_empty() && !far.indices.is_empty());
                assert!(
                    near.indices.len() / 3 <= if k == 0 { 1800 } else { 3500 },
                    "{kind:?} {rarity:?} near{}",
                    near.indices.len() / 3
                );
                assert!(
                    far.indices.len() / 3 <= if k == 0 { 600 } else { 1000 },
                    "{kind:?} {rarity:?} far{}",
                    far.indices.len() / 3
                );
                max_near[k] = max_near[k].max(near.indices.len() / 3);
                max_far[k] = max_far[k].max(far.indices.len() / 3);
                max_radius[k] = max_radius[k].max(n.radius);
                for v in &near.vertices {
                    assert!(v.position.iter().chain(&v.normal).all(|v| v.is_finite()));
                }
                for s in n.solids.iter().filter(|s| s.grounded) {
                    for (p, b) in s.footprint.iter().zip(&s.bottom) {
                        assert!((*b - ground(&w, p[0], p[1]) + EMBED).abs() < 0.005);
                    }
                }
                // Structural footprints and tops survive far LOD exactly, including
                // each arch soffit. There is no enclosing box that could fill a gap.
                for s in n.solids.iter().filter(|s| !s.detail) {
                    for (p, t) in s.footprint.iter().zip(&s.top) {
                        assert!(near.vertices.iter().any(|v| v.position == [p[0], *t, p[1]]));
                        assert!(far.vertices.iter().any(|v| v.position == [p[0], *t, p[1]]));
                    }
                }
            }
        }
        println!(
            "natural landmark tier budgets: near{max_near:?},far{max_far:?},radius{max_radius:?}"
        );
    }
    #[test]
    fn topology_changes_with_seed_and_rarity_is_not_only_a_scale_label() {
        let w = World::new(1337);
        let p = w.spawn();
        let mut frequencies = [0; 3];
        let mut seeds = [Vec::new(), Vec::new(), Vec::new()];
        for seed in 0..10000 {
            let k = NaturalRarity::from_id(seed).index();
            frequencies[k] += 1;
            if seeds[k].len() < 12 {
                seeds[k].push(seed);
            }
        }
        assert!((frequencies[0] as i32 - 8600).abs() < 250);
        assert!((frequencies[1] as i32 - 1200).abs() < 150);
        assert!((frequencies[2] as i32 - 200).abs() < 75);
        for kind in KINDS {
            let mut variants = std::collections::HashSet::new();
            let mut component_counts = [Vec::new(), Vec::new(), Vec::new()];
            for (k, tier_seeds) in seeds.iter().enumerate() {
                for &seed in tier_seeds {
                    let n = recipe(&w, seed, kind, p[0], p[1], 0.41, [0.5; 3]);
                    let solids: Vec<_> = n.solids.iter().filter(|s| !s.detail).collect();
                    variants.insert((solids.len(), solids.iter().filter(|s| !s.grounded).count()));
                    component_counts[k].push(solids.len());
                    if matches!(kind, NaturalKind::Arch | NaturalKind::StoneWindow) {
                        for along in -4..=4 {
                            let q = rotated(0., along as f32 * 1.5, n.yaw);
                            let (x, z) = (n.x + q[0], n.z + q[1]);
                            assert!(
                                !n.blocks(x, ground(&w, x, z), z, 0.4, 2.15),
                                "primary passage lost in{kind:?} seed{seed}"
                            );
                        }
                    }
                }
            }
            assert!(
                variants.len() >= 4,
                "{kind:?} topology repeated across tiers"
            );
            if !matches!(kind, NaturalKind::SplitMonolith) {
                assert!(
                    component_counts[2].iter().max() > component_counts[0].iter().max(),
                    "{kind:?} monumental tier never gains structure"
                );
            }
        }
        println!("candidate rarity census over10000 ids:{frequencies:?}");
    }

    #[test]
    fn structural_roofs_connect_back_to_grounded_supports() {
        let w = World::new(1337);
        let p = w.spawn();
        let seeds = [Vec::new(), Vec::new(), Vec::new()];
        let mut seeds = seeds;
        for seed in 0..10000 {
            let k = NaturalRarity::from_id(seed).index();
            if seeds[k].len() < 6 {
                seeds[k].push(seed);
            }
        }
        for kind in KINDS {
            for &seed in seeds.iter().flatten() {
                let n = recipe(&w, seed, kind, p[0], p[1], 0.41, [0.5; 3]);
                let solids: Vec<_> = n.solids.iter().filter(|s| !s.detail).collect();
                let mut connected: Vec<_> = solids.iter().map(|s| s.grounded).collect();
                for _ in 0..solids.len() {
                    let mut changed = false;
                    for (i, a) in solids.iter().enumerate() {
                        if connected[i] {
                            continue;
                        }
                        for (j, b) in solids.iter().enumerate() {
                            if !connected[j] {
                                continue;
                            }
                            let mut probes = Vec::new();
                            for s in [*a, *b] {
                                let center = s
                                    .footprint
                                    .iter()
                                    .fold([0.; 2], |a, p| [a[0] + p[0], a[1] + p[1]])
                                    .map(|v| v / s.footprint.len() as f32);
                                probes.push(center);
                                for k in 0..s.footprint.len() {
                                    let x = s.footprint[k];
                                    let y = s.footprint[(k + 1) % s.footprint.len()];
                                    probes.push(x);
                                    probes.push([(x[0] + y[0]) * 0.5, (x[1] + y[1]) * 0.5]);
                                }
                            }
                            if probes.into_iter().any(|p| {
                                a.contains_xz(p[0], p[1], 0.03)
                                    && b.contains_xz(p[0], p[1], 0.03)
                                    && a.height_at(p, &a.bottom).max(b.height_at(p, &b.bottom))
                                        <= a.height_at(p, &a.top).min(b.height_at(p, &b.top)) + 0.07
                            }) {
                                connected[i] = true;
                                changed = true;
                                break;
                            }
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                assert!(
                    connected.iter().all(|v| *v),
                    "unsupported roof piece in{kind:?},seed{seed},connected{connected:?}"
                );
            }
        }
    }

    #[test]
    fn overhead_wedge_allows_walking_below_but_blocks_its_actual_solid() {
        let s = Solid {
            footprint: vec![[-4., -2.], [4., -2.], [4., 2.], [-4., 2.]],
            bottom: vec![4., 8., 8., 4.],
            top: vec![7., 11., 11., 7.],
            grounded: false,
            detail: false,
            color: [0.5; 3],
        };
        for x in -3..=3 {
            assert!(!s.blocks(x as f32, 0., 0., 0.35, 1.9));
            assert!(s.blocks(x as f32, 6.5, 0., 0.35, 1.9));
        }
        assert!(!s.blocks(0., 12., 0., 0.35, 1.9));
        assert!(!s.blocks(5., 7., 0., 0.35, 1.9));
    }
    #[test]
    fn actual_landmarks_are_deterministic_clear_and_cross_cell_queries_find_colliders() {
        let w = World::new(1337);
        let mut count = 0;
        let mut kinds = std::collections::HashSet::new();
        'scan: for gz in -95..95 {
            for gx in -95..95 {
                let Some(n) = landmark_at(&w, gx, gz) else {
                    continue;
                };
                count += 1;
                kinds.insert(format!("{:?}", n.kind));
                let again = landmark_at(&w, gx, gz).unwrap();
                assert_eq!(
                    serde_json::to_string(&n).unwrap(),
                    serde_json::to_string(&again).unwrap()
                );
                assert!(eligible_footprint(&w, &n));
                let solid = n.solids.iter().find(|s| s.grounded && !s.detail).unwrap();
                let x = solid.footprint.iter().map(|p| p[0]).sum::<f32>()
                    / solid.footprint.len() as f32;
                let z = solid.footprint.iter().map(|p| p[1]).sum::<f32>()
                    / solid.footprint.len() as f32;
                let feet = ground(&w, x, z);
                assert!(blocks_player(&w, x, feet, z, 0.35, 1.8));
                assert!(landmarks_near(&w, x, z, 0.5).iter().any(|a| a.id == n.id));
                if count >= 40 && kinds.len() >= 8 {
                    break 'scan;
                }
            }
        }
        assert!(
            count >= 20 && kinds.len() >= 6,
            "{count} real monuments, {} kinds",
            kinds.len()
        );
        println!(
            "validated {count} real natural landmarks, {} families",
            kinds.len()
        );
    }
}
