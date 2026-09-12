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
pub const MAX_RADIUS: f32 = 76.;
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
fn recipe(
    world: &World,
    seed: u32,
    kind: NaturalKind,
    x: f32,
    z: f32,
    yaw: f32,
    color: [f32; 3],
) -> NaturalLandmark {
    let scale = 0.78 + random(seed, 100) * 0.54;
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
            let depth = (3. + random(seed, 102) * 1.4) * scale;
            let leg = 3.0 * scale;
            let baseline = [[-half, 0.], [half, 0.], [0., 0.]]
                .map(|p| b.local_ground(p))
                .into_iter()
                .fold(f32::NEG_INFINITY, f32::max);
            let spring = baseline
                + (if kind == NaturalKind::StoneWindow {
                    13.
                } else {
                    6.4
                }) * scale;
            let rise = (8. + random(seed, 103) * 6.) * scale;
            for side in [-1., 1.] {
                let variation = (side + 1.) as u32 * 45;
                let c = [side * half, side * 0.30 * depth];
                let h = (spring + 4. * scale - b.local_ground(c)).max(5.);
                b.column(
                    c,
                    [
                        leg * (0.91 + random(seed, 110 + variation) * 0.20),
                        depth * 1.18,
                    ],
                    h,
                    0.16 * side,
                    120 + variation,
                    false,
                );
                // Unequal buttresses merge the columns into a weathered rock fin;
                // both through-entrances and the center corridor remain empty.
                b.talus(
                    [c[0] + side * 2.0 * scale, c[1]],
                    [6.8 * scale, depth * 1.90],
                    h * 0.90,
                    side * 0.15,
                    150 + variation,
                );
                b.talus(
                    [c[0] + side * 4.7 * scale, c[1] + depth * 0.65],
                    [6.2 * scale, depth * 1.70],
                    h * 0.53,
                    side * 0.4,
                    180 + variation,
                );
                b.talus(
                    [c[0] + side * 3.2 * scale, c[1] - depth * 1.2],
                    [4.7 * scale, depth * 1.22],
                    h * 0.40,
                    -side * 0.3,
                    210 + variation,
                );
            }
            let span = half * 0.94;
            // Unequal fracture spans and an asymmetric arch profile avoid an
            // architectural keystone/radial-block rhythm.
            let cuts = [-1.0, -0.84, -0.57, -0.23, 0.15, 0.43, 0.79, 1.0];
            let profile = |u: f32| spring + rise * (1. - u * u).max(0.).sqrt() + u * 1.0 * scale;
            for i in 0..7 {
                b.beam(
                    cuts[i] * span - 0.30 * scale,
                    cuts[i + 1] * span + 0.30 * scale,
                    (random(seed, 270 + i as u32) - 0.5) * 0.38 * depth,
                    depth * (0.90 + random(seed, 290 + i as u32) * 0.32),
                    profile(cuts[i]),
                    profile(cuts[i + 1]),
                    (2.1 + random(seed, 310 + i as u32) * 2.1) * scale,
                );
            }
            if kind == NaturalKind::StoneWindow {
                b.beam(
                    -half,
                    half * 0.17,
                    0.,
                    depth * 1.09,
                    baseline + 4.9 * scale,
                    baseline + 6.5 * scale,
                    3.4 * scale,
                );
                b.beam(
                    half * 0.13,
                    half,
                    0.,
                    depth * 0.95,
                    baseline + 6.3 * scale,
                    baseline + 5.4 * scale,
                    2.8 * scale,
                );
            }
        }
        NaturalKind::Pinnacles => {
            for i in 0..7 {
                let a = i as f32 * 2.39996;
                let r = if i == 0 {
                    0.
                } else {
                    (13. + random(seed, 201 + i) * 13.) * scale
                };
                let height = (15. + random(seed, 221 + i) * 25.) * scale;
                let size = (2.0 + random(seed, 241 + i) * 2.4) * scale;
                let c = [a.cos() * r, a.sin() * r];
                b.column(
                    c,
                    [size * 0.76, size * 0.62],
                    height,
                    a,
                    261 + i * 90,
                    false,
                );
                let top = b
                    .solids
                    .last()
                    .unwrap()
                    .top
                    .iter()
                    .copied()
                    .fold(f32::INFINITY, f32::min);
                b.cap(
                    c,
                    [size * 0.50, size * 0.43],
                    top - 0.5,
                    height * 0.24,
                    a,
                    281 + i * 90,
                );
                b.talus(
                    [c[0] + a.cos() * size * 0.20, c[1] + a.sin() * size * 0.20],
                    [size * 1.85, size * 1.30],
                    height * 0.48,
                    a + 0.45,
                    298 + i * 90,
                );
            }
        }
        NaturalKind::GraniteTor => {
            for i in 0..5 {
                let c = [
                    (i as f32 - 2.) * 10. * scale,
                    ((i % 2) as f32 - 0.5) * 7. * scale,
                ];
                let h = (8. + random(seed, 301 + i) * 12.) * scale;
                let size = [
                    (5.3 + random(seed, 321 + i) * 1.5) * scale,
                    (4.2 + random(seed, 341 + i)) * scale,
                ];
                b.column(c, size, h, 0.17 * (i as f32 - 2.), 360 + i * 20, false);
                let y = b
                    .solids
                    .last()
                    .unwrap()
                    .top
                    .iter()
                    .copied()
                    .fold(f32::INFINITY, f32::min);
                b.cap(
                    c,
                    [size[0] * 1.1, size[1] * 1.12],
                    y - 1.0,
                    4. * scale,
                    0.18,
                    480 + i * 11,
                );
                if i == 2 {
                    b.cap(
                        [c[0] + 0.5 * scale, c[1]],
                        [size[0] * 0.73, size[1] * 0.84],
                        y + 2.6 * scale,
                        5.3 * scale,
                        -0.12,
                        560,
                    );
                }
            }
        }
        NaturalKind::BoulderRing => {
            // Broken natural amphitheatre: irregular arcs, two generous entrances.
            for i in 0..8 {
                let a = TAU * i as f32 / 8. + 0.18;
                if i == 2 || i == 6 {
                    continue;
                }
                let r = (19. + random(seed, 601 + i) * 6.) * scale;
                b.column(
                    [a.cos() * r, a.sin() * r],
                    [(4.2 + random(seed, 621 + i) * 2.3) * scale, 4.0 * scale],
                    (5.0 + random(seed, 641 + i) * 7.) * scale,
                    a,
                    660 + i * 20,
                    false,
                );
            }
        }
        NaturalKind::BrokenEscarpment => {
            // Two banks of layered walls, an actual central cleft and side alcoves.
            for side in [-1., 1.] {
                for i in 0..3 {
                    let c = [
                        side * (9. + i as f32 * 9.) * scale,
                        (if i == 1 { 4. } else { 0. }) * scale,
                    ];
                    let side_salt = (side + 1.) as u32 * 71;
                    let h = (10. + i as f32 * 3. + random(seed, 701 + i + side_salt) * 9.) * scale;
                    let size = [
                        (4.7 + random(seed, 713 + i + side_salt) * 1.2) * scale,
                        (2.5 + random(seed, 719 + i + side_salt) * 1.8) * scale,
                    ];
                    b.column(
                        c,
                        size,
                        h,
                        side * 0.08,
                        730 + i * 20 + (side + 1.) as u32 * 200,
                        false,
                    );
                    let y = b
                        .solids
                        .last()
                        .unwrap()
                        .top
                        .iter()
                        .copied()
                        .fold(f32::INFINITY, f32::min);
                    b.cap(
                        [c[0], c[1] - 0.4 * scale],
                        [size[0] * 1.08, size[1] * 1.16],
                        y - 0.6,
                        2.2 * scale,
                        0.,
                        880 + i * 13,
                    );
                    b.talus(
                        [c[0] + side * 1.7 * scale, c[1] - size[1] * 0.80],
                        [size[0] * 1.18, size[1] * 1.9],
                        h * 0.46,
                        side * 0.28,
                        2310 + i * 17 + side_salt,
                    );
                }
            }
        }
        NaturalKind::BasaltPipes => {
            for row in 0..3 {
                for i in 0..7 {
                    let c = [
                        (i as f32 - 3.) * 3.8 * scale,
                        (row as f32 - 1.) * 3.3 * scale,
                    ];
                    let h = (8.
                        + row as f32 * 6.
                        + (3. - (i as f32 - 3.).abs()) * 3.
                        + random(seed, 901 + i + row * 7) * 4.)
                        * scale;
                    b.column(
                        c,
                        [2.30 * scale, 2.12 * scale],
                        h,
                        PI / 6.,
                        940 + (i + row * 7) * 13,
                        false,
                    );
                }
            }
        }
        NaturalKind::SplitMonolith => {
            let h = (24. + random(seed, 1201) * 15.) * scale;
            for side in [-1., 1.] {
                let c = [side * 6.7 * scale, side * 0.8 * scale];
                b.column(
                    c,
                    [4.1 * scale, 6.5 * scale],
                    h * (if side < 0. { 1. } else { 0.89 }),
                    side * 0.04,
                    1220 + (side + 1.) as u32 * 80,
                    false,
                );
            }
        }
        NaturalKind::CoastalStacks => {
            for i in 0..4 {
                let c = [
                    (i as f32 - 1.5) * 14. * scale,
                    (i as f32 * 1.8).sin() * 9. * scale,
                ];
                let h = (17. + random(seed, 1401 + i) * 19.) * scale;
                b.column(
                    c,
                    [
                        (4. + random(seed, 1421 + i) * 2.) * scale,
                        (3.4 + random(seed, 1441 + i)) * scale,
                    ],
                    h,
                    0.15 * i as f32,
                    1460 + i * 20,
                    false,
                );
            }
        }
        NaturalKind::Hoodoos => {
            for i in 0..7 {
                let a = i as f32 * 2.39996;
                let r = if i == 0 {
                    0.
                } else {
                    (10. + random(seed, 1601 + i) * 12.) * scale
                };
                let c = [a.cos() * r, a.sin() * r];
                let width = (1.9 + random(seed, 1621 + i)) * scale;
                let h = (10. + random(seed, 1641 + i) * 13.) * scale;
                b.column(c, [width, width * 0.9], h, a, 1660 + i * 20, false);
                let y = b
                    .solids
                    .last()
                    .unwrap()
                    .top
                    .iter()
                    .copied()
                    .fold(f32::INFINITY, f32::min);
                b.cap(
                    c,
                    [width * 1.72, width * 1.47],
                    y - 0.5,
                    2.5 * scale,
                    a + 0.2,
                    1800 + i * 13,
                );
            }
        }
    }
    // Loose angular stones hint at erosion; the far LOD omits only these small chips.
    for i in 0..5 {
        let side = if i % 2 == 0 { -1. } else { 1. };
        let c = [
            side * (16. + random(seed, 2001 + i) * 13.) * scale,
            (random(seed, 2021 + i) - 0.5) * 27. * scale,
        ];
        b.column(
            c,
            [1.4 * scale, 0.95 * scale],
            (0.8 + random(seed, 2041 + i)) * scale,
            random(seed, 2061 + i) * TAU,
            2080 + i * 13,
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
        let mut max_near = 0;
        let mut max_far = 0;
        for kind in KINDS {
            for seed in [5, 36, 901] {
                let n = recipe(&w, seed, kind, p[0], p[1], 0.47, [0.5, 0.45, 0.35]);
                assert!(n.radius <= MAX_RADIUS, "{:?} {}", kind, n.radius);
                assert!(n.solids.len() <= 28);
                let mut near = MeshData::default();
                let mut far = MeshData::default();
                n.append_mesh(&w, &mut near, false, 0);
                n.append_mesh(&w, &mut far, true, 0);
                assert!(!near.indices.is_empty() && !far.indices.is_empty());
                assert!(near.indices.len() / 3 <= 1300);
                assert!(far.indices.len() / 3 <= 400);
                max_near = max_near.max(near.indices.len() / 3);
                max_far = max_far.max(far.indices.len() / 3);
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
        println!("natural landmark triangle limits: near {max_near}, far {max_far}");
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
