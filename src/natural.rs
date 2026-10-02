//! Sparse natural monuments built entirely from shared deterministic solid recipes.
//! Every opening is genuinely empty geometry. Collision tests individual solids,
//! including their actual sloped underside, rather than a landmark-wide cylinder.
pub mod catalog;
mod recipes;
use crate::geometry::{terrain_surface_height_lod, MeshData, CHUNK_SIZE};
use crate::regions::{Geology, LandscapeKind};
use crate::world::{hash, rand01, ShoreKind, World};
use recipes::recipe;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};

pub const CELL_SIZE: f32 = crate::worldgen::NATURAL.cell_size;
pub const MAX_RADIUS: f32 = crate::worldgen::NATURAL.max_radius;
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
    /// Persistent identity is independent of the random hash used for appearance.
    pub fn stable_id(&self) -> crate::worldgen::LocationId {
        crate::worldgen::LocationId::cell(
            "natural",
            (self.x / CELL_SIZE).floor() as i32,
            (self.z / CELL_SIZE).floor() as i32,
            0,
        )
    }
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
    if random(id, 1) > crate::worldgen::NATURAL.candidate_probability {
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
    if region.slope > catalog::definition(kind).max_slope {
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
    if world.settlements.clears(
        n.x,
        n.z,
        n.radius + crate::worldgen::NATURAL.settlement_clearance,
    ) {
        return false;
    }
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
    let definition = catalog::definition(n.kind);
    if hi - lo > definition.max_support_relief {
        return false;
    }
    // Passage families require walking clearance across the actual opening.
    if definition.walk_through {
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
