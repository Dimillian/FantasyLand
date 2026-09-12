//! Coarse, streamed terrain continues the actual world beyond the detailed chunks.
use crate::{
    ecology,
    geometry::{MeshData, Vertex},
    world::{World, WORLD_SIZE},
};
use glam::Vec3;

pub const PATCH_SIZE: f32 = 1536.0;

pub fn patch(world: &World, cx: i32, cz: i32) -> MeshData {
    const N: usize = 16;
    let mut grid = Vec::with_capacity((N + 1) * (N + 1));
    for z in 0..=N {
        for x in 0..=N {
            let wx = cx as f32 * PATCH_SIZE + x as f32 * PATCH_SIZE / N as f32;
            let wz = cz as f32 * PATCH_SIZE + z as f32 * PATCH_SIZE / N as f32;
            let sample = world.natural_sample(wx, wz);
            let mut color = ecology::ground_color(world.seed, wx, wz, &sample);
            if sample.height < sample.water_height {
                color = [0.12, 0.36, 0.44];
            }
            grid.push((
                Vec3::new(wx, sample.height.max(sample.water_height), wz),
                color,
                sample.ocean,
                (sample.water_height - sample.height).max(0.0),
            ));
        }
    }
    let mut mesh = MeshData::default();
    let at = |x: usize, z: usize| grid[z * (N + 1) + x];
    let half = WORLD_SIZE * 0.5;
    for z in 0..N {
        for x in 0..N {
            let a = at(x, z);
            let b = at(x, z + 1);
            let c = at(x + 1, z + 1);
            let d = at(x + 1, z);
            if a.0.x < -half || a.0.z < -half || c.0.x > half || c.0.z > half {
                continue;
            }
            for points in [[a, b, d], [b, c, d]] {
                let normal = (points[1].0 - points[0].0)
                    .cross(points[2].0 - points[0].0)
                    .normalize_or_zero()
                    .to_array();
                let ocean = points.iter().all(|p| p.2);
                let freshwater = points.iter().all(|p| !p.2 && p.3 > 0.0);
                let color = std::array::from_fn(|i| {
                    (points[0].1[i] + points[1].1[i] + points[2].1[i]) / 3.
                });
                let start = mesh.vertices.len() as u32;
                for (position, _, _, depth) in points {
                    mesh.vertices.push(Vertex {
                        position: position.to_array(),
                        normal,
                        color: if ocean {
                            [0.0, 0.0, depth + 1.0]
                        } else if freshwater {
                            [0.0, 0.0, -(depth + 1.0)]
                        } else {
                            color
                        },
                        material: if ocean || freshwater { 8.0 } else { 7.0 },
                    });
                }
                mesh.indices.extend([start, start + 1, start + 2]);
            }
        }
    }
    mesh
}

// Distant stands are a separate stream: their material is faded out inside the
// ordinary tree ring and beyond the configured vegetation horizon. Sparse 3D
// crowns retain parallax and hill silhouettes without baking detailed forests.
const STAND_STEP: f32 = 96.0;
const STAND_CELLS: usize = 16;
const MAX_CANOPY_TREES: usize = 384;

#[derive(Clone, Copy)]
struct CanopyTree {
    x: f32,
    z: f32,
    seed: u32,
    kind: u32, // broadleaf, birch, willow, pine, fir
    height: f32,
    radius: f32,
    green: [f32; 3],
    bark: [f32; 3],
    lean: f32,
}

/// Coherent, low-detail forest crowns for the distant vegetation ring.
/// At most 384 sixteen-triangle trees (0.81 MB of vertex/index data) per patch.
/// Camera-independent generation makes streaming order and travel deterministic.
pub fn canopy(world: &World, cx: i32, cz: i32) -> MeshData {
    let trees = canopy_trees(world, cx, cz);
    if trees.is_empty() {
        return MeshData::default();
    }
    let surface = CanopySurface::new(world, cx, cz);
    let mut mesh = MeshData {
        vertices: Vec::with_capacity(trees.len() * 48),
        indices: Vec::with_capacity(trees.len() * 48),
    };
    for tree in trees {
        // A coarse mountain cliff can span more relief than a whole tree; leave
        // its exposed face open, even when a small natural shelf could grow one.
        let (_, slope) = surface.at(tree.x, tree.z);
        if slope > 0.90 {
            continue;
        }
        canopy_tree(&mut mesh, &surface, tree);
    }
    mesh
}

fn canopy_random(seed: u32, salt: u32) -> f32 {
    crate::world::rand01(crate::world::hash(
        seed ^ salt.wrapping_mul(0x9e3779b9),
        3,
        7,
    ))
}
fn canopy_mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
fn canopy_trees(world: &World, cx: i32, cz: i32) -> Vec<CanopyTree> {
    use crate::world::{hash, Biome};
    let mut trees = Vec::new();
    let half = WORLD_SIZE * 0.5;
    for z in 0..STAND_CELLS {
        for x in 0..STAND_CELLS {
            let gx = cx * STAND_CELLS as i32 + x as i32;
            let gz = cz * STAND_CELLS as i32 + z as i32;
            let stand_seed = hash(world.seed ^ 0x2a6d90f3, gx, gz);
            let yaw = canopy_random(stand_seed, 1) * std::f32::consts::TAU;
            // Paired, unevenly spaced crowns read as small stands. The occupied
            // fraction follows exactly the regional forest/clearing field.
            for member in 0..2 {
                let seed = hash(stand_seed, member, 419);
                let side = if member == 0 { -1.0 } else { 1.0 };
                let offset = 13.0 + canopy_random(seed, 2) * 10.0;
                let wx = (gx as f32 + 0.5) * STAND_STEP
                    + yaw.cos() * offset * side
                    + (canopy_random(seed, 3) - 0.5) * 18.0;
                let wz = (gz as f32 + 0.5) * STAND_STEP
                    + yaw.sin() * offset * side
                    + (canopy_random(seed, 4) - 0.5) * 18.0;
                if wx.abs() > half - 32.0 || wz.abs() > half - 32.0 {
                    continue;
                }
                // Far roads need no route generation: at this scale individual
                // verges are subpixel. Coast, water, climate and clearings retain
                // the same exclusions and density as nearby actual trees.
                let sample = world.natural_sample(wx, wz);
                let density = ecology::tree_density(world.seed, wx, wz, &sample);
                let occupancy = ((density - 0.035) / 0.76).clamp(0.0, 1.0);
                if canopy_random(seed, 5) >= occupancy {
                    continue;
                }
                let region = crate::regions::sample(world.seed, wx, wz, &sample);
                let species = canopy_random(seed, 6);
                let mut kind = match sample.biome {
                    Biome::PineForest => {
                        if species < 0.46 {
                            4
                        } else if species < 0.85 {
                            3
                        } else {
                            1
                        }
                    }
                    Biome::Forest => {
                        if species < 0.47 {
                            0
                        } else if species < 0.71 {
                            1
                        } else if species < 0.87 {
                            4
                        } else {
                            3
                        }
                    }
                    Biome::Grassland => {
                        if species < 0.62 {
                            0
                        } else if species < 0.9 {
                            1
                        } else {
                            3
                        }
                    }
                    Biome::Wetland => {
                        if species < 0.61 {
                            2
                        } else if species < 0.87 {
                            1
                        } else {
                            0
                        }
                    }
                    Biome::Moor => {
                        if species < 0.64 {
                            1
                        } else {
                            3
                        }
                    }
                    _ => continue,
                };
                if region.pale > 0.5 && species < 0.70 {
                    kind = 1;
                }
                if region.ancient > 0.62
                    && matches!(sample.biome, Biome::Forest | Biome::Grassland)
                    && species < 0.57
                {
                    kind = 0;
                }
                let scale = region.tree_scale
                    * (1.0 + density * 0.32)
                    * (0.84 + canopy_random(seed, 7) * 0.48);
                let h = (match kind {
                    0 => 12.0,
                    1 => 17.0,
                    2 => 13.0,
                    3 => 20.0,
                    _ => 27.0,
                } * scale)
                    .clamp(7.0, 51.0);
                let r = h * match kind {
                    0 => 0.57,
                    1 => 0.25,
                    2 => 0.66,
                    3 => 0.30,
                    _ => 0.25,
                };
                let (lo, hi) = match kind {
                    1 => ([0.35, 0.46, 0.20], [0.50, 0.56, 0.28]),
                    2 => ([0.29, 0.40, 0.22], [0.43, 0.49, 0.30]),
                    3 => ([0.18, 0.29, 0.18], [0.31, 0.40, 0.23]),
                    4 => ([0.16, 0.29, 0.25], [0.25, 0.38, 0.29]),
                    _ => ([0.25, 0.39, 0.16], [0.42, 0.49, 0.21]),
                };
                let green = canopy_mix(
                    canopy_mix(lo, hi, canopy_random(seed, 8)),
                    [0.58, 0.64, 0.47],
                    region.pale * 0.75,
                );
                let bark = if kind == 1 {
                    [0.70, 0.71, 0.61]
                } else {
                    canopy_mix([0.29, 0.235, 0.16], [0.53, 0.55, 0.46], region.pale * 0.8)
                };
                trees.push(CanopyTree {
                    x: wx,
                    z: wz,
                    seed,
                    kind,
                    height: h,
                    radius: r,
                    green,
                    bark,
                    lean: 0.025 + region.exposure * 0.12,
                });
            }
        }
    }
    // A spatially uncorrelated cap preserves groves across the entire patch,
    // rather than deleting the final rows of the densest forests.
    if trees.len() > MAX_CANOPY_TREES {
        trees.sort_unstable_by_key(|tree| tree.seed);
        trees.truncate(MAX_CANOPY_TREES);
    }
    trees
}

struct CanopySurface {
    origin: [f32; 2],
    heights: [[f32; STAND_CELLS + 1]; STAND_CELLS + 1],
}
impl CanopySurface {
    fn new(world: &World, cx: i32, cz: i32) -> Self {
        let origin = [cx as f32 * PATCH_SIZE, cz as f32 * PATCH_SIZE];
        let heights = std::array::from_fn(|z| {
            std::array::from_fn(|x| {
                let s = world.natural_sample(
                    origin[0] + x as f32 * STAND_STEP,
                    origin[1] + z as f32 * STAND_STEP,
                );
                s.height.max(s.water_height)
            })
        });
        Self { origin, heights }
    }
    fn at(&self, wx: f32, wz: f32) -> (f32, f32) {
        let x = ((wx - self.origin[0]) / STAND_STEP).clamp(0.0, STAND_CELLS as f32);
        let z = ((wz - self.origin[1]) / STAND_STEP).clamp(0.0, STAND_CELLS as f32);
        let ix = (x.floor() as usize).min(STAND_CELLS - 1);
        let iz = (z.floor() as usize).min(STAND_CELLS - 1);
        let u = x - ix as f32;
        let v = z - iz as f32;
        let a = self.heights[iz][ix];
        let b = self.heights[iz + 1][ix];
        let c = self.heights[iz + 1][ix + 1];
        let d = self.heights[iz][ix + 1];
        // Same diagonal and barycentric surface as horizon::patch, including
        // submerged terrain vertices that are drawn at the water surface.
        let (height, dx, dz) = if u + v <= 1.0 {
            (a + (d - a) * u + (b - a) * v, d - a, b - a)
        } else {
            (c + (b - c) * (1.0 - u) + (d - c) * (1.0 - v), c - b, c - d)
        };
        (height, (dx * dx + dz * dz).sqrt() / STAND_STEP)
    }
}

fn canopy_triangle(mesh: &mut MeshData, p: [Vec3; 3], color: [f32; 3]) {
    let normal = (p[1] - p[0])
        .cross(p[2] - p[0])
        .normalize_or_zero()
        .to_array();
    let start = mesh.vertices.len() as u32;
    for position in p {
        mesh.vertices.push(Vertex {
            position: position.to_array(),
            normal,
            color,
            material: 9.0,
        });
    }
    mesh.indices.extend([start, start + 1, start + 2]);
}
fn canopy_shell_triangle(mesh: &mut MeshData, mut p: [Vec3; 3], center: Vec3, color: [f32; 3]) {
    if (p[1] - p[0])
        .cross(p[2] - p[0])
        .dot((p[0] + p[1] + p[2]) / 3.0 - center)
        < 0.0
    {
        p.swap(1, 2);
    }
    canopy_triangle(mesh, p, color);
}
fn canopy_tree(mesh: &mut MeshData, surface: &CanopySurface, t: CanopyTree) {
    let ground = surface.at(t.x, t.z).0;
    let origin = Vec3::new(t.x, ground, t.z);
    let yaw = canopy_random(t.seed, 9) * std::f32::consts::TAU;
    let lean = Vec3::new(yaw.cos(), 0.0, yaw.sin()) * t.height * t.lean;
    let stem_top = origin + Vec3::Y * (t.height * 0.70) + lean * 0.7;
    // Crossed solid trunk faces, each lower corner embedded in its own actual
    // rendered triangle. A level base would float on the downhill side.
    for axis in [Vec3::X, Vec3::Z] {
        let spread = axis * (0.30 + t.height * 0.009);
        let foot = |p: Vec3| Vec3::new(p.x, surface.at(p.x, p.z).0 - 0.35, p.z);
        let a = foot(origin - spread);
        let b = foot(origin + spread);
        let c = stem_top + spread * 0.20;
        let d = stem_top - spread * 0.20;
        canopy_triangle(mesh, [a, b, c], t.bark);
        canopy_triangle(mesh, [a, c, d], t.bark);
    }
    if t.kind >= 3 {
        // Two uneven, overlapping tapered crowns retain a fir/pine silhouette;
        // they are tree-sized crowns, not terrain-sized forest pyramids.
        for (tier, (bottom, top, radius)) in [(0.25, 0.80, 1.0), (0.57, 1.0, 0.66)]
            .into_iter()
            .enumerate()
        {
            let center = origin + Vec3::Y * (t.height * bottom) + lean * bottom;
            let tip = origin + Vec3::Y * (t.height * top) + lean * top;
            let ring: [Vec3; 4] = std::array::from_fn(|i| {
                let angle = yaw + i as f32 * std::f32::consts::FRAC_PI_2;
                let irregular = 0.85 + canopy_random(t.seed, 30 + (tier * 4 + i) as u32) * 0.30;
                center
                    + Vec3::new(
                        angle.cos() * t.radius * radius * irregular,
                        (canopy_random(t.seed, 45 + i as u32) - 0.5) * t.height * 0.025,
                        angle.sin() * t.radius * radius * 0.86 * irregular,
                    )
            });
            let inside = center + Vec3::Y * (t.height * (top - bottom) * 0.3);
            for i in 0..4 {
                let color = t.green.map(|v| v * (0.93 + 0.055 * i as f32));
                canopy_shell_triangle(mesh, [ring[i], ring[(i + 1) % 4], tip], inside, color);
            }
            for ids in [[0, 2, 1], [0, 3, 2]] {
                canopy_shell_triangle(
                    mesh,
                    ids.map(|i| ring[i]),
                    inside,
                    t.green.map(|v| v * 0.80),
                );
            }
        }
    } else {
        // A six-sided irregular crown gives broadleaf, slender birch and
        // drooping willow distinct outlines without flat billboard walls.
        let center = origin + Vec3::Y * (t.height * 0.66) + lean * 0.66;
        let top = origin + Vec3::Y * t.height + lean;
        let bottom =
            origin + Vec3::Y * (t.height * if t.kind == 2 { 0.27 } else { 0.40 }) + lean * 0.4;
        let ring: [Vec3; 6] = std::array::from_fn(|i| {
            let angle = yaw + i as f32 * std::f32::consts::TAU / 6.0;
            let irregular = 0.81 + canopy_random(t.seed, 60 + i as u32) * 0.32;
            center
                + Vec3::new(
                    angle.cos() * t.radius * irregular,
                    (canopy_random(t.seed, 70 + i as u32) - 0.5) * t.height * 0.14,
                    angle.sin() * t.radius * 0.79 * irregular,
                )
        });
        for i in 0..6 {
            let color = t
                .green
                .map(|v| v * (0.95 + canopy_random(t.seed, 80 + i as u32) * 0.10));
            canopy_shell_triangle(mesh, [ring[i], ring[(i + 1) % 6], top], center, color);
            canopy_shell_triangle(
                mesh,
                [ring[i], bottom, ring[(i + 1) % 6]],
                center,
                color.map(|v| v * 0.86),
            );
        }
    }
}

#[cfg(test)]
mod canopy_tests {
    use super::*;

    #[test]
    fn coarse_root_interpolation_matches_emitted_horizon_triangles() {
        let world = World::new(1337);
        for (cx, cz) in [(-8, 38), (-49, 45), (32, 17)] {
            let ground = patch(&world, cx, cz);
            let surface = CanopySurface::new(&world, cx, cz);
            for triangle in ground.vertices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(triangle[i].position));
                for weights in [[0.2, 0.3, 0.5], [0.6, 0.25, 0.15], [0.05, 0.85, 0.10]] {
                    let p = a * weights[0] + b * weights[1] + c * weights[2];
                    assert!(
                        (surface.at(p.x, p.z).0 - p.y).abs() < 0.035,
                        "rendered root mismatch at {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn canopy_eligibility_determinism_and_geometry_budget() {
        let world = World::new(1337);
        let mut tree_count = 0;
        for (cx, cz) in [
            (-8, 38),
            (-49, 45),
            (30, 81),
            (-8, 32),
            (32, 17),
            (-38, 13),
            (-40, 106),
        ] {
            let trees = canopy_trees(&world, cx, cz);
            assert!(trees.len() <= MAX_CANOPY_TREES);
            for tree in &trees {
                let s = world.natural_sample(tree.x, tree.z);
                assert!(ecology::tree_density(world.seed, tree.x, tree.z, &s) > 0.035);
                assert!(
                    !s.ocean
                        && s.shore == crate::world::ShoreKind::None
                        && s.height > s.water_height
                );
                assert!(tree.x > cx as f32 * PATCH_SIZE && tree.x < (cx + 1) as f32 * PATCH_SIZE);
                assert!(tree.z > cz as f32 * PATCH_SIZE && tree.z < (cz + 1) as f32 * PATCH_SIZE);
            }
            let a = canopy(&world, cx, cz);
            let b = canopy(&world, cx, cz);
            assert_eq!(
                bytemuck::cast_slice::<Vertex, u8>(&a.vertices),
                bytemuck::cast_slice::<Vertex, u8>(&b.vertices)
            );
            assert_eq!(a.indices, b.indices);
            assert!(a.indices.len() / 3 <= MAX_CANOPY_TREES * 16);
            assert_eq!(a.indices.len(), a.vertices.len());
            for vertex in &a.vertices {
                assert_eq!(vertex.material, 9.0);
                assert!(vertex
                    .position
                    .iter()
                    .chain(vertex.normal.iter())
                    .chain(vertex.color.iter())
                    .all(|v| v.is_finite()));
                assert!((Vec3::from_array(vertex.normal).length() - 1.0).abs() < 0.001);
            }
            assert!(a.indices.iter().all(|i| (*i as usize) < a.vertices.len()));
            tree_count += a.indices.len() / 48;
        }
        assert!(
            tree_count > 100,
            "representative forest regions should produce visible crowns"
        );
    }

    #[test]
    fn every_trunk_corner_embeds_in_its_own_coarse_triangle() {
        let world = World::new(1337);
        for (cx, cz) in [(-8, 38), (-49, 45), (-8, 32)] {
            let surface = CanopySurface::new(&world, cx, cz);
            for tree in canopy_trees(&world, cx, cz) {
                let mut mesh = MeshData::default();
                canopy_tree(&mut mesh, &surface, tree);
                assert_eq!(mesh.indices.len(), 48);
                // Two triangles per trunk plane: these are the four distinct
                // ground corners, not crown vertices or artificially low bounds.
                for index in [0, 1, 6, 7] {
                    let p = mesh.vertices[index].position;
                    let expected = surface.at(p[0], p[2]).0 - 0.35;
                    assert!((p[1] - expected).abs() < 0.003);
                }
            }
        }
    }

    #[test]
    fn open_clearings_and_ocean_do_not_become_a_forest_wall() {
        let world = World::new(1337);
        // Select from ecology inputs, never from the renderer's resulting tree
        // count. Landform changes may move forest/clearing boundaries entirely.
        let mut forests = Vec::new();
        let mut exposed = Vec::new();
        let mut oceans = Vec::new();
        'scan: for cz in (-108..=108).step_by(3) {
            for cx in (-108..=108).step_by(3) {
                let mut density = 0.;
                let mut granite = 0;
                let mut water = 0;
                let mut rockiness = 0.;
                for iz in 0..4 {
                    for ix in 0..4 {
                        let x = (cx as f32 + (ix as f32 + 0.5) / 4.) * PATCH_SIZE;
                        let z = (cz as f32 + (iz as f32 + 0.5) / 4.) * PATCH_SIZE;
                        let s = world.natural_sample(x, z);
                        let r = crate::regions::sample(world.seed, x, z, &s);
                        density += ecology::tree_density(world.seed, x, z, &s);
                        granite += usize::from(r.geology == crate::regions::Geology::Granite);
                        water += usize::from(s.ocean);
                        rockiness += r.rockiness;
                    }
                }
                density /= 16.;
                rockiness /= 16.;
                if forests.len() < 3 && water == 0 && density > 0.46 {
                    forests.push((cx, cz));
                }
                if exposed.len() < 3
                    && water == 0
                    && granite >= 12
                    && rockiness > 0.60
                    && density < 0.050
                {
                    exposed.push((cx, cz));
                }
                if oceans.len() < 3 && water == 16 {
                    oceans.push((cx, cz));
                }
                if forests.len() == 3 && exposed.len() == 3 && oceans.len() == 3 {
                    break 'scan;
                }
            }
        }
        assert_eq!(
            (forests.len(), exposed.len(), oceans.len()),
            (3, 3, 3),
            "need representative ecological fixtures"
        );
        let forest: usize = forests
            .iter()
            .map(|&(cx, cz)| canopy(&world, cx, cz).indices.len())
            .sum();
        let exposed: usize = exposed
            .iter()
            .map(|&(cx, cz)| canopy(&world, cx, cz).indices.len())
            .sum();
        assert!(
            forest > 3 * 48 * 40,
            "forested ecology must keep substantial distant cover"
        );
        assert!(
            exposed < forest / 3,
            "exposed granite must remain mostly open"
        );
        for (cx, cz) in oceans {
            assert_eq!(canopy(&world, cx, cz).indices.len(), 0);
        }
        println!(
            "canopy regression: {} forest trees vs {} open-granite trees across three patches each",
            forest / 48,
            exposed / 48
        );
    }
}
