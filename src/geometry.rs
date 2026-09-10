//! Asset-free, deterministic low-poly geometry for the streamed world.
//! World positions are retained here; the renderer performs the camera-relative transform.
use crate::ecology;
use crate::world::{hash, rand01, Biome, World};
use bytemuck::{Pod, Zeroable};
use std::f32::consts::{PI, TAU};

pub const CHUNK_SIZE: f32 = 192.0;
const PROP_GRID: f32 = 12.0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 3],
    pub material: f32,
}

#[derive(Default, Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    fn triangle(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], color: [f32; 3], material: f32) {
        let u = sub(b, a);
        let v = sub(c, a);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len < 0.00001 {
            return;
        }
        let normal = [n[0] / len, n[1] / len, n[2] / len];
        let i = self.vertices.len() as u32;
        self.vertices.extend([a, b, c].map(|position| Vertex {
            position,
            normal,
            color,
            material,
        }));
        self.indices.extend([i, i + 1, i + 2]);
    }
    fn quad(
        &mut self,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        d: [f32; 3],
        color: [f32; 3],
        material: f32,
    ) {
        self.triangle(a, b, c, color, material);
        self.triangle(a, c, d, color, material);
    }
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(c: [f32; 3], amount: f32) -> [f32; 3] {
    [c[0] * amount, c[1] * amount, c[2] * amount]
}
fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}
fn random(seed: u32, salt: u32) -> f32 {
    rand01(hash(
        seed ^ salt.wrapping_mul(747796405),
        salt as i32,
        (seed >> 16) as i32,
    ))
}

#[derive(Clone, Copy)]
struct GroundVertex {
    position: [f32; 3],
    color: [f32; 3],
    water: f32,
}
fn ground_vertex(world: &World, x: f32, z: f32) -> GroundVertex {
    let s = world.sample(x, z);
    let color = ecology::ground_color(world.seed, x, z, &s);
    let wet = mix(color, [0.34, 0.34, 0.25], (s.river * 0.7).clamp(0.0, 0.7));
    let color = mix(wet, [0.55, 0.43, 0.28], (s.road * 0.93).clamp(0.0, 0.93));
    GroundVertex {
        position: [x, s.height, z],
        color,
        water: s.water_height,
    }
}

pub fn terrain_chunk(world: &World, cx: i32, cz: i32, lod: u32) -> MeshData {
    let divisions = (32_u32 >> lod.min(4)).max(2) as usize;
    let step = CHUNK_SIZE / divisions as f32;
    let ox = cx as f32 * CHUNK_SIZE;
    let oz = cz as f32 * CHUNK_SIZE;
    let mut grid = Vec::with_capacity((divisions + 1) * (divisions + 1));
    for z in 0..=divisions {
        for x in 0..=divisions {
            grid.push(ground_vertex(
                world,
                ox + x as f32 * step,
                oz + z as f32 * step,
            ));
        }
    }
    let mut mesh = MeshData {
        vertices: Vec::with_capacity(divisions * divisions * 6 + divisions * 24),
        indices: Vec::with_capacity(divisions * divisions * 6 + divisions * 24),
    };
    let at = |x: usize, z: usize| grid[z * (divisions + 1) + x];
    for z in 0..divisions {
        for x in 0..divisions {
            let a = at(x, z);
            let b = at(x, z + 1);
            let c = at(x + 1, z + 1);
            let d = at(x + 1, z);
            let h = hash(
                world.seed,
                (ox / step) as i32 + x as i32,
                (oz / step) as i32 + z as i32,
            );
            let tone = 0.96 + rand01(h) * 0.08;
            // Alternating diagonals avoid a strong regular diagonal pattern on slopes.
            if h & 1 == 0 {
                terrain_triangle(&mut mesh, a, b, d, tone);
                terrain_triangle(&mut mesh, b, c, d, tone * 0.985);
            } else {
                terrain_triangle(&mut mesh, a, b, c, tone);
                terrain_triangle(&mut mesh, a, c, d, tone * 0.985);
            }
        }
    }
    // Overlapping vertical edges close cracks between neighboring terrain LODs.
    let skirt_depth = (step * 1.8).max(20.0);
    for i in 0..divisions {
        for (a, b) in [
            (at(i, 0), at(i + 1, 0)),
            (at(i + 1, divisions), at(i, divisions)),
            (at(0, i + 1), at(0, i)),
            (at(divisions, i), at(divisions, i + 1)),
        ] {
            let mut low_a = a.position;
            low_a[1] -= skirt_depth;
            let mut low_b = b.position;
            low_b[1] -= skirt_depth;
            mesh.quad(
                a.position,
                b.position,
                low_b,
                low_a,
                mul(a.color, 0.86),
                0.0,
            );
        }
    }
    road_ribbons(world, &mut mesh, ox, oz, divisions, &grid);
    mesh
}
fn terrain_triangle(
    mesh: &mut MeshData,
    a: GroundVertex,
    b: GroundVertex,
    c: GroundVertex,
    tone: f32,
) {
    let color = [
        (a.color[0] + b.color[0] + c.color[0]) / 3.0,
        (a.color[1] + b.color[1] + c.color[1]) / 3.0,
        (a.color[2] + b.color[2] + c.color[2]) / 3.0,
    ];
    mesh.triangle(a.position, b.position, c.position, mul(color, tone), 0.0);
}

pub fn water_chunk(world: &World, cx: i32, cz: i32, lod: u32) -> MeshData {
    let divisions = (32_u32 >> lod.min(4)).max(2) as usize;
    let step = CHUNK_SIZE / divisions as f32;
    let ox = cx as f32 * CHUNK_SIZE;
    let oz = cz as f32 * CHUNK_SIZE;
    let mut mesh = MeshData::default();
    let mut grid = Vec::with_capacity((divisions + 1) * (divisions + 1));
    for z in 0..=divisions {
        for x in 0..=divisions {
            grid.push(ground_vertex(
                world,
                ox + x as f32 * step,
                oz + z as f32 * step,
            ));
        }
    }
    for z in 0..divisions {
        for x in 0..divisions {
            let corners = [
                grid[z * (divisions + 1) + x],
                grid[(z + 1) * (divisions + 1) + x],
                grid[(z + 1) * (divisions + 1) + x + 1],
                grid[z * (divisions + 1) + x + 1],
            ];
            let mut valid = 0;
            let mut water = 0.0;
            for p in corners {
                if p.water > -999.0 {
                    water += p.water;
                    valid += 1;
                }
            }
            if valid == 0 {
                continue;
            }
            water /= valid as f32;
            let h = hash(
                world.seed,
                (ox / step) as i32 + x as i32,
                (oz / step) as i32 + z as i32,
            );
            // Use the terrain's diagonal, then clip exactly to its shoreline instead of
            // exposing water tiles beyond the bank. Shader ripples provide color variation.
            if h & 1 == 0 {
                water_triangle(&mut mesh, [corners[0], corners[1], corners[3]], water);
                water_triangle(&mut mesh, [corners[1], corners[2], corners[3]], water);
            } else {
                water_triangle(&mut mesh, [corners[0], corners[1], corners[2]], water);
                water_triangle(&mut mesh, [corners[0], corners[2], corners[3]], water);
            }
        }
    }
    // Material4's otherwise-unused color carries the local downstream tangent.
    // Keep world-space wave phases continuous; only advection follows the river.
    for triangle in mesh.vertices.chunks_exact_mut(3) {
        let x = triangle.iter().map(|v| v.position[0]).sum::<f32>() / 3.0;
        let z = triangle.iter().map(|v| v.position[2]).sum::<f32>() / 3.0;
        let flow = world.water_flow(x, z);
        for v in triangle {
            v.color = [flow[0], flow[1], 0.0];
        }
    }
    mesh
}
fn water_triangle(mesh: &mut MeshData, triangle: [GroundVertex; 3], fallback: f32) {
    let points = triangle.map(|p| {
        let level = if p.water > -999.0 { p.water } else { fallback };
        (
            [p.position[0], level, p.position[2]],
            level - p.position[1] + 0.012,
        )
    });
    let mut polygon = Vec::with_capacity(4);
    for i in 0..3 {
        let (a, da) = points[i];
        let (b, db) = points[(i + 1) % 3];
        if da >= 0.0 {
            polygon.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            polygon.push([
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]);
        }
    }
    for i in 1..polygon.len().saturating_sub(1) {
        mesh.triangle(
            polygon[0],
            polygon[i],
            polygon[i + 1],
            [0.25, 0.39, 0.46],
            4.0,
        );
    }
}

// Interpolate the exact rendered terrain triangle, including its alternating diagonal.
// Roads therefore sit on the visible surface even in coarse distant chunks.
fn grid_height(
    world: &World,
    ox: f32,
    oz: f32,
    divisions: usize,
    grid: &[GroundVertex],
    x: f32,
    z: f32,
) -> f32 {
    let step = CHUNK_SIZE / divisions as f32;
    let fx = ((x - ox) / step).clamp(0.0, divisions as f32 - 0.00001);
    let fz = ((z - oz) / step).clamp(0.0, divisions as f32 - 0.00001);
    let ix = fx.floor() as usize;
    let iz = fz.floor() as usize;
    let u = fx - ix as f32;
    let v = fz - iz as f32;
    let a = grid[iz * (divisions + 1) + ix].position[1];
    let b = grid[(iz + 1) * (divisions + 1) + ix].position[1];
    let c = grid[(iz + 1) * (divisions + 1) + ix + 1].position[1];
    let d = grid[iz * (divisions + 1) + ix + 1].position[1];
    let h = hash(
        world.seed,
        (ox / step) as i32 + ix as i32,
        (oz / step) as i32 + iz as i32,
    );
    if h & 1 == 0 {
        if u + v <= 1.0 {
            a + (d - a) * u + (b - a) * v
        } else {
            c + (b - c) * (1.0 - u) + (d - c) * (1.0 - v)
        }
    } else if v >= u {
        a + (c - b) * u + (b - a) * v
    } else {
        a + (d - a) * u + (c - d) * v
    }
}
fn road_ribbons(
    world: &World,
    mesh: &mut MeshData,
    ox: f32,
    oz: f32,
    divisions: usize,
    grid: &[GroundVertex],
) {
    for road in world.roads_near(ox + CHUNK_SIZE / 2.0, oz + CHUNK_SIZE / 2.0, CHUNK_SIZE) {
        for segment in road.windows(2) {
            let a = segment[0];
            let b = segment[1];
            let dx = b[0] - a[0];
            let dz = b[1] - a[1];
            let len = (dx * dx + dz * dz).sqrt();
            if len < 0.01 {
                continue;
            }
            let pieces = (len / 5.0).ceil() as usize;
            for i in 0..pieces {
                let t0 = i as f32 / pieces as f32;
                let t1 = (i + 1) as f32 / pieces as f32;
                let mx = a[0] + dx * (t0 + t1) / 2.0;
                let mz = a[1] + dz * (t0 + t1) / 2.0;
                if !owns(ox, oz, mx, mz) {
                    continue;
                }
                let sample = world.sample(mx, mz);
                if sample.water_height > sample.height {
                    continue;
                }
                let path_color = mul(
                    [0.53, 0.42, 0.27],
                    0.96 + rand01(hash(world.seed ^ 0x524f4144, mx as i32, mz as i32)) * 0.08,
                );
                let start = [a[0] + dx * t0, a[1] + dz * t0];
                let end = [a[0] + dx * t1, a[1] + dz * t1];
                for (left, right, color) in [
                    (-4.0, -2.6, mix([0.40, 0.40, 0.24], path_color, 0.6)),
                    (-2.6, 2.6, path_color),
                    (2.6, 4.0, mix([0.40, 0.40, 0.24], path_color, 0.6)),
                ] {
                    let points = [(start, left), (end, left), (end, right), (start, right)].map(
                        |(p, side)| {
                            let x = p[0] + dz / len * side;
                            let z = p[1] - dx / len * side;
                            let y = if x >= ox
                                && x <= ox + CHUNK_SIZE
                                && z >= oz
                                && z <= oz + CHUNK_SIZE
                            {
                                grid_height(world, ox, oz, divisions, grid, x, z)
                            } else {
                                world.height(x, z)
                            };
                            [x, y + 0.075, z]
                        },
                    );
                    mesh.quad(points[0], points[1], points[2], points[3], color, 0.0);
                }
            }
        }
    }
}

// Physics uses the same six-meter triangles as the finest streamed terrain.
// Sampling the analytic generator directly would let feet sink into a curved ridge.
fn terrain_surface_height(world: &World, x: f32, z: f32) -> f32 {
    let step = 6.0;
    let ix = (x / step).floor() as i32;
    let iz = (z / step).floor() as i32;
    let ox = ix as f32 * step;
    let oz = iz as f32 * step;
    let u = (x - ox) / step;
    let v = (z - oz) / step;
    let a = world.height(ox, oz);
    let b = world.height(ox, oz + step);
    let c = world.height(ox + step, oz + step);
    let d = world.height(ox + step, oz);
    if hash(world.seed, ix, iz) & 1 == 0 {
        if u + v <= 1.0 {
            a + (d - a) * u + (b - a) * v
        } else {
            c + (b - c) * (1.0 - u) + (d - c) * (1.0 - v)
        }
    } else if v >= u {
        a + (c - b) * u + (b - a) * v
    } else {
        a + (d - a) * u + (c - d) * v
    }
}

/// Ground/deck surface used for walking. This uses the same five-meter bridge spans
/// as the visible mesh, including bank approaches and the seven-meter deck width.
pub fn walk_height(world: &World, x: f32, z: f32) -> f32 {
    let sample = world.sample(x, z);
    let mut height = terrain_surface_height(world, x, z);
    if sample.road < 0.8 {
        return height;
    }
    for road in world.roads_near(x, z, 6.0) {
        for segment in road.windows(2) {
            let a = segment[0];
            let b = segment[1];
            let dx = b[0] - a[0];
            let dz = b[1] - a[1];
            let len = (dx * dx + dz * dz).sqrt();
            if len < 0.01 {
                continue;
            }
            let along = ((x - a[0]) * dx + (z - a[1]) * dz) / len;
            let across = ((x - a[0]) * dz - (z - a[1]) * dx) / len;
            if across.abs() > 3.5 || along < -0.05 || along > len + 0.05 {
                continue;
            }
            let pieces = (len / 5.0).ceil() as i32;
            let piece_len = len / pieces as f32;
            let nearest = (along / piece_len).floor() as i32;
            for i in (nearest - 1).max(0)..=(nearest + 1).min(pieces - 1) {
                let middle = (i as f32 + 0.5) * piece_len;
                if (middle - along).abs() > piece_len / 2.0 + 0.05 {
                    continue;
                }
                let start = [
                    a[0] + dx / len * (middle - piece_len / 2.0),
                    a[1] + dz / len * (middle - piece_len / 2.0),
                ];
                let end = [
                    a[0] + dx / len * (middle + piece_len / 2.0),
                    a[1] + dz / len * (middle + piece_len / 2.0),
                ];
                if let Some((y0, y1)) = bridge_heights(world, start, end) {
                    let t = ((along - (middle - piece_len / 2.0)) / piece_len).clamp(0.0, 1.0);
                    height = height.max(y0 + (y1 - y0) * t);
                }
            }
        }
    }
    height
}

#[derive(Clone, Copy, Debug)]
enum PropKind {
    Pine,
    Fir,
    Broadleaf,
    Birch,
    Willow,
    DeadTree,
    Boulder,
    Shrub,
    Reed,
    FallenLog,
    Stump,
}
#[derive(Clone, Copy)]
struct Prop {
    position: [f32; 3],
    scale: f32,
    canopy: f32,
    seed: u32,
    kind: PropKind,
    biome: Biome,
}

fn prop_at(world: &World, gx: i32, gz: i32) -> Option<Prop> {
    let seed = hash(world.seed ^ 0x54455252, gx, gz);
    let x = gx as f32 * PROP_GRID + 2.0 + random(seed, 1) * (PROP_GRID - 4.0);
    let z = gz as f32 * PROP_GRID + 2.0 + random(seed, 2) * (PROP_GRID - 4.0);
    let pick = random(seed, 3);
    let sample = world.sample(x, z);
    if sample.road > 0.07 || sample.water_height > sample.height + 0.8 {
        return None;
    }
    let density = ecology::tree_density(world.seed, x, z, &sample);
    let species = random(seed, 6);
    let detail = random(seed, 8);
    let kind = if sample.river > 0.28 || sample.water_height > sample.height - 0.8 {
        if pick < 0.64 {
            PropKind::Reed
        } else if pick < 0.77 {
            PropKind::Boulder
        } else {
            return None;
        }
    } else if pick < density {
        match sample.biome {
            Biome::Forest => {
                if species < 0.43 {
                    PropKind::Broadleaf
                } else if species < 0.67 {
                    PropKind::Birch
                } else if species < 0.83 {
                    PropKind::Fir
                } else if species < 0.96 {
                    PropKind::Pine
                } else {
                    PropKind::DeadTree
                }
            }
            Biome::PineForest => {
                if species < 0.46 {
                    PropKind::Fir
                } else if species < 0.82 {
                    PropKind::Pine
                } else if species < 0.96 {
                    PropKind::Birch
                } else {
                    PropKind::DeadTree
                }
            }
            Biome::Grassland => {
                if species < 0.60 {
                    PropKind::Broadleaf
                } else if species < 0.88 {
                    PropKind::Birch
                } else if species < 0.97 {
                    PropKind::Pine
                } else {
                    PropKind::DeadTree
                }
            }
            Biome::Wetland => {
                if species < 0.61 {
                    PropKind::Willow
                } else if species < 0.87 {
                    PropKind::Birch
                } else {
                    PropKind::Broadleaf
                }
            }
            Biome::Moor => {
                if species < 0.52 {
                    PropKind::DeadTree
                } else if species < 0.80 {
                    PropKind::Birch
                } else {
                    PropKind::Pine
                }
            }
            _ => return None,
        }
    } else if detail < density * 0.075 {
        if species < 0.64 {
            PropKind::FallenLog
        } else {
            PropKind::Stump
        }
    } else if matches!(sample.biome, Biome::Alpine | Biome::Desert) && detail < 0.18 {
        PropKind::Boulder
    } else if detail > 0.964 {
        PropKind::Boulder
    } else if sample.biome == Biome::Wetland && detail < 0.28 {
        PropKind::Reed
    } else if detail < 0.045 + density * (1.0 - density) * 0.25
        && !matches!(sample.biome, Biome::Alpine | Biome::Desert)
    {
        PropKind::Shrub
    } else {
        return None;
    };
    // The same larger crowns and coordinates are used by near meshes, distant
    // silhouettes and trunk collision. Dense stands can close their canopy.
    let grove_scale = if matches!(
        kind,
        PropKind::Pine
            | PropKind::Fir
            | PropKind::Broadleaf
            | PropKind::Birch
            | PropKind::Willow
            | PropKind::DeadTree
    ) {
        1.0 + density * 0.32
    } else {
        1.0
    };
    Some(Prop {
        position: [x, terrain_surface_height(world, x, z) - 0.08, z],
        scale: (0.65 + random(seed, 4) * 0.87) * grove_scale,
        canopy: if matches!(kind, PropKind::Pine | PropKind::Fir) {
            1.0 + density * 0.30
        } else {
            1.0
        },
        seed,
        kind,
        biome: sample.biome,
    })
}

pub fn props_chunk(world: &World, cx: i32, cz: i32) -> MeshData {
    props_chunk_with_cover(world, cx, cz, true)
}

/// The streaming renderer requests ground cover only in its closest prop ring.
pub fn props_chunk_with_cover(world: &World, cx: i32, cz: i32, cover: bool) -> MeshData {
    let mut mesh = MeshData::default();
    let ox = cx as f32 * CHUNK_SIZE;
    let oz = cz as f32 * CHUNK_SIZE;
    let midx = ox + CHUNK_SIZE / 2.0;
    let midz = oz + CHUNK_SIZE / 2.0;
    let sites = world.sites_near(midx, midz, CHUNK_SIZE + 90.0);
    let landmarks = world.landmarks_near(midx, midz, CHUNK_SIZE + 40.0);
    let start_x = (ox / PROP_GRID).round() as i32;
    let start_z = (oz / PROP_GRID).round() as i32;
    let count = (CHUNK_SIZE / PROP_GRID) as i32;
    for z in start_z..start_z + count {
        for x in start_x..start_x + count {
            if let Some(p) = prop_at(world, x, z) {
                if sites.iter().any(|s| {
                    (s.x - p.position[0]).powi(2) + (s.z - p.position[2]).powi(2) < 45.0_f32.powi(2)
                }) {
                    continue;
                }
                if landmarks.iter().any(|s| {
                    (s.x - p.position[0]).powi(2) + (s.z - p.position[2]).powi(2) < 15.0_f32.powi(2)
                }) {
                    continue;
                }
                match p.kind {
                    PropKind::Pine => pine(&mut mesh, p),
                    PropKind::Fir => fir(&mut mesh, p),
                    PropKind::Broadleaf => broadleaf(&mut mesh, p),
                    PropKind::Birch => birch(&mut mesh, p),
                    PropKind::Willow => willow(&mut mesh, p),
                    PropKind::DeadTree => dead_tree(&mut mesh, p),
                    PropKind::FallenLog => fallen_log(&mut mesh, p, world),
                    PropKind::Stump => stump(&mut mesh, p),
                    PropKind::Boulder => rock(&mut mesh, p.position, p.scale, p.seed),
                    PropKind::Shrub => {
                        let color = mul([0.27, 0.34, 0.14], 0.86 + random(p.seed, 12) * 0.24);
                        polyhedron(
                            &mut mesh,
                            [p.position[0], p.position[1] + 0.6 * p.scale, p.position[2]],
                            [1.25 * p.scale, 0.95 * p.scale, 1.10 * p.scale],
                            p.seed,
                            color,
                            1.0,
                        );
                    }
                    PropKind::Reed => reeds(&mut mesh, p),
                }
            }
        }
    }
    for site in sites {
        if owns(ox, oz, site.x, site.z) {
            let y = world.height(site.x, site.z);
            let seed = hash(world.seed, site.x as i32, site.z as i32);
            settlement_marker(&mut mesh, [site.x, y, site.z], seed, &site.kind);
        }
    }
    for landmark in landmarks {
        if owns(ox, oz, landmark.x, landmark.z) {
            let y = world.height(landmark.x, landmark.z);
            let seed = hash(world.seed ^ 7123, landmark.x as i32, landmark.z as i32);
            landmark_mesh(&mut mesh, [landmark.x, y, landmark.z], seed, &landmark.kind);
        }
    }
    if cover {
        ground_cover(world, &mut mesh, ox, oz);
    }
    bridges(world, &mut mesh, ox, oz);
    mesh
}
/// Cheap tree-only silhouettes for the outer streaming ring. Candidate positions,
/// species and clearing exclusions match nearby geometry; every second tree is
/// retained to keep distant forests within the rendering budget.
pub fn distant_props_chunk(world: &World, cx: i32, cz: i32) -> MeshData {
    let mut mesh = MeshData::default();
    let ox = cx as f32 * CHUNK_SIZE;
    let oz = cz as f32 * CHUNK_SIZE;
    let sites = world.sites_near(
        ox + CHUNK_SIZE / 2.0,
        oz + CHUNK_SIZE / 2.0,
        CHUNK_SIZE + 90.0,
    );
    let landmarks = world.landmarks_near(
        ox + CHUNK_SIZE / 2.0,
        oz + CHUNK_SIZE / 2.0,
        CHUNK_SIZE + 40.0,
    );
    let gx = (ox / PROP_GRID).round() as i32;
    let gz = (oz / PROP_GRID).round() as i32;
    let count = (CHUNK_SIZE / PROP_GRID) as i32;
    for z in gz..gz + count {
        for x in gx..gx + count {
            if hash(world.seed ^ 0x54455252, x, z) & 1 != 0 {
                continue;
            }
            let Some(p) = prop_at(world, x, z) else {
                continue;
            };
            if sites.iter().any(|s| {
                (s.x - p.position[0]).powi(2) + (s.z - p.position[2]).powi(2) < 45.0_f32.powi(2)
            }) {
                continue;
            }
            if landmarks.iter().any(|s| {
                (s.x - p.position[0]).powi(2) + (s.z - p.position[2]).powi(2) < 15.0_f32.powi(2)
            }) {
                continue;
            }
            let [x, y, z] = p.position;
            match p.kind {
                PropKind::Pine => {
                    let h = (10.0 + random(p.seed, 9) * 7.0) * p.scale;
                    let radius = h * (0.22 + random(p.seed, 10) * 0.055) * p.canopy;
                    let color = mix([0.18, 0.28, 0.17], [0.29, 0.36, 0.19], random(p.seed, 12));
                    cone(
                        &mut mesh,
                        [x, y + h * 0.20, z],
                        radius,
                        h * 0.58,
                        4,
                        random(p.seed, 11) * TAU,
                        mul(color, 0.9),
                        1.0,
                    );
                    cone(
                        &mut mesh,
                        [x, y + h * 0.52, z],
                        radius * 0.60,
                        h * 0.49,
                        4,
                        random(p.seed, 11) * TAU,
                        color,
                        1.0,
                    );
                }
                PropKind::Fir => {
                    let h = (15.0 + random(p.seed, 241) * 9.0) * p.scale;
                    let color = mix([0.15, 0.25, 0.22], [0.22, 0.32, 0.25], random(p.seed, 243));
                    cone(
                        &mut mesh,
                        [x, y + h * 0.13, z],
                        h * 0.17 * p.canopy,
                        h * 0.78,
                        5,
                        random(p.seed, 242) * TAU,
                        color,
                        1.0,
                    );
                }
                PropKind::Broadleaf => {
                    let h = (6.5 + random(p.seed, 15) * 4.0) * p.scale;
                    let color = mix([0.26, 0.34, 0.14], [0.41, 0.44, 0.19], random(p.seed, 17));
                    polyhedron(
                        &mut mesh,
                        [x, y + h * 0.86, z],
                        [h * 0.63, h * 0.35, h * 0.60],
                        p.seed,
                        color,
                        1.0,
                    );
                }
                PropKind::Birch => {
                    let h = (12.0 + random(p.seed, 261) * 7.0) * p.scale;
                    let color = mix([0.37, 0.44, 0.19], [0.47, 0.49, 0.25], random(p.seed, 263));
                    polyhedron(
                        &mut mesh,
                        [x, y + h * 0.73, z],
                        [h * 0.23, h * 0.33, h * 0.23],
                        p.seed,
                        color,
                        1.0,
                    );
                }
                PropKind::Willow => {
                    let h = (8.0 + random(p.seed, 271) * 4.0) * p.scale;
                    let color = mix([0.30, 0.37, 0.19], [0.40, 0.44, 0.24], random(p.seed, 273));
                    polyhedron(
                        &mut mesh,
                        [x, y + h * 0.75, z],
                        [h * 0.54, h * 0.42, h * 0.54],
                        p.seed,
                        color,
                        1.0,
                    );
                }
                _ => {}
            }
        }
    }
    mesh
}

fn owns(ox: f32, oz: f32, x: f32, z: f32) -> bool {
    x >= ox && x < ox + CHUNK_SIZE && z >= oz && z < oz + CHUNK_SIZE
}

/// Collision matches the deterministic tree trunks and boulders used by props_chunk.
/// Landmark structures are intentionally open-sided in this first exploration slice.
pub fn blocks_player(world: &World, x: f32, z: f32) -> bool {
    let gx = (x / PROP_GRID).floor() as i32;
    let gz = (z / PROP_GRID).floor() as i32;
    let sites = world.sites_near(x, z, 50.0);
    let landmarks = world.landmarks_near(x, z, 20.0);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let cell_x = (gx + dx) as f32 * PROP_GRID;
            let cell_z = (gz + dz) as f32 * PROP_GRID;
            if x < cell_x - 4.0
                || x > cell_x + PROP_GRID + 4.0
                || z < cell_z - 4.0
                || z > cell_z + PROP_GRID + 4.0
            {
                continue;
            }
            if let Some(p) = prop_at(world, gx + dx, gz + dz) {
                let radius = match p.kind {
                    PropKind::Pine | PropKind::Fir => 0.48 * p.scale + 0.35,
                    PropKind::Broadleaf | PropKind::Willow => 0.72 * p.scale + 0.35,
                    PropKind::Birch | PropKind::DeadTree => 0.40 * p.scale + 0.35,
                    PropKind::Boulder => 2.25 * p.scale + 0.35,
                    PropKind::Stump => 0.48 * p.scale + 0.35,
                    _ => continue,
                };
                if (p.position[0] - x).powi(2) + (p.position[2] - z).powi(2) >= radius * radius {
                    continue;
                }
                if sites.iter().any(|s| {
                    (s.x - p.position[0]).powi(2) + (s.z - p.position[2]).powi(2) < 45.0_f32.powi(2)
                }) {
                    continue;
                }
                if landmarks.iter().any(|s| {
                    (s.x - p.position[0]).powi(2) + (s.z - p.position[2]).powi(2) < 15.0_f32.powi(2)
                }) {
                    continue;
                }
                return true;
            }
        }
    }
    false
}

fn box_mesh(
    mesh: &mut MeshData,
    center: [f32; 3],
    size: [f32; 3],
    yaw: f32,
    color: [f32; 3],
    material: f32,
) {
    let [sx, sy, sz] = size.map(|v| v * 0.5);
    let points = [
        [-sx, -sy, -sz],
        [sx, -sy, -sz],
        [sx, sy, -sz],
        [-sx, sy, -sz],
        [-sx, -sy, sz],
        [sx, -sy, sz],
        [sx, sy, sz],
        [-sx, sy, sz],
    ]
    .map(|p| {
        [
            center[0] + p[0] * yaw.cos() + p[2] * yaw.sin(),
            center[1] + p[1],
            center[2] - p[0] * yaw.sin() + p[2] * yaw.cos(),
        ]
    });
    for face in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 4, 7, 3],
        [1, 2, 6, 5],
        [3, 7, 6, 2],
        [0, 1, 5, 4],
    ] {
        mesh.quad(
            points[face[0]],
            points[face[1]],
            points[face[2]],
            points[face[3]],
            color,
            material,
        );
    }
}
fn cone(
    mesh: &mut MeshData,
    base: [f32; 3],
    radius: f32,
    height: f32,
    sides: usize,
    yaw: f32,
    color: [f32; 3],
    material: f32,
) {
    let tip = [base[0], base[1] + height, base[2]];
    for i in 0..sides {
        let a = yaw + i as f32 * TAU / sides as f32;
        let b = yaw + (i + 1) as f32 * TAU / sides as f32;
        let p = [
            base[0] + a.sin() * radius,
            base[1],
            base[2] + a.cos() * radius,
        ];
        let q = [
            base[0] + b.sin() * radius,
            base[1],
            base[2] + b.cos() * radius,
        ];
        mesh.triangle(
            p,
            q,
            tip,
            mul(color, 0.96 + (i % 3) as f32 * 0.035),
            material,
        );
        mesh.triangle(base, q, p, mul(color, 0.65), material);
    }
}
fn trunk(mesh: &mut MeshData, base: [f32; 3], radius: f32, height: f32, yaw: f32, color: [f32; 3]) {
    let sides = 5;
    for i in 0..sides {
        let a = yaw + i as f32 * TAU / sides as f32;
        let b = yaw + (i + 1) as f32 * TAU / sides as f32;
        let p = [
            base[0] + a.sin() * radius,
            base[1],
            base[2] + a.cos() * radius,
        ];
        let q = [
            base[0] + b.sin() * radius,
            base[1],
            base[2] + b.cos() * radius,
        ];
        let r = [
            base[0] + b.sin() * radius * 0.65,
            base[1] + height,
            base[2] + b.cos() * radius * 0.65,
        ];
        let s = [
            base[0] + a.sin() * radius * 0.65,
            base[1] + height,
            base[2] + a.cos() * radius * 0.65,
        ];
        mesh.quad(p, q, r, s, mul(color, 0.91 + i as f32 * 0.035), 3.0);
    }
}
fn pine(mesh: &mut MeshData, p: Prop) {
    let h = (10.0 + random(p.seed, 9) * 7.0) * p.scale;
    let r = h * (0.22 + random(p.seed, 10) * 0.055) * p.canopy;
    let yaw = random(p.seed, 11) * TAU;
    let color = mix([0.18, 0.28, 0.17], [0.29, 0.36, 0.19], random(p.seed, 12));
    trunk(
        mesh,
        p.position,
        0.38 * p.scale,
        h * 0.7,
        yaw,
        [0.30, 0.23, 0.15],
    );
    let [x, y, z] = p.position;
    for level in 0..3 {
        let f = level as f32;
        cone(
            mesh,
            [x, y + h * (0.20 + f * 0.22), z],
            r * (1.0 - f * 0.24),
            h * (0.47 - f * 0.035),
            5,
            yaw + f * 0.25,
            mul(color, 0.88 + f * 0.065),
            1.0,
        );
    }
}
// A tapered branch or fallen limb with a small polygonal cross-section.
fn branch(mesh: &mut MeshData, start: [f32; 3], end: [f32; 3], r0: f32, r1: f32, color: [f32; 3]) {
    let delta = sub(end, start);
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if length < 0.001 {
        return;
    }
    let axis = delta.map(|v| v / length);
    let helper = if axis[1].abs() < 0.9 {
        [0., 1., 0.]
    } else {
        [1., 0., 0.]
    };
    let mut right = [
        axis[1] * helper[2] - axis[2] * helper[1],
        axis[2] * helper[0] - axis[0] * helper[2],
        axis[0] * helper[1] - axis[1] * helper[0],
    ];
    let len = (right[0] * right[0] + right[1] * right[1] + right[2] * right[2]).sqrt();
    right = right.map(|v| v / len);
    let up = [
        right[1] * axis[2] - right[2] * axis[1],
        right[2] * axis[0] - right[0] * axis[2],
        right[0] * axis[1] - right[1] * axis[0],
    ];
    let ring = |center: [f32; 3], radius: f32, angle: f32| {
        [
            center[0] + radius * (right[0] * angle.cos() + up[0] * angle.sin()),
            center[1] + radius * (right[1] * angle.cos() + up[1] * angle.sin()),
            center[2] + radius * (right[2] * angle.cos() + up[2] * angle.sin()),
        ]
    };
    for i in 0..5 {
        let a = i as f32 * TAU / 5.;
        let b = (i + 1) as f32 * TAU / 5.;
        let p = ring(start, r0, a);
        let q = ring(start, r0, b);
        let r = ring(end, r1, b);
        let s = ring(end, r1, a);
        mesh.quad(p, s, r, q, mul(color, 0.92 + (i % 3) as f32 * 0.06), 3.0);
    }
}

fn fir(mesh: &mut MeshData, p: Prop) {
    let h = (15.0 + random(p.seed, 241) * 9.0) * p.scale;
    let yaw = random(p.seed, 242) * TAU;
    let color = mix([0.15, 0.25, 0.22], [0.22, 0.32, 0.25], random(p.seed, 243));
    trunk(
        mesh,
        p.position,
        0.37 * p.scale,
        h * 0.90,
        yaw,
        [0.28, 0.23, 0.19],
    );
    let [x, y, z] = p.position;
    for i in 0..5 {
        let f = i as f32;
        let radius = h * (0.17 - f * 0.027) * (0.94 + random(p.seed, 245 + i) * 0.12) * p.canopy;
        cone(
            mesh,
            [x, y + h * (0.13 + f * 0.15), z],
            radius,
            h * (0.32 - f * 0.021),
            6,
            yaw + f * 0.32,
            mul(color, 0.88 + f * 0.032),
            1.0,
        );
    }
}
fn broadleaf(mesh: &mut MeshData, p: Prop) {
    let h = (6.5 + random(p.seed, 15) * 4.0) * p.scale;
    let yaw = random(p.seed, 16) * TAU;
    let [x, y, z] = p.position;
    let color = mix([0.26, 0.34, 0.14], [0.41, 0.44, 0.19], random(p.seed, 17));
    trunk(
        mesh,
        p.position,
        0.64 * p.scale,
        h * 0.64,
        yaw,
        [0.31, 0.24, 0.16],
    );
    for i in 0..3 {
        let a = yaw + i as f32 * TAU / 3.0;
        let off = h * 0.30;
        let center = [
            x + a.sin() * off,
            y + h * (0.81 + random(p.seed, 254 + i) * 0.10),
            z + a.cos() * off,
        ];
        branch(
            mesh,
            [x, y + h * 0.45, z],
            center,
            0.25 * p.scale,
            0.10 * p.scale,
            [0.32, 0.25, 0.16],
        );
        polyhedron(
            mesh,
            center,
            [h * 0.43, h * 0.31, h * 0.41],
            p.seed ^ (i * 17),
            mul(color, 0.92 + i as f32 * 0.04),
            1.0,
        );
    }
}
fn birch(mesh: &mut MeshData, p: Prop) {
    let h = (12.0 + random(p.seed, 261) * 7.0) * p.scale;
    let yaw = random(p.seed, 262) * TAU;
    let [x, y, z] = p.position;
    let lean = h * 0.07;
    let pale = [0.69, 0.68, 0.57];
    let end = [x + yaw.sin() * lean, y + h * 0.88, z + yaw.cos() * lean];
    branch(mesh, p.position, end, 0.24 * p.scale, 0.095 * p.scale, pale);
    let color = mix([0.37, 0.44, 0.19], [0.47, 0.49, 0.25], random(p.seed, 263));
    for i in 0..3 {
        let a = yaw + 1.9 * i as f32;
        let level = 0.58 + i as f32 * 0.14;
        let center = [
            x + yaw.sin() * lean * level + a.sin() * h * 0.10,
            y + h * level,
            z + yaw.cos() * lean * level + a.cos() * h * 0.10,
        ];
        branch(
            mesh,
            [
                x + yaw.sin() * lean * 0.5,
                y + h * 0.46,
                z + yaw.cos() * lean * 0.5,
            ],
            center,
            0.13 * p.scale,
            0.055 * p.scale,
            pale,
        );
        polyhedron(
            mesh,
            center,
            [h * 0.17, h * 0.20, h * 0.16],
            p.seed ^ (i * 71),
            mul(color, 0.90 + i as f32 * 0.06),
            1.0,
        );
    }
    // A few charcoal bark scars make pale trunks read as birches at walking distance.
    for i in 0..4 {
        let f = 0.12 + i as f32 * 0.10;
        let center = [
            x + yaw.sin() * lean * f,
            y + h * f,
            z + yaw.cos() * lean * f,
        ];
        let r = 0.24 * p.scale * (1.0 - f * 0.5);
        mesh.quad(
            [center[0] - r, center[1], center[2] + r],
            [center[0] + r, center[1], center[2] + r],
            [center[0] + r * 0.5, center[1] + 0.10, center[2] + r],
            [center[0] - r, center[1] + 0.10, center[2] + r],
            [0.28, 0.29, 0.26],
            3.0,
        );
    }
}
fn willow(mesh: &mut MeshData, p: Prop) {
    let h = (8.0 + random(p.seed, 271) * 4.0) * p.scale;
    let yaw = random(p.seed, 272) * TAU;
    let [x, y, z] = p.position;
    trunk(
        mesh,
        p.position,
        0.65 * p.scale,
        h * 0.64,
        yaw,
        [0.34, 0.29, 0.20],
    );
    let color = mix([0.30, 0.37, 0.19], [0.40, 0.44, 0.24], random(p.seed, 273));
    polyhedron(
        mesh,
        [x, y + h * 0.85, z],
        [h * 0.45, h * 0.32, h * 0.43],
        p.seed,
        color,
        1.0,
    );
    for i in 0..4 {
        let a = yaw + i as f32 * TAU / 4.0;
        let off = h * 0.39;
        let center = [x + a.sin() * off, y + h * 0.67, z + a.cos() * off];
        branch(
            mesh,
            [x, y + h * 0.47, z],
            [center[0], y + h * 0.85, center[2]],
            0.22 * p.scale,
            0.08 * p.scale,
            [0.34, 0.29, 0.20],
        );
        // Narrow hanging lobes distinguish the willow from a round oak crown.
        polyhedron(
            mesh,
            center,
            [h * 0.20, h * 0.36, h * 0.20],
            p.seed ^ (i * 31),
            mul(color, 0.90 + i as f32 * 0.025),
            1.0,
        );
    }
}
fn dead_tree(mesh: &mut MeshData, p: Prop) {
    let h = (5.0 + random(p.seed, 281) * 4.0) * p.scale;
    let yaw = random(p.seed, 282) * TAU;
    let [x, y, z] = p.position;
    let lean = if p.biome == Biome::Moor {
        h * 0.28
    } else {
        h * 0.13
    };
    let crown = [x + yaw.sin() * lean, y + h, z + yaw.cos() * lean];
    let color = [0.40, 0.36, 0.28];
    branch(
        mesh,
        p.position,
        crown,
        0.34 * p.scale,
        0.09 * p.scale,
        color,
    );
    for i in 0..4 {
        let f = 0.35 + i as f32 * 0.14;
        let a = yaw + i as f32 * 2.2;
        let start = [
            x + yaw.sin() * lean * f,
            y + h * f,
            z + yaw.cos() * lean * f,
        ];
        let end = [
            start[0] + a.sin() * h * 0.26,
            start[1] + h * 0.25,
            start[2] + a.cos() * h * 0.26,
        ];
        branch(mesh, start, end, 0.16 * p.scale, 0.025 * p.scale, color);
        if i % 2 == 0 {
            branch(
                mesh,
                end,
                [
                    end[0] + yaw.sin() * h * 0.13,
                    end[1] + h * 0.14,
                    end[2] + yaw.cos() * h * 0.13,
                ],
                0.06 * p.scale,
                0.018 * p.scale,
                color,
            );
        }
    }
}
fn fallen_log(mesh: &mut MeshData, p: Prop, world: &World) {
    let yaw = random(p.seed, 291) * TAU;
    let len = (3.0 + random(p.seed, 292) * 3.0) * p.scale;
    let r = 0.36 * p.scale;
    let [x, y, z] = p.position;
    let ex = x + yaw.sin() * len;
    let ez = z + yaw.cos() * len;
    let end = [
        ex,
        terrain_surface_height(world, ex, ez) + r * 0.82 - 0.08,
        ez,
    ];
    branch(mesh, [x, y + r, z], end, r, r * 0.82, [0.31, 0.25, 0.17]);
    polyhedron(
        mesh,
        [x, y + r, z],
        [r * 1.12, r * 1.12, r * 1.12],
        p.seed,
        [0.47, 0.37, 0.23],
        3.0,
    );
    let middle_y = (y + r) + (end[1] - (y + r)) * 0.55;
    branch(
        mesh,
        [
            x + yaw.sin() * len * 0.55,
            middle_y,
            z + yaw.cos() * len * 0.55,
        ],
        [
            x + yaw.sin() * len * 0.55 + 0.7,
            middle_y + 0.8,
            z + yaw.cos() * len * 0.55,
        ],
        0.12 * p.scale,
        0.04 * p.scale,
        [0.31, 0.25, 0.17],
    );
}
fn stump(mesh: &mut MeshData, p: Prop) {
    let height = (0.5 + random(p.seed, 296) * 0.6) * p.scale;
    trunk(
        mesh,
        p.position,
        0.45 * p.scale,
        height,
        random(p.seed, 297) * TAU,
        [0.32, 0.25, 0.17],
    );
    cone(
        mesh,
        [p.position[0], p.position[1] + height, p.position[2]],
        0.32 * p.scale,
        0.04,
        5,
        0.0,
        [0.55, 0.43, 0.26],
        3.0,
    );
}

// Small ground cover is one deliberately bounded layer: at most 4,096 candidate
// clumps per chunk and 4–8 triangles per clump. Material6 can fade independently.
fn ground_cover(world: &World, mesh: &mut MeshData, ox: f32, oz: f32) {
    let divisions = 64;
    let spacing = 3.0;
    let mut grid = Vec::with_capacity(33 * 33);
    for z in 0..=32 {
        for x in 0..=32 {
            grid.push(ground_vertex(
                world,
                ox + x as f32 * 6.0,
                oz + z as f32 * 6.0,
            ));
        }
    }
    for iz in 0..divisions {
        for ix in 0..divisions {
            let gx = (ox / spacing) as i32 + ix as i32;
            let gz = (oz / spacing) as i32 + iz as i32;
            let seed = hash(world.seed ^ 0x47524153, gx, gz);
            let x = ox + (ix as f32 + 0.12 + random(seed, 301) * 0.76) * spacing;
            let z = oz + (iz as f32 + 0.12 + random(seed, 302) * 0.76) * spacing;
            let sample = world.sample(x, z);
            if sample.road > 0.10 || sample.water_height > sample.height + 0.15 {
                continue;
            }
            let cover = ecology::sample(world.seed, x, z, &sample);
            if random(seed, 304) > cover.grass_density {
                continue;
            }
            let y = grid_height(world, ox, oz, 32, &grid, x, z) - 0.018;
            let scale = (0.62 + random(seed, 305) * 0.80) * cover.grass_height;
            let kind = random(seed, 306);
            let open = 1.0 - cover.tree_density;
            let color = match sample.biome {
                Biome::Grassland | Biome::Forest => {
                    mix([0.25, 0.41, 0.17], [0.38, 0.52, 0.19], open)
                }
                Biome::PineForest => mix([0.25, 0.36, 0.23], [0.39, 0.48, 0.23], open),
                Biome::Wetland => [0.37, 0.49, 0.23],
                Biome::Moor => [0.46, 0.46, 0.25],
                Biome::Alpine => [0.49, 0.50, 0.34],
                Biome::Desert => [0.70, 0.55, 0.29],
            };
            let first_vertex = mesh.vertices.len();
            if kind < cover.ferns {
                fern(mesh, [x, y, z], scale, seed, color);
            } else if kind < cover.ferns + cover.flowers {
                flowers(mesh, [x, y, z], scale, seed, color);
            } else if kind < cover.ferns + cover.flowers + cover.heather {
                heather(mesh, [x, y, z], scale, seed);
            } else {
                grass_clump(
                    mesh,
                    [x, y, z],
                    scale,
                    seed,
                    color,
                    sample.biome == Biome::Wetland,
                );
            }
            drape_cover(world, mesh, first_vertex, ox, oz, &grid, y + 0.018);
            // Infrequent palm-sized stones interrupt the vegetation without adding a
            // second dense field or collision obstacles around the player's feet.
            if kind > 0.974 {
                let rx = x + 0.55;
                let rz = z - 0.25;
                let ry = terrain_surface_height(world, rx, rz);
                polyhedron(
                    mesh,
                    [rx, ry + 0.07, rz],
                    [0.34, 0.22, 0.27],
                    seed,
                    [0.44, 0.43, 0.35],
                    2.0,
                );
            }
        }
    }
}
// Each blade endpoint roots to its own position on the rendered triangle. Using
// one shared height for a whole clump caused floating blades on steep slopes.
fn drape_cover(
    world: &World,
    mesh: &mut MeshData,
    first: usize,
    ox: f32,
    oz: f32,
    grid: &[GroundVertex],
    center_height: f32,
) {
    for triangle in mesh.vertices[first..].chunks_exact_mut(3) {
        for v in triangle.iter_mut() {
            let x = v.position[0];
            let z = v.position[2];
            let h = if x >= ox && x <= ox + CHUNK_SIZE && z >= oz && z <= oz + CHUNK_SIZE {
                grid_height(world, ox, oz, 32, grid, x, z)
            } else {
                terrain_surface_height(world, x, z)
            };
            v.position[1] += h - center_height;
        }
        let a = sub(triangle[1].position, triangle[0].position);
        let b = sub(triangle[2].position, triangle[0].position);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 0.000001 {
            let normal = n.map(|v| v / len);
            for v in triangle {
                v.normal = normal;
            }
        }
    }
}
fn grass_clump(
    mesh: &mut MeshData,
    base: [f32; 3],
    scale: f32,
    seed: u32,
    color: [f32; 3],
    wet: bool,
) {
    let count = if wet { 5 } else { 4 };
    for i in 0..count {
        let a = random(seed, 310 + i) * TAU;
        let offset = random(seed, 320 + i) * 0.55 * scale;
        let x = base[0] + a.sin() * offset;
        let z = base[2] + a.cos() * offset;
        let h = (0.30 + random(seed, 330 + i) * 0.50) * scale * if wet { 1.3 } else { 1.0 };
        let w = (0.14 + random(seed, 340 + i) * 0.10) * scale;
        let bend = a + 0.5;
        let lean = h * 0.26;
        mesh.triangle(
            [x - a.cos() * w, base[1], z + a.sin() * w],
            [x + a.cos() * w, base[1], z - a.sin() * w],
            [x + bend.sin() * lean, base[1] + h, z + bend.cos() * lean],
            mul(color, 0.90 + i as f32 * 0.045),
            6.0,
        );
    }
}
fn fern(mesh: &mut MeshData, base: [f32; 3], scale: f32, seed: u32, color: [f32; 3]) {
    for i in 0..4 {
        let a = random(seed, 352) * TAU + i as f32 * TAU / 4.0;
        let len = (0.55 + random(seed, 355 + i) * 0.40) * scale;
        let w = len * 0.21;
        let middle = [
            base[0] + a.sin() * len * 0.56,
            base[1] + 0.42 * scale,
            base[2] + a.cos() * len * 0.56,
        ];
        let left = [middle[0] - a.cos() * w, middle[1], middle[2] + a.sin() * w];
        let right = [middle[0] + a.cos() * w, middle[1], middle[2] - a.sin() * w];
        let tip = [
            base[0] + a.sin() * len,
            base[1] + 0.24 * scale,
            base[2] + a.cos() * len,
        ];
        let tone = 0.90 + i as f32 * 0.035;
        mesh.triangle(base, left, tip, mul(color, tone), 6.0);
        mesh.triangle(base, tip, right, mul(color, tone * 0.88), 6.0);
    }
}
fn heather(mesh: &mut MeshData, base: [f32; 3], scale: f32, seed: u32) {
    grass_clump(mesh, base, scale * 0.70, seed, [0.36, 0.34, 0.22], false);
    for i in 0..3 {
        let a = random(seed, 368 + i) * TAU;
        let offset = 0.25 * scale;
        let h = (0.3 + random(seed, 372 + i) * 0.18) * scale;
        let x = base[0] + a.sin() * offset;
        let z = base[2] + a.cos() * offset;
        let w = 0.17 * scale;
        mesh.triangle(
            [x - w, base[1] + h * 0.62, z],
            [x + w, base[1] + h * 0.62, z],
            [x, base[1] + h, z + 0.05],
            [0.47, 0.32, 0.38],
            6.0,
        );
    }
}
fn flowers(mesh: &mut MeshData, base: [f32; 3], scale: f32, seed: u32, color: [f32; 3]) {
    grass_clump(mesh, base, scale * 0.64, seed, color, false);
    let tint = random(seed, 383);
    let petals = if tint < 0.50 {
        [0.86, 0.65, 0.22]
    } else if tint < 0.88 {
        [0.85, 0.79, 0.51]
    } else {
        [0.77, 0.42, 0.29]
    };
    for i in 0..2 {
        let a = random(seed, 385 + i) * TAU;
        let x = base[0] + a.sin() * 0.32;
        let z = base[2] + a.cos() * 0.32;
        let y = base[1] + 0.42 * scale;
        mesh.triangle(
            [x - 0.025, base[1], z],
            [x + 0.025, base[1], z],
            [x, y + 0.025, z],
            color,
            6.0,
        );
        mesh.triangle(
            [x - 0.11, y, z],
            [x + 0.11, y, z],
            [x, y + 0.15, z + 0.04],
            petals,
            6.0,
        );
    }
}

fn polyhedron(
    mesh: &mut MeshData,
    center: [f32; 3],
    scale: [f32; 3],
    seed: u32,
    color: [f32; 3],
    material: f32,
) {
    let t = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let vertices = [
        [-1., t, 0.],
        [1., t, 0.],
        [-1., -t, 0.],
        [1., -t, 0.],
        [0., -1., t],
        [0., 1., t],
        [0., -1., -t],
        [0., 1., -t],
        [t, 0., -1.],
        [t, 0., 1.],
        [-t, 0., -1.],
        [-t, 0., 1.],
    ];
    let mut positions = [[0.; 3]; 12];
    for (i, p) in vertices.iter().enumerate() {
        let k = (0.90 + random(seed, 30 + i as u32) * 0.20) / (1.0 + t * t).sqrt();
        positions[i] = [
            center[0] + p[0] * scale[0] * k,
            center[1] + p[1] * scale[1] * k,
            center[2] + p[2] * scale[2] * k,
        ];
    }
    let faces = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    for (i, face) in faces.iter().enumerate() {
        mesh.triangle(
            positions[face[0]],
            positions[face[1]],
            positions[face[2]],
            mul(color, 0.94 + random(seed, 60 + i as u32) * 0.12),
            material,
        );
    }
}
fn rock(mesh: &mut MeshData, base: [f32; 3], scale: f32, seed: u32) {
    let size = scale * (1.2 + random(seed, 90) * 1.4);
    polyhedron(
        mesh,
        [base[0], base[1] + size * 0.43, base[2]],
        [size * 1.18, size * 0.95, size],
        seed,
        [0.43, 0.44, 0.40],
        2.0,
    );
}
fn reeds(mesh: &mut MeshData, p: Prop) {
    for i in 0..4 {
        let a = random(p.seed, 100 + i) * TAU;
        let r = random(p.seed, 110 + i) * 1.5;
        let h = (1.0 + random(p.seed, 120 + i)) * p.scale;
        box_mesh(
            mesh,
            [
                p.position[0] + a.sin() * r,
                p.position[1] + h / 2.0,
                p.position[2] + a.cos() * r,
            ],
            [0.11, h, 0.11],
            a,
            [0.40, 0.42, 0.22],
            1.0,
        );
    }
}

fn banner(mesh: &mut MeshData, base: [f32; 3], height: f32, seed: u32) {
    trunk(mesh, base, 0.16, height, 0.0, [0.37, 0.27, 0.16]);
    let red = mix([0.48, 0.17, 0.12], [0.27, 0.31, 0.42], random(seed, 135));
    let [x, y, z] = base;
    let a = [x + 0.12, y + height - 0.3, z];
    let b = [x + 2.9, y + height - 0.45, z + 0.2];
    let c = [x + 2.65, y + height - 2.0, z + 0.3];
    let d = [x + 0.12, y + height - 2.25, z];
    mesh.quad(a, b, c, d, red, 5.0);
    mesh.quad(d, c, b, a, mul(red, 0.95), 5.0);
    box_mesh(
        mesh,
        [x + 1.2, y + height - 1.15, z + 0.10],
        [0.24, 1.35, 0.025],
        0.0,
        [0.73, 0.62, 0.36],
        5.0,
    );
}
fn settlement_marker(mesh: &mut MeshData, base: [f32; 3], seed: u32, kind: &str) {
    let [x, y, z] = base;
    // A survey/watch platform makes future settlement sites recognizable from a distance.
    let height = if kind == "town" { 12.0 } else { 8.0 };
    let wood = [0.35, 0.27, 0.18];
    for dx in [-2.8, 2.8] {
        for dz in [-2.8, 2.8] {
            box_mesh(
                mesh,
                [x + dx, y + height * 0.5, z + dz],
                [0.6, height, 0.6],
                0.0,
                wood,
                3.0,
            );
        }
    }
    box_mesh(
        mesh,
        [x, y + height - 2.0, z],
        [7.1, 0.45, 7.1],
        0.0,
        wood,
        3.0,
    );
    cone(
        mesh,
        [x, y + height + 0.2, z],
        5.3,
        3.2,
        4,
        PI / 4.0,
        [0.36, 0.23, 0.17],
        3.0,
    );
    banner(mesh, [x + 3.4, y, z + 3.4], height + 5.0, seed);
    for j in 0..8 {
        box_mesh(
            mesh,
            [x - 2.65, y + 0.8 + j as f32 * 0.7, z + 3.5],
            [1.0, 0.12, 0.28],
            0.0,
            wood,
            3.0,
        );
    }
    box_mesh(
        mesh,
        [x - 3.17, y + 3.3, z + 3.5],
        [0.15, 6.6, 0.15],
        0.0,
        wood,
        3.0,
    );
    box_mesh(
        mesh,
        [x - 2.12, y + 3.3, z + 3.5],
        [0.15, 6.6, 0.15],
        0.0,
        wood,
        3.0,
    );
    signpost(mesh, [x + 8.0, y, z + 5.0], seed);
    camp(mesh, [x - 10.0, y, z + 8.0], seed ^ 31);
}
fn signpost(mesh: &mut MeshData, base: [f32; 3], seed: u32) {
    let [x, y, z] = base;
    let yaw = random(seed, 141) * 0.5;
    box_mesh(
        mesh,
        [x, y + 2.6, z],
        [0.34, 5.2, 0.34],
        yaw,
        [0.35, 0.25, 0.15],
        3.0,
    );
    box_mesh(
        mesh,
        [x, y + 4.25, z],
        [3.6, 0.75, 0.20],
        yaw,
        [0.49, 0.36, 0.20],
        3.0,
    );
    box_mesh(
        mesh,
        [x - 0.35, y + 3.2, z],
        [2.9, 0.68, 0.20],
        yaw,
        [0.43, 0.31, 0.18],
        3.0,
    );
    // Small inset route marks stay legible as simple high-contrast shapes at distance.
    box_mesh(
        mesh,
        [x + 0.92, y + 4.25, z + 0.13],
        [0.75, 0.12, 0.06],
        yaw,
        [0.18, 0.17, 0.12],
        3.0,
    );
}
fn landmark_mesh(mesh: &mut MeshData, base: [f32; 3], seed: u32, kind: &str) {
    let [x, y, z] = base;
    match kind {
        "watchtower" => {
            let stone = [0.47, 0.46, 0.39];
            // Open doorway and open observation level, all assembled from rectangular piers.
            for dx in [-2.7, 2.7] {
                for dz in [-2.7, 2.7] {
                    box_mesh(
                        mesh,
                        [x + dx, y + 8.0, z + dz],
                        [1.5, 16.0, 1.5],
                        0.0,
                        stone,
                        2.0,
                    );
                }
            }
            box_mesh(
                mesh,
                [x, y + 7.0, z - 2.7],
                [4.0, 14.0, 1.5],
                0.0,
                stone,
                2.0,
            );
            box_mesh(
                mesh,
                [x - 2.7, y + 7.0, z],
                [1.5, 14.0, 4.0],
                0.0,
                stone,
                2.0,
            );
            box_mesh(
                mesh,
                [x + 2.7, y + 7.0, z],
                [1.5, 14.0, 4.0],
                0.0,
                stone,
                2.0,
            );
            box_mesh(
                mesh,
                [x, y + 9.5, z + 2.7],
                [4.0, 9.0, 1.5],
                0.0,
                stone,
                2.0,
            );
            box_mesh(
                mesh,
                [x, y + 14.4, z],
                [7.3, 0.7, 7.3],
                0.0,
                [0.37, 0.37, 0.33],
                2.0,
            );
            cone(
                mesh,
                [x, y + 18.0, z],
                6.0,
                4.0,
                4,
                PI / 4.0,
                [0.36, 0.23, 0.18],
                3.0,
            );
            banner(mesh, [x + 4.0, y, z + 4.0], 9.0, seed);
        }
        "ruin" => {
            let stone = [0.48, 0.46, 0.39];
            for i in 0..7 {
                let a = i as f32 * TAU / 8.0;
                let h = 3.0 + random(seed, 150 + i) * 7.0;
                box_mesh(
                    mesh,
                    [x + a.sin() * 6.5, y + h / 2.0, z + a.cos() * 6.5],
                    [2.5, h, 1.6],
                    a,
                    stone,
                    2.0,
                );
            }
            box_mesh(
                mesh,
                [x, y + 6.7, z + 6.5],
                [7.0, 1.4, 1.6],
                0.0,
                stone,
                2.0,
            );
            for i in 0..5 {
                let a = random(seed, 165 + i) * TAU;
                rock(
                    mesh,
                    [x + a.sin() * 9.0, y, z + a.cos() * 9.0],
                    0.8,
                    seed ^ i,
                );
            }
        }
        "standing_stones" => {
            for i in 0..7 {
                let a = i as f32 * TAU / 7.0;
                let h = 4.0 + random(seed, 180 + i) * 3.5;
                polyhedron(
                    mesh,
                    [x + a.sin() * 7.2, y + h * 0.43, z + a.cos() * 7.2],
                    [1.6, h * 0.85, 1.2],
                    seed ^ i,
                    [0.43, 0.45, 0.43],
                    2.0,
                );
            }
            box_mesh(
                mesh,
                [x, y + 0.5, z],
                [3.5, 1.0, 2.3],
                random(seed, 188),
                [0.46, 0.45, 0.41],
                2.0,
            );
        }
        "camp" => camp(mesh, base, seed),
        "shrine" => {
            let stone = [0.53, 0.51, 0.43];
            box_mesh(mesh, [x, y + 0.35, z], [6.0, 0.7, 6.0], 0.0, stone, 2.0);
            box_mesh(mesh, [x, y + 0.95, z], [4.5, 0.5, 4.5], 0.0, stone, 2.0);
            for dx in [-1.8, 1.8] {
                for dz in [-1.8, 1.8] {
                    box_mesh(
                        mesh,
                        [x + dx, y + 3.1, z + dz],
                        [0.5, 4.0, 0.5],
                        0.0,
                        stone,
                        2.0,
                    );
                }
            }
            cone(
                mesh,
                [x, y + 5.2, z],
                3.8,
                2.1,
                4,
                PI / 4.0,
                [0.32, 0.36, 0.34],
                2.0,
            );
            polyhedron(
                mesh,
                [x, y + 2.4, z],
                [0.9, 1.35, 0.8],
                seed,
                [0.61, 0.56, 0.37],
                2.0,
            );
            banner(mesh, [x + 5.0, y, z], 6.2, seed);
        }
        _ => {
            for i in 0..4 {
                let f = i as f32;
                polyhedron(
                    mesh,
                    [x, y + 0.6 + f * 1.0, z],
                    [2.0 - f * 0.34, 1.0, 1.6 - f * 0.28],
                    seed ^ i,
                    [0.45, 0.44, 0.39],
                    2.0,
                );
            }
            signpost(mesh, [x + 3.0, y, z], seed);
        }
    }
}
fn camp(mesh: &mut MeshData, base: [f32; 3], seed: u32) {
    let [x, y, z] = base;
    let cloth = mix([0.57, 0.46, 0.30], [0.44, 0.35, 0.23], random(seed, 200));
    let a = [x - 3.0, y, z - 3.0];
    let b = [x + 3.0, y, z - 3.0];
    let c = [x, y + 3.8, z - 3.0];
    let d = [x - 3.0, y, z + 3.0];
    let e = [x + 3.0, y, z + 3.0];
    let f = [x, y + 3.8, z + 3.0];
    mesh.quad(a, d, f, c, cloth, 5.0);
    mesh.quad(c, f, e, b, mul(cloth, 0.91), 5.0);
    mesh.triangle(a, c, b, mul(cloth, 0.8), 5.0);
    // Entrance remains visibly dark and open.
    mesh.triangle(d, [x - 0.8, y, z + 3.0], f, mul(cloth, 0.85), 5.0);
    mesh.triangle(f, [x + 0.8, y, z + 3.0], e, mul(cloth, 0.85), 5.0);
    for dx in [-3.15, 3.15] {
        box_mesh(
            mesh,
            [x + dx, y + 0.65, z + 3.2],
            [0.16, 1.3, 0.16],
            0.0,
            [0.34, 0.25, 0.16],
            3.0,
        );
    }
    for i in 0..7 {
        let a = i as f32 * TAU / 7.0;
        rock(
            mesh,
            [x + 6.0 + a.sin() * 1.2, y, z + a.cos() * 1.2],
            0.24,
            seed ^ i,
        );
    }
    box_mesh(
        mesh,
        [x + 6.0, y + 0.25, z],
        [1.8, 0.4, 0.38],
        0.8,
        [0.24, 0.18, 0.12],
        3.0,
    );
    box_mesh(
        mesh,
        [x + 6.0, y + 0.35, z],
        [1.8, 0.4, 0.38],
        -0.8,
        [0.28, 0.20, 0.13],
        3.0,
    );
    cone(
        mesh,
        [x + 6.0, y + 0.40, z],
        0.35,
        0.75,
        5,
        0.0,
        [0.76, 0.35, 0.08],
        5.0,
    );
    box_mesh(
        mesh,
        [x + 4.0, y + 0.35, z + 3.0],
        [3.0, 0.55, 0.6],
        0.1,
        [0.35, 0.26, 0.17],
        3.0,
    );
}

// Each deck endpoint meets the higher of the bank or the river clearance.
// Adjacent spans share endpoints, so bank ramps and walking height are continuous.
std::thread_local! {
    // Bridge nodes repeat in adjacent spans, streamed meshes, and every physics
    // frame. Cache only the immutable water envelope, with a bounded footprint.
    static BRIDGE_WATER_NODES: std::cell::RefCell<std::collections::HashMap<(u32,i32,i32),f32>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
fn bridge_water_envelope(world: &World, p: [f32; 2]) -> f32 {
    let key = (
        world.seed,
        (p[0] * 4.0).round() as i32,
        (p[1] * 4.0).round() as i32,
    );
    if let Some(height) = BRIDGE_WATER_NODES.with(|cache| cache.borrow().get(&key).copied()) {
        return height;
    }
    let center = [key.1 as f32 * 0.25, key.2 as f32 * 0.25];
    let mut water = -10000.0_f32;
    // The orientation-independent 7.5m neighborhood contains the entire next
    // five-meter span and its seven-meter deck width, including upstream edges.
    // Both endpoints inspect the span interior as well as its upstream deck edge.
    for z in -4..=4 {
        for x in -4..=4 {
            let dx = x as f32 * 1.5;
            let dz = z as f32 * 1.5;
            if dx * dx + dz * dz > 7.5 * 7.5 {
                continue;
            }
            water = water.max(world.sample(center[0] + dx, center[1] + dz).water_height);
        }
    }
    BRIDGE_WATER_NODES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(key, water);
    });
    water
}
fn bridge_heights(world: &World, start: [f32; 2], end: [f32; 2]) -> Option<(f32, f32)> {
    let water0 = bridge_water_envelope(world, start);
    let water1 = bridge_water_envelope(world, end);
    if water0 < -999.0 && water1 < -999.0 {
        return None;
    }
    let ground0 = terrain_surface_height(world, start[0], start[1]);
    let ground1 = terrain_surface_height(world, end[0], end[1]);
    // A small reserve covers sub-grid water gradients. Heights depend only on a
    // shared endpoint, never its neighboring span, preserving continuous ramps.
    let floor0 = ground0.max(water0 + 1.55);
    let floor1 = ground1.max(water1 + 1.55);
    let middle = world.sample((start[0] + end[0]) / 2.0, (start[1] + end[1]) / 2.0);
    if floor0 <= ground0 && floor1 <= ground1 && middle.water_height + 1.3 <= middle.height {
        return None;
    }
    Some((floor0 + 0.015, floor1 + 0.015))
}

fn sloped_box(
    mesh: &mut MeshData,
    start: [f32; 2],
    end: [f32; 2],
    y0: f32,
    y1: f32,
    width: f32,
    thickness: f32,
    color: [f32; 3],
    material: f32,
) {
    let dx = end[0] - start[0];
    let dz = end[1] - start[1];
    let len = (dx * dx + dz * dz).sqrt();
    if len < 0.001 {
        return;
    }
    let rx = dz / len * width / 2.0;
    let rz = -dx / len * width / 2.0;
    let top = [
        [start[0] - rx, y0, start[1] - rz],
        [end[0] - rx, y1, end[1] - rz],
        [end[0] + rx, y1, end[1] + rz],
        [start[0] + rx, y0, start[1] + rz],
    ];
    let bottom = top.map(|p| [p[0], p[1] - thickness, p[2]]);
    mesh.quad(top[0], top[1], top[2], top[3], color, material);
    mesh.quad(
        bottom[3],
        bottom[2],
        bottom[1],
        bottom[0],
        mul(color, 0.70),
        material,
    );
    for i in 0..4 {
        let j = (i + 1) % 4;
        mesh.quad(
            top[i],
            bottom[i],
            bottom[j],
            top[j],
            mul(color, 0.90),
            material,
        );
    }
}
fn bridges(world: &World, mesh: &mut MeshData, ox: f32, oz: f32) {
    let roads = world.roads_near(ox + CHUNK_SIZE / 2.0, oz + CHUNK_SIZE / 2.0, CHUNK_SIZE);
    for road in roads {
        for segment in road.windows(2) {
            let a = segment[0];
            let b = segment[1];
            let dx = b[0] - a[0];
            let dz = b[1] - a[1];
            let len = (dx * dx + dz * dz).sqrt();
            if len < 0.01 {
                continue;
            }
            let pieces = (len / 5.0).ceil() as usize;
            let piece_len = len / pieces as f32;
            let yaw = dx.atan2(dz);
            for i in 0..pieces {
                let t = (i as f32 + 0.5) / pieces as f32;
                let x = a[0] + dx * t;
                let z = a[1] + dz * t;
                if !owns(ox, oz, x, z) {
                    continue;
                }
                let start = [
                    x - dx / len * piece_len / 2.0,
                    z - dz / len * piece_len / 2.0,
                ];
                let end = [
                    x + dx / len * piece_len / 2.0,
                    z + dz / len * piece_len / 2.0,
                ];
                let Some((y0, y1)) = bridge_heights(world, start, end) else {
                    continue;
                };
                let y = (y0 + y1) / 2.0;
                let wood = [0.46, 0.34, 0.21];
                sloped_box(mesh, start, end, y0, y1, 7.0, 0.36, wood, 3.0);
                // Individual dark plank seams and posts make crossings recognizable on approach.
                for j in 0..4 {
                    let f = j as f32 / 4.0;
                    let along = (f - 0.5) * piece_len;
                    box_mesh(
                        mesh,
                        [
                            x + dx / len * along,
                            y0 + (y1 - y0) * f + 0.014,
                            z + dz / len * along,
                        ],
                        [7.02, 0.025, 0.055],
                        yaw,
                        [0.31, 0.25, 0.17],
                        3.0,
                    );
                }
                for side in [-1.0, 1.0] {
                    let px = x + dz / len * 3.35 * side;
                    let pz = z - dx / len * 3.35 * side;
                    box_mesh(
                        mesh,
                        [px, y - 0.15, pz],
                        [0.28, 2.6, 0.28],
                        yaw,
                        [0.34, 0.26, 0.17],
                        3.0,
                    );
                    let offset = [dz / len * 3.35 * side, -dx / len * 3.35 * side];
                    sloped_box(
                        mesh,
                        [start[0] + offset[0], start[1] + offset[1]],
                        [end[0] + offset[0], end[1] + offset[1]],
                        y0 + 1.05,
                        y1 + 1.05,
                        0.2,
                        0.2,
                        [0.39, 0.29, 0.18],
                        3.0,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_keeps_its_upstream_deck_edge_above_water() {
        // Regression: center-only water samples submerged this edge of a
        // seven-meter crossing, even though its center appeared to be clear.
        let world = World::new(1337);
        let (x, z) = (-7614.833, 2457.720);
        let sample = world.sample(x, z);
        assert!(sample.road > 0.8 && sample.water_height > sample.height);
        assert!(walk_height(&world, x, z) > sample.water_height + 1.3);
    }
    fn assert_mesh(mesh: &MeshData) {
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
        assert!(mesh.vertices.iter().all(|v| v
            .position
            .iter()
            .chain(v.normal.iter())
            .chain(v.color.iter())
            .all(|n| n.is_finite())));
        for v in &mesh.vertices {
            let n = v.normal.iter().map(|n| n * n).sum::<f32>();
            assert!((n - 1.0).abs() < 0.001);
        }
    }
    #[test]
    fn primitives_are_finite_and_indexed() {
        let mut m = MeshData::default();
        box_mesh(
            &mut m,
            [2., 3., 4.],
            [3., 5., 2.],
            0.4,
            [0.4, 0.5, 0.2],
            0.0,
        );
        cone(&mut m, [0., 0., 0.], 2., 5., 5, 0., [0.3, 0.4, 0.2], 1.0);
        polyhedron(&mut m, [1., 2., 3.], [2., 4., 2.], 17, [0.4, 0.4, 0.4], 2.0);
        sloped_box(
            &mut m,
            [0., 0.],
            [2., 5.],
            0.,
            1.,
            7.,
            0.3,
            [0.3, 0.2, 0.1],
            3.0,
        );
        assert_mesh(&m);
        assert!(!m.vertices.is_empty());
    }
    #[test]
    fn ground_winding_faces_up() {
        let mut m = MeshData::default();
        m.triangle([0., 0., 0.], [0., 0., 1.], [1., 0., 0.], [1., 1., 1.], 0.0);
        assert_eq!(m.vertices[0].normal, [0., 1., 0.]);
    }
    #[test]
    fn terrain_and_water_are_finite_at_all_lods() {
        let w = World::new(1337);
        for lod in 0..=4 {
            let mesh = terrain_chunk(&w, -1, 29, lod);
            assert_mesh(&mesh);
            assert!(!mesh.vertices.is_empty());
            assert_mesh(&water_chunk(&w, -1, 29, lod));
        }
    }
    #[test]
    fn walking_surface_matches_finest_terrain() {
        let w = World::new(1337);
        let ox = -192.0;
        let oz = 5568.0;
        let mut grid = Vec::new();
        for z in 0..=32 {
            for x in 0..=32 {
                grid.push(ground_vertex(&w, ox + x as f32 * 6.0, oz + z as f32 * 6.0));
            }
        }
        for (x, z) in [
            (-191.5, 5568.4),
            (-120.3, 5601.7),
            (-6.0, 5740.3),
            (-0.01, 5759.8),
        ] {
            assert!(
                (terrain_surface_height(&w, x, z) - grid_height(&w, ox, oz, 32, &grid, x, z)).abs()
                    < 0.0001
            );
        }
    }
    #[test]
    fn props_are_deterministic() {
        let w = World::new(1337);
        let a = props_chunk(&w, -1, 29);
        let b = props_chunk(&w, -1, 29);
        assert_mesh(&a);
        assert_eq!(a.vertices.len(), b.vertices.len());
        for (a, b) in a.vertices.iter().zip(b.vertices.iter()) {
            assert_eq!(a.position, b.position);
            assert_eq!(a.color, b.color);
        }
    }
    #[test]
    fn all_tree_and_ground_cover_recipes_are_finite() {
        for kind in [
            PropKind::Pine,
            PropKind::Fir,
            PropKind::Broadleaf,
            PropKind::Birch,
            PropKind::Willow,
            PropKind::DeadTree,
            PropKind::FallenLog,
            PropKind::Stump,
        ] {
            let p = Prop {
                position: [0., 5., 0.],
                scale: 1.1,
                canopy: 1.2,
                seed: 7123,
                kind,
                biome: Biome::Forest,
            };
            let mut m = MeshData::default();
            match kind {
                PropKind::Pine => pine(&mut m, p),
                PropKind::Fir => fir(&mut m, p),
                PropKind::Broadleaf => broadleaf(&mut m, p),
                PropKind::Birch => birch(&mut m, p),
                PropKind::Willow => willow(&mut m, p),
                PropKind::DeadTree => dead_tree(&mut m, p),
                PropKind::FallenLog => fallen_log(&mut m, p, &World::new(1337)),
                PropKind::Stump => stump(&mut m, p),
                _ => {}
            }
            assert_mesh(&m);
            assert!(m.indices.len() / 3 <= 170);
        }
        let mut m = MeshData::default();
        grass_clump(&mut m, [0., 0., 0.], 1., 2, [0.3, 0.4, 0.2], false);
        fern(&mut m, [0., 0., 0.], 1., 2, [0.3, 0.4, 0.2]);
        heather(&mut m, [0., 0., 0.], 1., 2);
        flowers(&mut m, [0., 0., 0.], 1., 2, [0.3, 0.4, 0.2]);
        assert_mesh(&m);
    }
    #[test]
    fn vegetation_triangle_budgets_are_bounded() {
        let w = World::new(1337);
        for (cx, cz) in [(-1, 29), (0, 30), (10, 10), (-20, 40), (100, -100)] {
            let near = props_chunk(&w, cx, cz);
            let far = distant_props_chunk(&w, cx, cz);
            assert_mesh(&near);
            assert_mesh(&far);
            let grass = near.vertices.iter().filter(|v| v.material == 6.0).count() / 3;
            assert!(grass <= 8 * 4096, "ground-cover budget exceeded: {}", grass);
            assert!(
                far.indices.len() / 3 <= 2560,
                "distant trees exceeded budget"
            );
            assert!(
                near.indices.len() / 3 < 56000,
                "near geometry exceeded budget"
            );
            assert!(far.vertices.iter().all(|v| v.material == 1.0));
        }
    }
    #[test]
    fn cover_can_be_streamed_separately_and_blade_bases_follow_terrain() {
        let w = World::new(42);
        let ox = -3840.0;
        let oz = 7680.0;
        let mut grid = Vec::new();
        for z in 0..=32 {
            for x in 0..=32 {
                grid.push(ground_vertex(&w, ox + x as f32 * 6., oz + z as f32 * 6.));
            }
        }
        let mut mesh = MeshData::default();
        for i in 0..80 {
            let x = ox + 0.5 + random(i, 301) * 190.0;
            let z = oz + 0.5 + random(i, 302) * 190.0;
            let y = terrain_surface_height(&w, x, z);
            let first = mesh.vertices.len();
            grass_clump(&mut mesh, [x, y - 0.018, z], 1.4, i, [0.3, 0.4, 0.2], false);
            drape_cover(&w, &mut mesh, first, ox, oz, &grid, y);
            for tri in mesh.vertices[first..].chunks_exact(3) {
                for v in &tri[..2] {
                    let ground = terrain_surface_height(&w, v.position[0], v.position[2]);
                    assert!(
                        (v.position[1] - ground + 0.018).abs() < 0.001,
                        "grass base missed terrain: {} at {:?}, ground{}",
                        v.position[1] - ground + 0.018,
                        v.position,
                        ground
                    );
                }
            }
        }
        assert_mesh(&mesh);
        assert!(props_chunk_with_cover(&w, -20, 40, false)
            .vertices
            .iter()
            .all(|v| v.material != 6.0));
    }
    #[test]
    fn landmark_recipes_are_finite() {
        for kind in [
            "ruin",
            "watchtower",
            "standing_stones",
            "camp",
            "shrine",
            "waystone",
        ] {
            let mut m = MeshData::default();
            landmark_mesh(&mut m, [0., 5., 0.], 412, kind);
            assert_mesh(&m);
        }
    }
}
