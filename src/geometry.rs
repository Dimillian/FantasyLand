//! Asset-free, deterministic low-poly geometry for the streamed world.
//! World positions are retained here; the renderer performs the camera-relative transform.
use crate::ecology;
use crate::world::{hash, rand01, Biome, RoadKind, World};
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
    // Explicit draped ribbons carry road color. Tinting coarse terrain vertices
    // inflated narrow paths into large triangular patches.
    let color = wet;
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
            // Blue stores ocean depth (+1); zero retains the river material.
            v.color[0] = flow[0];
            v.color[1] = flow[1];
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
            polygon.push((a, da));
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            polygon.push((
                [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ],
                0.0,
            ));
        }
    }
    for i in 1..polygon.len().saturating_sub(1) {
        let start = mesh.vertices.len() as u32;
        for (position, depth) in [polygon[0], polygon[i], polygon[i + 1]] {
            mesh.vertices.push(Vertex {
                position,
                normal: [0.0, 1.0, 0.0],
                color: [
                    0.0,
                    0.0,
                    if position[1] <= crate::world::SEA_LEVEL + 0.01 {
                        depth.max(0.0) + 1.0
                    } else {
                        0.0
                    },
                ],
                material: 4.0,
            });
        }
        mesh.indices.extend([start, start + 1, start + 2]);
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
// Shared mitered cross-sections close the wedges between curved route segments.
fn road_cross_section(points: &[[f32; 2]], index: usize) -> [f32; 2] {
    let normal = |a: [f32; 2], b: [f32; 2]| {
        let dx = b[0] - a[0];
        let dz = b[1] - a[1];
        let len = dx.hypot(dz).max(0.001);
        [dz / len, -dx / len]
    };
    if index == 0 {
        return normal(points[0], points[1]);
    }
    if index + 1 == points.len() {
        return normal(points[index - 1], points[index]);
    }
    let a = normal(points[index - 1], points[index]);
    let b = normal(points[index], points[index + 1]);
    let divisor = (1.0 + a[0] * b[0] + a[1] * b[1]).max(0.625);
    [(a[0] + b[0]) / divisor, (a[1] + b[1]) / divisor]
}

fn bridge_half_width(kind: RoadKind) -> f32 {
    kind.half_width() + 0.2
}

fn road_ribbons(
    world: &World,
    mesh: &mut MeshData,
    ox: f32,
    oz: f32,
    divisions: usize,
    grid: &[GroundVertex],
) {
    let step = CHUNK_SIZE / divisions as f32;
    for road in world.road_routes_near(ox + CHUNK_SIZE / 2.0, oz + CHUNK_SIZE / 2.0, CHUNK_SIZE) {
        let width = road.kind.half_width();
        let verge = width + road.kind.shoulder_width();
        let margin = verge * 2.0 + 3.0;
        for (segment_index, segment) in road.points.windows(2).enumerate() {
            let section_a = road_cross_section(&road.points, segment_index);
            let section_b = road_cross_section(&road.points, segment_index + 1);
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
                // Include strips whose center is outside but whose shoulders
                // reach this chunk. Actual ownership comes from triangle clipping.
                if mx < ox - margin
                    || mx > ox + CHUNK_SIZE + margin
                    || mz < oz - margin
                    || mz > oz + CHUNK_SIZE + margin
                {
                    continue;
                }
                let sample = world.sample(mx, mz);
                if sample.water_height > sample.height {
                    continue;
                }
                let ground = ecology::ground_color(world.seed, mx, mz, &sample);
                let base_color = match road.kind {
                    RoadKind::Main => [0.56, 0.44, 0.28],
                    RoadKind::Lane => [0.47, 0.39, 0.25],
                    RoadKind::Trail => mix(ground, [0.44, 0.36, 0.23], 0.76),
                };
                let path_color = mul(
                    base_color,
                    0.96 + rand01(hash(world.seed ^ 0x524f4144, mx as i32, mz as i32)) * 0.08,
                );
                let start = [a[0] + dx * t0, a[1] + dz * t0];
                let end = [a[0] + dx * t1, a[1] + dz * t1];
                for (left, right, color) in [
                    (-verge, -width, mix(ground, path_color, 0.35)),
                    (-width, width, path_color),
                    (width, verge, mix(ground, path_color, 0.35)),
                ] {
                    // Work in chunk-local XZ so clipping stays accurate far
                    // from the origin. A road quad may cross multiple planes.
                    let footprint = [
                        (start, t0, left),
                        (end, t1, left),
                        (end, t1, right),
                        (start, t0, right),
                    ]
                    .map(|(p, t, side)| {
                        let section = [
                            section_a[0] + (section_b[0] - section_a[0]) * t,
                            section_a[1] + (section_b[1] - section_a[1]) * t,
                        ];
                        [p[0] + section[0] * side - ox, p[1] + section[1] * side - oz]
                    });
                    let min_x = footprint.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
                    let max_x = footprint
                        .iter()
                        .map(|p| p[0])
                        .fold(f32::NEG_INFINITY, f32::max);
                    let min_z = footprint.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
                    let max_z = footprint
                        .iter()
                        .map(|p| p[1])
                        .fold(f32::NEG_INFINITY, f32::max);
                    if max_x <= 0.0 || min_x >= CHUNK_SIZE || max_z <= 0.0 || min_z >= CHUNK_SIZE {
                        continue;
                    }
                    let cell = |v: f32| {
                        ((v / step).floor() as i32).clamp(0, divisions as i32 - 1) as usize
                    };
                    for iz in cell(min_z)..=cell(max_z) {
                        for ix in cell(min_x)..=cell(max_x) {
                            let at = |x: usize, z: usize| {
                                let p = grid[z * (divisions + 1) + x].position;
                                [p[0] - ox, p[1], p[2] - oz]
                            };
                            let a = at(ix, iz);
                            let b = at(ix, iz + 1);
                            let c = at(ix + 1, iz + 1);
                            let d = at(ix + 1, iz);
                            let diagonal = hash(
                                world.seed,
                                (ox / step) as i32 + ix as i32,
                                (oz / step) as i32 + iz as i32,
                            );
                            let triangles = if diagonal & 1 == 0 {
                                [[a, b, d], [b, c, d]]
                            } else {
                                [[a, b, c], [a, c, d]]
                            };
                            for triangle in triangles {
                                road_on_triangle(mesh, footprint, triangle, ox, oz, color);
                            }
                        }
                    }
                }
            }
        }
    }
}

// Sutherland-Hodgman intersection of a convex road quad with one clockwise
// terrain triangle. Four vertices clipped by three edges produce at most seven
// vertices; fixed buffers avoid per-piece heap allocation.
fn road_clip_triangle(quad: [[f32; 2]; 4], triangle: [[f32; 3]; 3]) -> ([[f32; 2]; 8], usize) {
    let mut polygon = [[0.0; 2]; 8];
    polygon[..4].copy_from_slice(&quad);
    let mut count = 4;
    for edge in 0..3 {
        if count < 3 {
            break;
        }
        let a = triangle[edge];
        let b = triangle[(edge + 1) % 3];
        let signed_distance =
            |p: [f32; 2]| (b[0] - a[0]) * (p[1] - a[2]) - (b[2] - a[2]) * (p[0] - a[0]);
        let mut output = [[0.0; 2]; 8];
        let mut output_count = 0;
        let push = |output: &mut [[f32; 2]; 8], count: &mut usize, p: [f32; 2]| {
            if *count == 0
                || (output[*count - 1][0] - p[0]).abs() + (output[*count - 1][1] - p[1]).abs()
                    > 0.000001
            {
                output[*count] = p;
                *count += 1;
            }
        };
        let mut previous = polygon[count - 1];
        let mut previous_distance = signed_distance(previous);
        for current in polygon[..count].iter().copied() {
            let current_distance = signed_distance(current);
            if (current_distance <= 0.0) != (previous_distance <= 0.0) {
                let t =
                    (previous_distance / (previous_distance - current_distance)).clamp(0.0, 1.0);
                push(
                    &mut output,
                    &mut output_count,
                    [
                        previous[0] + (current[0] - previous[0]) * t,
                        previous[1] + (current[1] - previous[1]) * t,
                    ],
                );
            }
            if current_distance <= 0.0 {
                push(&mut output, &mut output_count, current);
            }
            previous = current;
            previous_distance = current_distance;
        }
        if output_count > 1
            && (output[0][0] - output[output_count - 1][0]).abs()
                + (output[0][1] - output[output_count - 1][1]).abs()
                < 0.000001
        {
            output_count -= 1;
        }
        polygon = output;
        count = output_count;
    }
    (polygon, count)
}

fn road_on_triangle(
    mesh: &mut MeshData,
    quad: [[f32; 2]; 4],
    triangle: [[f32; 3]; 3],
    ox: f32,
    oz: f32,
    color: [f32; 3],
) {
    let (polygon, count) = road_clip_triangle(quad, triangle);
    if count < 3 {
        return;
    }
    let [a, b, c] = triangle;
    let determinant = (b[0] - a[0]) * (c[2] - a[2]) - (b[2] - a[2]) * (c[0] - a[0]);
    let drape = |p: [f32; 2]| {
        let u = ((p[0] - a[0]) * (c[2] - a[2]) - (p[1] - a[2]) * (c[0] - a[0])) / determinant;
        let v = ((b[0] - a[0]) * (p[1] - a[2]) - (b[2] - a[2]) * (p[0] - a[0])) / determinant;
        [
            ox + p[0],
            a[1] + u * (b[1] - a[1]) + v * (c[1] - a[1]) + 0.025,
            oz + p[1],
        ]
    };
    let first = drape(polygon[0]);
    for i in 1..count - 1 {
        mesh.triangle(first, drape(polygon[i]), drape(polygon[i + 1]), color, 0.0);
    }
}

// Physics uses the same six-meter triangles as the finest streamed terrain.
// Sampling the analytic generator directly would let feet sink into a curved ridge.
fn terrain_surface_height(world: &World, x: f32, z: f32) -> f32 {
    terrain_surface_height_lod(world, x, z, 0)
}
/// Exact triangle interpolation used by the terrain mesh at this detail level.
pub fn terrain_surface_height_lod(world: &World, x: f32, z: f32, lod: u32) -> f32 {
    let step = 6.0 * (1u32 << lod.min(4)) as f32;
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
/// as the visible mesh, including bank approaches and each road class's deck width.
pub fn walk_height(world: &World, x: f32, z: f32) -> f32 {
    let mut height = terrain_surface_height(world, x, z);
    for road in world.road_routes_near(x, z, 6.0) {
        for segment in road.points.windows(2) {
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
            if across.abs() > bridge_half_width(road.kind) || along < -0.05 || along > len + 0.05 {
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
    if sample.ocean || sample.road > 0.07 || sample.water_height > sample.height + 0.8 {
        return None;
    }
    let density = ecology::tree_density(world.seed, x, z, &sample);
    let species = random(seed, 6);
    let detail = random(seed, 8);
    let kind = if sample.shore != crate::world::ShoreKind::None {
        if pick < 0.035 {
            PropKind::Boulder
        } else {
            return None;
        }
    } else if sample.river > 0.28 || sample.water_height > sample.height - 0.8 {
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
    props_chunk_at_lod(world, cx, cz, cover, 0)
}
pub fn props_chunk_at_lod(world: &World, cx: i32, cz: i32, cover: bool, lod: u32) -> MeshData {
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
            if let Some(mut p) = prop_at(world, x, z) {
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
                p.position[1] =
                    terrain_surface_height_lod(world, p.position[0], p.position[2], lod) - 0.08;
                let first = mesh.vertices.len();
                match p.kind {
                    PropKind::Pine => pine(&mut mesh, p),
                    PropKind::Fir => fir(&mut mesh, p),
                    PropKind::Broadleaf => broadleaf(&mut mesh, p),
                    PropKind::Birch => birch(&mut mesh, p),
                    PropKind::Willow => willow(&mut mesh, p),
                    PropKind::DeadTree => dead_tree(&mut mesh, p),
                    PropKind::FallenLog => fallen_log_lod(&mut mesh, p, world, lod),
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
                anchor_prop(world, &mut mesh, first, p, lod);
            }
        }
    }
    for site in sites {
        if owns(ox, oz, site.x, site.z) {
            let seed = hash(world.seed, site.x as i32, site.z as i32);
            settlement_marker(
                world,
                &mut mesh,
                [site.x, 0.0, site.z],
                seed,
                &site.kind,
                lod,
            );
        }
    }
    for landmark in landmarks {
        if owns(ox, oz, landmark.x, landmark.z) {
            let seed = hash(world.seed ^ 7123, landmark.x as i32, landmark.z as i32);
            landmark_mesh(
                world,
                &mut mesh,
                [landmark.x, 0.0, landmark.z],
                seed,
                &landmark.kind,
                lod,
            );
        }
    }
    if cover {
        ground_cover_lod(world, &mut mesh, ox, oz, lod);
    }
    bridges(world, &mut mesh, ox, oz, lod);
    mesh
}
/// Cheap tree-only silhouettes for the outer streaming ring. Candidate positions,
/// species and clearing exclusions match nearby geometry; every second tree is
/// retained to keep distant forests within the rendering budget.
pub fn distant_props_chunk(world: &World, cx: i32, cz: i32) -> MeshData {
    distant_props_chunk_at_lod(world, cx, cz, 0)
}
pub fn distant_props_chunk_at_lod(world: &World, cx: i32, cz: i32, lod: u32) -> MeshData {
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
            let Some(mut p) = prop_at(world, x, z) else {
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
            p.position[1] =
                terrain_surface_height_lod(world, p.position[0], p.position[2], lod) - 0.08;
            let [x, y, z] = p.position;
            let first = mesh.vertices.len();
            let (height, radius) = match p.kind {
                PropKind::Pine => ((10.0 + random(p.seed, 9) * 7.0) * 0.7, 0.38),
                PropKind::Fir => ((15.0 + random(p.seed, 241) * 9.0) * 0.9, 0.37),
                PropKind::Broadleaf => ((6.5 + random(p.seed, 15) * 4.0) * 0.86, 0.64),
                PropKind::Birch => ((12.0 + random(p.seed, 261) * 7.0) * 0.73, 0.25),
                PropKind::Willow => ((8.0 + random(p.seed, 271) * 4.0) * 0.75, 0.65),
                _ => (0.0, 0.0),
            };
            if height > 0.0 {
                trunk(
                    &mut mesh,
                    p.position,
                    radius * p.scale,
                    height * p.scale,
                    0.0,
                    [0.31, 0.25, 0.18],
                );
            }
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
            anchor_prop(world, &mut mesh, first, p, lod);
        }
    }
    mesh
}

fn anchor_prop(world: &World, mesh: &mut MeshData, first: usize, p: Prop, lod: u32) {
    let drape = matches!(p.kind, PropKind::Boulder | PropKind::Shrub | PropKind::Reed);
    if matches!(p.kind, PropKind::FallenLog) {
        return;
    }
    let origin = terrain_surface_height_lod(world, p.position[0], p.position[2], lod);
    let mut heights = std::collections::HashMap::new();
    for vertex in &mut mesh.vertices[first..] {
        let relative = vertex.position[1] - p.position[1];
        let root_height = if matches!(p.kind, PropKind::DeadTree | PropKind::Birch) {
            p.scale * 0.20
        } else {
            0.01
        };
        let root = relative <= root_height;
        if drape || root {
            let x = vertex.position[0];
            let z = vertex.position[2];
            let ground = *heights
                .entry((x.to_bits(), z.to_bits(), root))
                .or_insert_with(|| {
                    let surface = terrain_surface_height_lod(world, x, z, lod);
                    // At chunk boundaries the neighboring terrain can be another LOD.
                    // Buried root extensions cover that transition without moving crowns.
                    if root
                        && ((x / CHUNK_SIZE).floor() != (p.position[0] / CHUNK_SIZE).floor()
                            || (z / CHUNK_SIZE).floor() != (p.position[2] / CHUNK_SIZE).floor()
                            || x.rem_euclid(CHUNK_SIZE)
                                .min(CHUNK_SIZE - x.rem_euclid(CHUNK_SIZE))
                                < 2.0
                            || z.rem_euclid(CHUNK_SIZE)
                                .min(CHUNK_SIZE - z.rem_euclid(CHUNK_SIZE))
                                < 2.0)
                    {
                        (0..=3)
                            .map(|level| terrain_surface_height_lod(world, x, z, level))
                            .fold(surface, f32::min)
                    } else {
                        surface
                    }
                });
            if drape {
                vertex.position[1] += ground - origin;
            } else {
                vertex.position[1] = vertex.position[1].min(ground - 0.15);
            }
            if matches!(p.kind, PropKind::Reed) {
                vertex.material = 1.4 - (relative / (p.scale * 1.5)).clamp(0.0, 1.0) * 0.4;
            }
        }
    }
    for triangle in mesh.vertices[first..].chunks_exact_mut(3) {
        let u = sub(triangle[1].position, triangle[0].position);
        let v = sub(triangle[2].position, triangle[0].position);
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length > 0.00001 {
            for vertex in triangle {
                vertex.normal = n.map(|v| v / length);
            }
        }
    }
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
#[cfg(test)]
fn fallen_log(mesh: &mut MeshData, p: Prop, world: &World) {
    fallen_log_lod(mesh, p, world, 0)
}
fn fallen_log_lod(mesh: &mut MeshData, p: Prop, world: &World, lod: u32) {
    let yaw = random(p.seed, 291) * TAU;
    let len = (3.0 + random(p.seed, 292) * 3.0) * p.scale;
    let r = 0.36 * p.scale;
    let [x, y, z] = p.position;
    let ex = x + yaw.sin() * len;
    let ez = z + yaw.cos() * len;
    let end = [
        ex,
        terrain_surface_height_lod(world, ex, ez, lod) + r * 0.70 - 0.10,
        ez,
    ];
    let first = mesh.vertices.len();
    let start = [x, y + r * 0.85, z];
    branch(mesh, start, end, r, r * 0.82, [0.31, 0.25, 0.17]);
    let body_end = mesh.vertices.len();
    let cap_center = [x, y + r, z];
    polyhedron(
        mesh,
        cap_center,
        [r * 1.12, r * 1.12, r * 1.12],
        p.seed,
        [0.47, 0.37, 0.23],
        3.0,
    );
    // A tilted circular end has less vertical radius than a horizontal one.
    // Anchor its actual lower ring, preserving the upper ring and central axis.
    // The split/cut end follows the same rule so it cannot detach on a slope.
    let owner = [
        (x / CHUNK_SIZE).floor() as i32,
        (z / CHUNK_SIZE).floor() as i32,
    ];
    let axis = sub(end, start);
    let half_length_squared = axis.iter().map(|v| v * v).sum::<f32>() * 0.5;
    let mut heights = std::collections::HashMap::new();
    for (index, vertex) in mesh.vertices[first..].iter_mut().enumerate() {
        let relative = sub(vertex.position, start);
        let along = relative
            .iter()
            .zip(axis.iter())
            .map(|(a, b)| a * b)
            .sum::<f32>();
        let center = if first + index >= body_end {
            cap_center
        } else if along < half_length_squared {
            start
        } else {
            end
        };
        if vertex.position[1] > center[1] + 0.0001 {
            continue;
        }
        let vx = vertex.position[0];
        let vz = vertex.position[2];
        let ground = *heights
            .entry((vx.to_bits(), vz.to_bits()))
            .or_insert_with(|| {
                let mut ground = terrain_surface_height_lod(world, vx, vz, lod);
                let cell = [
                    (vx / CHUNK_SIZE).floor() as i32,
                    (vz / CHUNK_SIZE).floor() as i32,
                ];
                let edge_x = vx.rem_euclid(CHUNK_SIZE);
                let edge_z = vz.rem_euclid(CHUNK_SIZE);
                // A long log may extend several meters into its neighboring chunk,
                // beyond the narrow edge margin used for upright tree roots.
                if cell != owner
                    || edge_x.min(CHUNK_SIZE - edge_x) < 2.0
                    || edge_z.min(CHUNK_SIZE - edge_z) < 2.0
                {
                    for level in 0..=3 {
                        ground = ground.min(terrain_surface_height_lod(world, vx, vz, level));
                    }
                }
                ground
            });
        vertex.position[1] = vertex.position[1].min(ground - 0.15);
    }
    for triangle in mesh.vertices[first..].chunks_exact_mut(3) {
        let a = sub(triangle[1].position, triangle[0].position);
        let b = sub(triangle[2].position, triangle[0].position);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
        if length > 0.00001 {
            for vertex in triangle {
                vertex.normal = n.map(|v| v / length);
            }
        }
    }
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
fn ground_cover_lod(world: &World, mesh: &mut MeshData, ox: f32, oz: f32, lod: u32) {
    let divisions = 64;
    let spacing = 3.0;
    let terrain_divisions = (32u32 >> lod.min(4)).max(2) as usize;
    let terrain_step = CHUNK_SIZE / terrain_divisions as f32;
    let mut grid = Vec::with_capacity((terrain_divisions + 1).pow(2));
    for z in 0..=terrain_divisions {
        for x in 0..=terrain_divisions {
            grid.push(ground_vertex(
                world,
                ox + x as f32 * terrain_step,
                oz + z as f32 * terrain_step,
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
            let y = grid_height(world, ox, oz, terrain_divisions, &grid, x, z) - 0.018;
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
            drape_cover_lod(world, mesh, first_vertex, ox, oz, &grid, y + 0.018, lod);
            // Infrequent palm-sized stones interrupt the vegetation without adding a
            // second dense field or collision obstacles around the player's feet.
            if kind > 0.974 {
                let rx = x + 0.55;
                let rz = z - 0.25;
                let ry = terrain_surface_height_lod(world, rx, rz, lod);
                let first_stone = mesh.vertices.len();
                polyhedron(
                    mesh,
                    [rx, ry + 0.07, rz],
                    [0.34, 0.22, 0.27],
                    seed,
                    [0.44, 0.43, 0.35],
                    2.0,
                );
                anchor_prop(
                    world,
                    mesh,
                    first_stone,
                    Prop {
                        position: [rx, ry, rz],
                        scale: 1.0,
                        canopy: 1.0,
                        seed,
                        kind: PropKind::Boulder,
                        biome: sample.biome,
                    },
                    lod,
                );
            }
        }
    }
}
// Each blade endpoint roots to its own position on the rendered triangle. Using
// one shared height for a whole clump caused floating blades on steep slopes.
#[cfg(test)]
fn drape_cover(
    world: &World,
    mesh: &mut MeshData,
    first: usize,
    ox: f32,
    oz: f32,
    grid: &[GroundVertex],
    center_height: f32,
) {
    drape_cover_lod(world, mesh, first, ox, oz, grid, center_height, 0)
}
fn drape_cover_lod(
    world: &World,
    mesh: &mut MeshData,
    first: usize,
    ox: f32,
    oz: f32,
    grid: &[GroundVertex],
    center_height: f32,
    lod: u32,
) {
    for triangle in mesh.vertices[first..].chunks_exact_mut(3) {
        for v in triangle.iter_mut() {
            let x = v.position[0];
            let z = v.position[2];
            let mut h = if x >= ox && x <= ox + CHUNK_SIZE && z >= oz && z <= oz + CHUNK_SIZE {
                grid_height(
                    world,
                    ox,
                    oz,
                    (32u32 >> lod.min(4)).max(2) as usize,
                    grid,
                    x,
                    z,
                )
            } else {
                terrain_surface_height_lod(world, x, z, lod)
            };
            let height_above_ground = v.position[1] - center_height;
            if height_above_ground <= 0.0 && !owns(ox, oz, x, z) {
                for level in 0..=3 {
                    h = h.min(terrain_surface_height_lod(world, x, z, level));
                }
            }
            v.position[1] += h - center_height;
            // Material6 fraction carries wind weight; roots are exactly zero.
            v.material = 6.0 + (height_above_ground / 0.65).clamp(0.0, 1.0) * 0.4;
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
// Structural heights follow the displayed terrain. Footings extend through all
// nearby terrain LODs so a structure straddling a chunk boundary cannot float.
fn structural_ground_range(
    world: &World,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    lod: u32,
) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    let nx = (size[0] / 0.5).ceil().max(1.0) as u32;
    let nz = (size[1] / 0.5).ceil().max(1.0) as u32;
    for iz in 0..=nz {
        for ix in 0..=nx {
            let dx = (ix as f32 / nx as f32 - 0.5) * size[0];
            let dz = (iz as f32 / nz as f32 - 0.5) * size[1];
            let x = center[0] + dx * yaw.cos() + dz * yaw.sin();
            let z = center[1] - dx * yaw.sin() + dz * yaw.cos();
            let ground = terrain_surface_height_lod(world, x, z, lod);
            low = low.min(ground);
            high = high.max(ground);
        }
    }
    (low, high)
}
fn structural_footing_bottom(
    world: &World,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    lod: u32,
) -> f32 {
    let mut bottom = structural_ground_range(world, center, size, yaw, lod).0;
    for level in 0..=3 {
        bottom = bottom.min(structural_ground_range(world, center, size, yaw, level).0);
    }
    bottom - 0.25
}
fn structural_support(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    top: f32,
    color: [f32; 3],
    material: f32,
    lod: u32,
) {
    let bottom = structural_footing_bottom(world, center, size, yaw, lod);
    box_mesh(
        mesh,
        [center[0], (bottom + top) * 0.5, center[1]],
        [size[0], (top - bottom).max(0.02), size[1]],
        yaw,
        color,
        material,
    );
}
fn structural_foundation(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    size: [f32; 2],
    yaw: f32,
    rise: f32,
    color: [f32; 3],
    lod: u32,
) -> f32 {
    let top = structural_ground_range(world, center, size, yaw, lod).1 + rise;
    structural_support(world, mesh, center, size, yaw, top, color, 2.0, lod);
    top
}
fn grounded_banner(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    height: f32,
    seed: u32,
    lod: u32,
) {
    let ground = structural_ground_range(world, center, [0.34, 0.34], 0.0, lod).1;
    let bottom = structural_footing_bottom(world, center, [0.34, 0.34], 0.0, lod);
    banner(
        mesh,
        [center[0], bottom, center[1]],
        ground + height - bottom,
        seed,
    );
}
fn structural_stone(
    world: &World,
    mesh: &mut MeshData,
    center: [f32; 2],
    scale: f32,
    seed: u32,
    lod: u32,
) {
    let size = scale * (1.2 + random(seed, 90) * 1.4);
    let y = structural_ground_range(world, center, [size * 2.4, size * 2.1], 0.0, lod).1;
    // The short buried core supports the faceted lower shell at steep or mixed
    // LOD edges; the visible silhouette stays the ordinary generated stone.
    structural_support(
        world,
        mesh,
        center,
        [size * 0.85, size * 0.7],
        0.0,
        y + size * 0.43,
        [0.43, 0.44, 0.40],
        2.0,
        lod,
    );
    rock(mesh, [center[0], y - size * 0.10, center[1]], scale, seed);
}
fn settlement_marker(
    world: &World,
    mesh: &mut MeshData,
    base: [f32; 3],
    seed: u32,
    kind: &str,
    lod: u32,
) {
    let [x, _, z] = base;
    let y = structural_ground_range(world, [x, z], [7.1, 7.1], 0.0, lod).1 + 0.12;
    let height = if kind == "town" { 12.0 } else { 8.0 };
    let wood = [0.35, 0.27, 0.18];
    for dx in [-2.8, 2.8] {
        for dz in [-2.8, 2.8] {
            structural_support(
                world,
                mesh,
                [x + dx, z + dz],
                [0.6, 0.6],
                0.0,
                y + height + 0.2,
                wood,
                3.0,
                lod,
            );
        }
    }
    let deck = y + height - 2.0;
    box_mesh(mesh, [x, deck, z], [7.1, 0.45, 7.1], 0.0, wood, 3.0);
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
    grounded_banner(world, mesh, [x + 3.4, z + 3.4], height + 5.0, seed, lod);
    let ladder_ground = structural_ground_range(world, [x - 2.65, z + 3.5], [1.2, 0.3], 0.0, lod).1;
    for dx in [-3.17, -2.12] {
        structural_support(
            world,
            mesh,
            [x + dx, z + 3.5],
            [0.15, 0.15],
            0.0,
            deck + 0.8,
            wood,
            3.0,
            lod,
        );
    }
    let rungs = ((deck - ladder_ground) / 0.7).ceil().max(1.0) as u32;
    for j in 0..rungs {
        let h = ladder_ground + (deck - ladder_ground) * (j + 1) as f32 / rungs as f32;
        box_mesh(
            mesh,
            [x - 2.65, h, z + 3.5],
            [1.15, 0.12, 0.28],
            0.0,
            wood,
            3.0,
        );
    }
    signpost(world, mesh, [x + 8.0, 0.0, z + 5.0], seed, lod);
    camp(world, mesh, [x - 10.0, 0.0, z + 8.0], seed ^ 31, lod);
}
fn signpost(world: &World, mesh: &mut MeshData, base: [f32; 3], seed: u32, lod: u32) {
    let [x, _, z] = base;
    let yaw = random(seed, 141) * 0.5;
    let y = structural_ground_range(world, [x, z], [0.34, 0.34], yaw, lod).1;
    structural_support(
        world,
        mesh,
        [x, z],
        [0.34, 0.34],
        yaw,
        y + 5.2,
        [0.35, 0.25, 0.15],
        3.0,
        lod,
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
    box_mesh(
        mesh,
        [
            x + 0.92 * yaw.cos() + 0.13 * yaw.sin(),
            y + 4.25,
            z - 0.92 * yaw.sin() + 0.13 * yaw.cos(),
        ],
        [0.75, 0.12, 0.06],
        yaw,
        [0.18, 0.17, 0.12],
        3.0,
    );
}
fn landmark_mesh(
    world: &World,
    mesh: &mut MeshData,
    base: [f32; 3],
    seed: u32,
    kind: &str,
    lod: u32,
) {
    let [x, _, z] = base;
    match kind {
        "watchtower" => {
            let stone = [0.47, 0.46, 0.39];
            let y = structural_ground_range(world, [x, z], [7.3, 7.3], 0.0, lod).1 + 0.12;
            for dx in [-2.7, 2.7] {
                for dz in [-2.7, 2.7] {
                    structural_support(
                        world,
                        mesh,
                        [x + dx, z + dz],
                        [1.5, 1.5],
                        0.0,
                        y + 18.0,
                        stone,
                        2.0,
                        lod,
                    );
                }
            }
            for (center, size) in [
                ([x, z - 2.7], [4.0, 1.5]),
                ([x - 2.7, z], [1.5, 4.0]),
                ([x + 2.7, z], [1.5, 4.0]),
            ] {
                structural_support(world, mesh, center, size, 0.0, y + 14.0, stone, 2.0, lod);
            }
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
            grounded_banner(world, mesh, [x + 4.0, z + 4.0], 9.0, seed, lod);
        }
        "ruin" => {
            let stone = [0.48, 0.46, 0.39];
            for i in 1..7 {
                let a = i as f32 * TAU / 8.0;
                let center = [x + a.sin() * 6.5, z + a.cos() * 6.5];
                let h = 3.0 + random(seed, 150 + i) * 7.0;
                let y = structural_ground_range(world, center, [2.5, 1.6], a, lod).1;
                structural_support(world, mesh, center, [2.5, 1.6], a, y + h, stone, 2.0, lod);
            }
            let arch = structural_ground_range(world, [x, z + 6.5], [7.0, 1.6], 0.0, lod).1 + 6.0;
            for dx in [-2.6, 2.6] {
                structural_support(
                    world,
                    mesh,
                    [x + dx, z + 6.5],
                    [1.6, 1.6],
                    0.0,
                    arch,
                    stone,
                    2.0,
                    lod,
                );
            }
            box_mesh(
                mesh,
                [x, arch + 0.7, z + 6.5],
                [7.0, 1.4, 1.6],
                0.0,
                stone,
                2.0,
            );
            for i in 0..5 {
                let a = random(seed, 165 + i) * TAU;
                structural_stone(
                    world,
                    mesh,
                    [x + a.sin() * 9.0, z + a.cos() * 9.0],
                    0.8,
                    seed ^ i,
                    lod,
                );
            }
        }
        "standing_stones" => {
            for i in 0..7 {
                let a = i as f32 * TAU / 7.0;
                let h = 4.0 + random(seed, 180 + i) * 3.5;
                let center = [x + a.sin() * 7.2, z + a.cos() * 7.2];
                let y = structural_ground_range(world, center, [3.2, 2.4], 0.0, lod).1;
                structural_support(
                    world,
                    mesh,
                    center,
                    [1.3, 1.0],
                    0.0,
                    y + h * 0.3,
                    [0.43, 0.45, 0.43],
                    2.0,
                    lod,
                );
                polyhedron(
                    mesh,
                    [center[0], y + h * 0.43, center[1]],
                    [1.6, h * 0.85, 1.2],
                    seed ^ i,
                    [0.43, 0.45, 0.43],
                    2.0,
                );
            }
            structural_foundation(
                world,
                mesh,
                [x, z],
                [3.5, 2.3],
                random(seed, 188),
                1.0,
                [0.46, 0.45, 0.41],
                lod,
            );
        }
        "camp" => camp(world, mesh, base, seed, lod),
        "shrine" => {
            let stone = [0.53, 0.51, 0.43];
            let slab = structural_foundation(world, mesh, [x, z], [6.0, 6.0], 0.0, 0.7, stone, lod);
            box_mesh(mesh, [x, slab + 0.25, z], [4.5, 0.5, 4.5], 0.0, stone, 2.0);
            for dx in [-1.8, 1.8] {
                for dz in [-1.8, 1.8] {
                    box_mesh(
                        mesh,
                        [x + dx, slab + 2.4, z + dz],
                        [0.5, 4.0, 0.5],
                        0.0,
                        stone,
                        2.0,
                    );
                }
            }
            cone(
                mesh,
                [x, slab + 4.4, z],
                3.8,
                2.1,
                4,
                PI / 4.0,
                [0.32, 0.36, 0.34],
                2.0,
            );
            polyhedron(
                mesh,
                [x, slab + 1.6, z],
                [0.9, 1.35, 0.8],
                seed,
                [0.61, 0.56, 0.37],
                2.0,
            );
            grounded_banner(world, mesh, [x + 5.0, z], 6.2, seed, lod);
        }
        _ => {
            let y = structural_foundation(
                world,
                mesh,
                [x, z],
                [2.7, 2.2],
                0.0,
                0.15,
                [0.45, 0.44, 0.39],
                lod,
            );
            for i in 0..4 {
                let f = i as f32;
                polyhedron(
                    mesh,
                    [x, y + 0.6 + f, z],
                    [2.0 - f * 0.34, 1.0, 1.6 - f * 0.28],
                    seed ^ i,
                    [0.45, 0.44, 0.39],
                    2.0,
                );
            }
            signpost(world, mesh, [x + 3.0, 0.0, z], seed, lod);
        }
    }
}
fn camp(world: &World, mesh: &mut MeshData, base: [f32; 3], seed: u32, lod: u32) {
    let [x, _, z] = base;
    let y = structural_foundation(
        world,
        mesh,
        [x, z],
        [6.25, 6.25],
        0.0,
        0.10,
        [0.39, 0.35, 0.27],
        lod,
    );
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
    mesh.triangle(d, [x - 0.8, y, z + 3.0], f, mul(cloth, 0.85), 5.0);
    mesh.triangle(f, [x + 0.8, y, z + 3.0], e, mul(cloth, 0.85), 5.0);
    for dz in [-3.0, 3.0] {
        trunk(
            mesh,
            [x, y - 0.04, z + dz],
            0.09,
            3.85,
            0.0,
            [0.34, 0.25, 0.16],
        );
    }
    for dx in [-3.15, 3.15] {
        let center = [x + dx, z + 3.2];
        let ground = structural_ground_range(world, center, [0.16, 0.16], 0.0, lod).1;
        structural_support(
            world,
            mesh,
            center,
            [0.16, 0.16],
            0.0,
            ground + 1.3,
            [0.34, 0.25, 0.16],
            3.0,
            lod,
        );
    }
    let fire = structural_foundation(
        world,
        mesh,
        [x + 6.0, z],
        [4.0, 4.0],
        0.0,
        0.04,
        [0.33, 0.31, 0.25],
        lod,
    );
    for i in 0..7 {
        let a = i as f32 * TAU / 7.0;
        rock(
            mesh,
            [x + 6.0 + a.sin() * 1.2, fire, z + a.cos() * 1.2],
            0.24,
            seed ^ i,
        );
    }
    box_mesh(
        mesh,
        [x + 6.0, fire + 0.18, z],
        [1.8, 0.4, 0.38],
        0.8,
        [0.24, 0.18, 0.12],
        3.0,
    );
    box_mesh(
        mesh,
        [x + 6.0, fire + 0.34, z],
        [1.8, 0.4, 0.38],
        -0.8,
        [0.28, 0.20, 0.13],
        3.0,
    );
    cone(
        mesh,
        [x + 6.0, fire + 0.40, z],
        0.35,
        0.75,
        5,
        0.0,
        [0.76, 0.35, 0.08],
        5.0,
    );
    // A fallen seat/log follows its own local slope. Its lower endpoints are
    // embedded, and small support feet also reach the adjacent terrain LODs.
    let a = [x + 4.0 - 1.5 * 0.1_f32.cos(), z + 3.0 + 1.5 * 0.1_f32.sin()];
    let b = [x + 4.0 + 1.5 * 0.1_f32.cos(), z + 3.0 - 1.5 * 0.1_f32.sin()];
    let ya = structural_ground_range(world, a, [0.6, 0.6], 0.0, lod).0 + 0.50;
    let yb = structural_ground_range(world, b, [0.6, 0.6], 0.0, lod).0 + 0.50;
    sloped_box(mesh, a, b, ya, yb, 0.6, 0.55, [0.35, 0.26, 0.17], 3.0);
    for (center, top) in [(a, ya), (b, yb)] {
        structural_support(
            world,
            mesh,
            center,
            [0.42, 0.45],
            0.1,
            top - 0.1,
            [0.35, 0.26, 0.17],
            3.0,
            lod,
        );
    }
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
fn bridges(world: &World, mesh: &mut MeshData, ox: f32, oz: f32, lod: u32) {
    let roads = world.road_routes_near(ox + CHUNK_SIZE / 2.0, oz + CHUNK_SIZE / 2.0, CHUNK_SIZE);
    for road in roads {
        let deck_width = bridge_half_width(road.kind) * 2.0;
        let rail_offset = deck_width * 0.5 - 0.15;
        for segment in road.points.windows(2) {
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
                sloped_box(mesh, start, end, y0, y1, deck_width, 0.36, wood, 3.0);
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
                        [deck_width + 0.02, 0.025, 0.055],
                        yaw,
                        [0.31, 0.25, 0.17],
                        3.0,
                    );
                }
                for side in [-1.0, 1.0] {
                    let px = x + dz / len * rail_offset * side;
                    let pz = z - dx / len * rail_offset * side;
                    box_mesh(
                        mesh,
                        [px, y - 0.15, pz],
                        [0.28, 2.6, 0.28],
                        yaw,
                        [0.34, 0.26, 0.17],
                        3.0,
                    );
                    // Railing uprights are joined to real piers reaching the bed.
                    let floor = structural_footing_bottom(world, [px, pz], [0.38, 0.38], yaw, lod);
                    if floor < y - 0.36 {
                        box_mesh(
                            mesh,
                            [px, (floor + y - 0.30) * 0.5, pz],
                            [0.38, y - 0.30 - floor, 0.38],
                            yaw,
                            [0.28, 0.22, 0.16],
                            3.0,
                        );
                    }
                    let offset = [
                        dz / len * rail_offset * side,
                        -dx / len * rail_offset * side,
                    ];
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
    fn sea_surface_clips_to_sloping_ground_and_interpolates_surf_depth() {
        let terrain = [
            GroundVertex {
                position: [0., -4., 0.],
                water: 0.,
                color: [0.; 3],
            },
            GroundVertex {
                position: [0., 4., 8.],
                water: -10000.,
                color: [0.; 3],
            },
            GroundVertex {
                position: [8., -4., 0.],
                water: 0.,
                color: [0.; 3],
            },
        ];
        let mut mesh = MeshData::default();
        water_triangle(&mut mesh, terrain, 0.);
        assert_mesh(&mesh);
        assert_eq!(mesh.vertices.len(), 6);
        for v in &mesh.vertices {
            assert_eq!(v.position[1], 0.0);
            assert!(
                v.position[2] <= 4.013,
                "sea extends beyond the visible slope"
            );
            assert!(v.color[2] >= 1.0, "ocean depth flag missing");
        }
        assert!(
            mesh.vertices
                .iter()
                .any(|v| (v.color[2] - 1.0).abs() < 0.001),
            "surf must reach the clipped shore"
        );
        assert!(
            mesh.vertices.iter().any(|v| v.color[2] > 5.0),
            "offshore depth was lost"
        );
        let dry = terrain.map(|mut v| {
            v.position[1] += 10.;
            v
        });
        let mut dry_mesh = MeshData::default();
        water_triangle(&mut dry_mesh, dry, 0.);
        assert!(
            dry_mesh.vertices.is_empty(),
            "dry coastal triangles must not carry an ocean plane"
        );
    }

    #[test]
    fn crossing_keeps_its_upstream_deck_edge_above_water() {
        let world = World::new(1337);
        // Resolve real crossings from the current drainage network: adding sea
        // outlets moves the old fixed river coordinate onto dry land. Every
        // selected span still checks the upstream side and both walking edges.
        let mut checked = 0;
        'routes: for road in world.road_routes_near(0.0, 0.0, 12000.0) {
            let half_width = bridge_half_width(road.kind);
            for segment in road.points.windows(2) {
                let dx = segment[1][0] - segment[0][0];
                let dz = segment[1][1] - segment[0][1];
                let length = dx.hypot(dz);
                if length < 0.01 {
                    continue;
                }
                let count = (length / 5.0).ceil() as usize;
                for i in 0..count {
                    let point = |t: f32| [segment[0][0] + dx * t, segment[0][1] + dz * t];
                    let start = point(i as f32 / count as f32);
                    let end = point((i + 1) as f32 / count as f32);
                    let middle = point((i as f32 + 0.5) / count as f32);
                    let sample = world.sample(middle[0], middle[1]);
                    if sample.water_height <= sample.height {
                        continue;
                    }
                    let (y0, y1) =
                        bridge_heights(&world, start, end).expect("wet road needs a deck");
                    for side in [-0.98, 0.0, 0.98] {
                        let x = middle[0] + dz / length * half_width * side;
                        let z = middle[1] - dx / length * half_width * side;
                        let water = world.sample(x, z).water_height;
                        assert!((y0 + y1) * 0.5 > water + 1.3, "deck edge submerged");
                        assert!(
                            walk_height(&world, x, z) >= (y0 + y1) * 0.5 - 0.01,
                            "deck edge missing from walking surface"
                        );
                    }
                    checked += 1;
                    if checked >= 24 {
                        break 'routes;
                    }
                }
            }
        }
        assert!(checked >= 4, "fixture needs several actual wet road spans");
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
            let grass = near
                .vertices
                .iter()
                .filter(|v| (6.0..=6.4).contains(&v.material))
                .count()
                / 3;
            assert!(grass <= 8 * 4096, "ground-cover budget exceeded: {}", grass);
            assert!(
                far.indices.len() / 3 <= 5120,
                "distant trees exceeded budget"
            );
            assert!(
                near.indices.len() / 3 < 56000,
                "near geometry exceeded budget"
            );
            assert!(far
                .vertices
                .iter()
                .all(|v| v.material == 1.0 || v.material == 3.0));
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
                    assert_eq!(v.material, 6.0, "wind must leave each blade root fixed");
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
            landmark_mesh(&World::new(1337), &mut m, [0., 5., 0.], 412, kind, 0);
            assert_mesh(&m);
        }
    }
}

#[cfg(test)]
mod grounding_tests {
    use super::*;

    fn finite(mesh: &MeshData) {
        assert!(!mesh.indices.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
        for v in &mesh.vertices {
            assert!(v
                .position
                .iter()
                .chain(v.normal.iter())
                .all(|n| n.is_finite()));
            assert!((v.normal.iter().map(|n| n * n).sum::<f32>() - 1.0).abs() < 0.001);
        }
    }
    fn assert_foundation(world: &World, vertices: &[Vertex], lod: u32) {
        let min = (0..3)
            .map(|axis| {
                vertices
                    .iter()
                    .map(|v| v.position[axis])
                    .fold(f32::INFINITY, f32::min)
            })
            .collect::<Vec<_>>();
        let max = (0..3)
            .map(|axis| {
                vertices
                    .iter()
                    .map(|v| v.position[axis])
                    .fold(f32::NEG_INFINITY, f32::max)
            })
            .collect::<Vec<_>>();
        for ix in 0..=12 {
            for iz in 0..=12 {
                let x = min[0] + (max[0] - min[0]) * ix as f32 / 12.0;
                let z = min[2] + (max[2] - min[2]) * iz as f32 / 12.0;
                assert!(
                    max[1] >= terrain_surface_height_lod(world, x, z, lod) - 0.025,
                    "foundation top buried at {x},{z}, lod {lod}"
                );
                for level in 0..=3 {
                    assert!(
                        min[1] < terrain_surface_height_lod(world, x, z, level) - 0.15,
                        "foundation underside floats at {x},{z}, lod {level}"
                    );
                }
            }
        }
    }
    #[test]
    fn grounding_camp_floors_cover_reported_sloping_sites() {
        let world = World::new(1337);
        for [sx, sz] in [
            [-16545.926, -12303.939], // Mistfield: original downhill tent corner +5.165m.
            [-16750.584, -14133.446], // Cinderhaven: original tent +6.844m.
            [-14646.897, -12129.922], // Greyholt: original tent +7.61m.
        ] {
            for lod in 0..=3 {
                let mut mesh = MeshData::default();
                let center = [sx - 10.0, f32::NAN, sz + 8.0];
                camp(&world, &mut mesh, center, 214, lod);
                finite(&mesh);
                assert_foundation(&world, &mesh.vertices[..36], lod);
                let hearth: Vec<_> = mesh
                    .vertices
                    .iter()
                    .copied()
                    .filter(|v| v.material == 2.0 && v.color == [0.33, 0.31, 0.25])
                    .collect();
                assert_eq!(hearth.len(), 36);
                assert_foundation(&world, &hearth, lod);
                let floor = mesh.vertices[..36]
                    .iter()
                    .map(|v| v.position[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                for v in mesh
                    .vertices
                    .iter()
                    .filter(|v| v.material == 5.0 && (v.position[0] - center[0]).abs() <= 3.01)
                {
                    assert!(
                        v.position[1] >= floor - 0.005,
                        "tent cloth dropped below its supporting floor"
                    );
                }
                let fire_floor = hearth
                    .iter()
                    .map(|v| v.position[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                let firewood = mesh
                    .vertices
                    .iter()
                    .filter(|v| v.color == [0.24, 0.18, 0.12])
                    .map(|v| v.position[1])
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    (firewood - fire_floor + 0.02).abs() < 0.003,
                    "firewood must rest on its local hearth"
                );
                println!("camp ({sx},{sz}) lod{lod}: tent floor {floor:.3}, local hearth {fire_floor:.3}, {} triangles", mesh.indices.len()/3);
            }
        }
    }
    #[test]
    fn grounding_every_landmark_and_settlement_ignores_parent_center_height() {
        let world = World::new(1337);
        let mut max_triangles = 0;
        for lod in 0..=3 {
            for kind in [
                "watchtower",
                "ruin",
                "standing_stones",
                "camp",
                "shrine",
                "waystone",
            ] {
                let mut first = MeshData::default();
                let mut second = MeshData::default();
                landmark_mesh(
                    &world,
                    &mut first,
                    [-16555.926, f32::NAN, -12295.939],
                    214,
                    kind,
                    lod,
                );
                landmark_mesh(
                    &world,
                    &mut second,
                    [-16555.926, 99999.0, -12295.939],
                    214,
                    kind,
                    lod,
                );
                finite(&first);
                assert_eq!(
                    bytemuck::cast_slice::<_, u8>(&first.vertices),
                    bytemuck::cast_slice::<_, u8>(&second.vertices),
                    "{kind}: an independent piece still inherits parent y"
                );
                max_triangles = max_triangles.max(first.indices.len() / 3);
            }
            for kind in ["town", "village", "hamlet"] {
                let mut mesh = MeshData::default();
                settlement_marker(
                    &world,
                    &mut mesh,
                    [-16545.926, f32::NAN, -12303.939],
                    214,
                    kind,
                    lod,
                );
                finite(&mesh);
                // First four cuboids are the four separately grounded platform legs.
                for leg in mesh.vertices[..144].chunks_exact(36) {
                    assert_foundation(&world, leg, lod);
                }
                max_triangles = max_triangles.max(mesh.indices.len() / 3);
            }
        }
        assert!(
            max_triangles < 800,
            "structural recipe budget exceeded: {max_triangles}"
        );
        println!("All landmark/site recipes finite and bounded: maximum {max_triangles} triangles");
    }
    #[test]
    fn grounding_rotated_footings_embed_entire_footprint_across_lods() {
        let world = World::new(1337);
        for lod in 0..=3 {
            for (center, size, yaw) in [
                ([-16558.9, -12292.9], [0.34, 0.34], 0.4),
                ([-16545.9, -12303.9], [2.5, 1.6], 1.3),
                ([-16750.6, -14133.4], [6.25, 6.25], 0.0),
            ] {
                let bottom = structural_footing_bottom(&world, center, size, yaw, lod);
                for ix in 0..=10 {
                    for iz in 0..=10 {
                        let dx = (ix as f32 / 10.0 - 0.5) * size[0];
                        let dz = (iz as f32 / 10.0 - 0.5) * size[1];
                        let x = center[0] + dx * yaw.cos() + dz * yaw.sin();
                        let z = center[1] - dx * yaw.sin() + dz * yaw.cos();
                        for level in 0..=3 {
                            assert!(
                                bottom < terrain_surface_height_lod(&world, x, z, level) - 0.15
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod vegetation_grounding_tests {
    use super::*;

    #[test]
    fn rendered_height_matches_each_terrain_lod() {
        let world = World::new(1337);
        let (ox, oz) = (-16704.0, -12480.0);
        for lod in 0..=4 {
            let divisions = (32 >> lod).max(2);
            let step = CHUNK_SIZE / divisions as f32;
            let mut grid = Vec::new();
            for z in 0..=divisions {
                for x in 0..=divisions {
                    grid.push(ground_vertex(
                        &world,
                        ox + x as f32 * step,
                        oz + z as f32 * step,
                    ));
                }
            }
            for i in 0..128 {
                let x = ox + random(i, 801) * (CHUNK_SIZE - 0.1);
                let z = oz + random(i, 802) * (CHUNK_SIZE - 0.1);
                assert!(
                    (terrain_surface_height_lod(&world, x, z, lod as u32)
                        - grid_height(&world, ox, oz, divisions, &grid, x, z))
                    .abs()
                        < 0.001
                );
            }
        }
    }

    #[test]
    fn tree_roots_embed_on_slopes_without_collapsing_stumps() {
        let world = World::new(1337);
        for lod in 0..=3 {
            for (x, z) in [(-16558.9, -12292.9), (-16704.1, -12288.1)] {
                for kind in [
                    PropKind::Pine,
                    PropKind::Fir,
                    PropKind::Broadleaf,
                    PropKind::Birch,
                    PropKind::Willow,
                    PropKind::DeadTree,
                    PropKind::Stump,
                ] {
                    let p = Prop {
                        position: [x, terrain_surface_height_lod(&world, x, z, lod) - 0.08, z],
                        scale: 1.1,
                        canopy: 1.2,
                        seed: 7123,
                        kind,
                        biome: Biome::Forest,
                    };
                    let mut mesh = MeshData::default();
                    match kind {
                        PropKind::Pine => pine(&mut mesh, p),
                        PropKind::Fir => fir(&mut mesh, p),
                        PropKind::Broadleaf => broadleaf(&mut mesh, p),
                        PropKind::Birch => birch(&mut mesh, p),
                        PropKind::Willow => willow(&mut mesh, p),
                        PropKind::DeadTree => dead_tree(&mut mesh, p),
                        PropKind::Stump => stump(&mut mesh, p),
                        _ => unreachable!(),
                    }
                    let before = mesh.vertices.clone();
                    anchor_prop(&world, &mut mesh, 0, p, lod);
                    let mut roots = 0;
                    for (old, new) in before.iter().zip(&mesh.vertices) {
                        if old.position[1] - p.position[1] < 0.22 {
                            roots += 1;
                            let surface = terrain_surface_height_lod(
                                &world,
                                new.position[0],
                                new.position[2],
                                lod,
                            );
                            assert!(
                                new.position[1] <= surface - 0.14,
                                "floating {:?} root at LOD {lod}",
                                kind
                            );
                        } else {
                            assert_eq!(
                                old.position, new.position,
                                "grounding changed the crown or stump top"
                            );
                        }
                    }
                    assert!(roots >= 5);
                }
            }
        }
    }
}

#[cfg(test)]
mod log_grounding_tests {
    use super::*;
    fn unanchored_log(mesh: &mut MeshData, p: Prop, world: &World, lod: u32) {
        let yaw = random(p.seed, 291) * TAU;
        let len = (3.0 + random(p.seed, 292) * 3.0) * p.scale;
        let r = 0.36 * p.scale;
        let [x, y, z] = p.position;
        let ex = x + yaw.sin() * len;
        let ez = z + yaw.cos() * len;
        let end = [
            ex,
            terrain_surface_height_lod(world, ex, ez, lod) + r * 0.70 - 0.10,
            ez,
        ];
        branch(
            mesh,
            [x, y + r * 0.85, z],
            end,
            r,
            r * 0.82,
            [0.31, 0.25, 0.17],
        );
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
    #[test]
    fn tilted_fallen_log_anchors_lower_rings_across_lod_seam() {
        let world = World::new(1337);
        let seed = (1..100u32)
            .find(|&seed| (random(seed, 291) * TAU).sin() > 0.9)
            .unwrap();
        let x = -16512.15;
        let z = -12295.9;
        let yaw = random(seed, 291) * TAU;
        let scale = 1.4;
        let length = (3.0 + random(seed, 292) * 3.0) * scale;
        let r = 0.36 * scale;
        let mut altered = 0;
        let mut worst_old_gap = f32::NEG_INFINITY;
        let mut minimum_embed = f32::INFINITY;
        for lod in 0..=3 {
            let y = terrain_surface_height_lod(&world, x, z, lod) - 0.08;
            let p = Prop {
                position: [x, y, z],
                scale,
                canopy: 1.0,
                seed,
                kind: PropKind::FallenLog,
                biome: Biome::Forest,
            };
            let ex = x + yaw.sin() * length;
            let ez = z + yaw.cos() * length;
            assert_ne!(
                (x / CHUNK_SIZE).floor(),
                (ex / CHUNK_SIZE).floor(),
                "fixture must cross a chunk boundary"
            );
            let start = [x, y + r * 0.85, z];
            let end = [
                ex,
                terrain_surface_height_lod(&world, ex, ez, lod) + r * 0.70 - 0.10,
                ez,
            ];
            assert!((end[1] - start[1]).abs() > 0.3, "fixture must be tilted");
            let cap = [x, y + r, z];
            let mut original = MeshData::default();
            let mut fixed = MeshData::default();
            unanchored_log(&mut original, p, &world, lod);
            fallen_log_lod(&mut fixed, p, &world, lod);
            assert_eq!(original.indices, fixed.indices);
            assert_eq!(original.vertices.len(), fixed.vertices.len());
            for (index, (a, b)) in original
                .vertices
                .iter()
                .zip(fixed.vertices.iter())
                .enumerate()
            {
                assert_eq!(a.position[0], b.position[0]);
                assert_eq!(a.position[2], b.position[2]);
                assert_eq!(a.color, b.color);
                assert_eq!(a.material, b.material);
                assert!(b
                    .position
                    .iter()
                    .chain(b.normal.iter())
                    .all(|v| v.is_finite()));
                assert!((b.normal.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 0.001);
                if index >= 90 {
                    assert_eq!(
                        a.position, b.position,
                        "attached limb must keep its original placement"
                    );
                    continue;
                }
                let center = if index >= 30 {
                    cap
                } else {
                    let ds = (a.position[0] - start[0]).powi(2)
                        + (a.position[1] - start[1]).powi(2)
                        + (a.position[2] - start[2]).powi(2);
                    let de = (a.position[0] - end[0]).powi(2)
                        + (a.position[1] - end[1]).powi(2)
                        + (a.position[2] - end[2]).powi(2);
                    if ds < de {
                        start
                    } else {
                        end
                    }
                };
                if a.position[1] > center[1] + 0.0001 {
                    assert_eq!(
                        a.position, b.position,
                        "upper ring/cap must preserve its silhouette"
                    );
                } else {
                    let ground = (0..=3)
                        .map(|level| {
                            terrain_surface_height_lod(&world, b.position[0], b.position[2], level)
                        })
                        .fold(f32::INFINITY, f32::min);
                    worst_old_gap = worst_old_gap.max(a.position[1] - ground);
                    minimum_embed = minimum_embed.min(ground - b.position[1]);
                    assert!(
                        b.position[1] <= ground - 0.145,
                        "lower ring or cap floats: {:?}, ground{ground}",
                        b.position
                    );
                    altered += usize::from(a.position != b.position);
                }
            }
        }
        assert!(altered > 0);
        assert!(
            worst_old_gap > 0.15,
            "fixture must catch an exposed seam: {worst_old_gap}"
        );
        println!("tilted log seed{seed} x{x} z{z}: old lower-ring/cap gap {worst_old_gap:.3}m, new minimum embed{minimum_embed:.3}m; {altered} lower vertices changed, upper rings and attached limb preserved across LOD0–3");
    }
}

#[cfg(test)]
mod road_grounding_tests {
    use super::*;
    // Add inside geometry.rs's existing #[cfg(test)] mod tests.
    #[test]
    fn road_ribbons_follow_sloping_terrain_at_every_lod() {
        let world = World::new(1337);
        let mut checked_triangles = 0;
        // Select real sloping routes rather than assuming the former grid still
        // crosses specific chunks. Exercise all three road classes.
        let routes = world.road_routes_near(-16546.0, -12304.0, 16000.0);
        let mut chunks = Vec::new();
        for kind in [RoadKind::Main, RoadKind::Lane, RoadKind::Trail] {
            let mut chosen = None;
            'class: for road in routes.iter().filter(|r| r.kind == kind) {
                for pair in road.points.windows(2) {
                    let x = (pair[0][0] + pair[1][0]) * 0.5;
                    let z = (pair[0][1] + pair[1][1]) * 0.5;
                    let sample = world.sample(x, z);
                    if sample.water_height > sample.height {
                        continue;
                    }
                    let cx = (x / CHUNK_SIZE).floor() as i32;
                    let cz = (z / CHUNK_SIZE).floor() as i32;
                    let heights = [
                        (0.0, 0.0),
                        (CHUNK_SIZE, 0.0),
                        (0.0, CHUNK_SIZE),
                        (CHUNK_SIZE, CHUNK_SIZE),
                    ]
                    .map(|(dx, dz)| {
                        world.height(cx as f32 * CHUNK_SIZE + dx, cz as f32 * CHUNK_SIZE + dz)
                    });
                    let range = heights.into_iter().fold(f32::NEG_INFINITY, f32::max)
                        - heights.into_iter().fold(f32::INFINITY, f32::min);
                    if range > 10.0 {
                        chosen = Some((cx, cz));
                        break 'class;
                    }
                }
            }
            chunks.push(chosen.expect("each road class needs a real sloping fixture"));
        }
        for (cx, cz) in chunks {
            for lod in 0..=4 {
                let divisions = (32_usize >> lod).max(2);
                let step = CHUNK_SIZE / divisions as f32;
                let ox = cx as f32 * CHUNK_SIZE;
                let oz = cz as f32 * CHUNK_SIZE;
                let mut grid = Vec::new();
                for z in 0..=divisions {
                    for x in 0..=divisions {
                        grid.push(ground_vertex(
                            &world,
                            ox + x as f32 * step,
                            oz + z as f32 * step,
                        ));
                    }
                }
                let lowest = grid
                    .iter()
                    .map(|v| v.position[1])
                    .fold(f32::INFINITY, f32::min);
                let highest = grid
                    .iter()
                    .map(|v| v.position[1])
                    .fold(f32::NEG_INFINITY, f32::max);
                assert!(
                    highest - lowest > 8.0,
                    "fixture must contain real sloping terrain"
                );
                let mut roads = MeshData::default();
                road_ribbons(&world, &mut roads, ox, oz, divisions, &grid);
                assert!(
                    !roads.vertices.is_empty(),
                    "missing road in chunk {cx},{cz} LOD{lod}"
                );
                for triangle in roads.vertices.chunks_exact(3) {
                    assert!(
                        triangle.iter().all(|v| v.normal[1] > 0.0),
                        "road winding faces down"
                    );
                    for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (0.2, 0.4), (0.6, 0.2)] {
                        let a = triangle[0].position;
                        let b = triangle[1].position;
                        let c = triangle[2].position;
                        let p: [f32; 3] =
                            std::array::from_fn(|i| a[i] + u * (b[i] - a[i]) + v * (c[i] - a[i]));
                        assert!(p.iter().all(|n| n.is_finite()));
                        assert!(
                            p[0] >= ox - 0.005
                                && p[0] <= ox + CHUNK_SIZE + 0.005
                                && p[2] >= oz - 0.005
                                && p[2] <= oz + CHUNK_SIZE + 0.005,
                            "road escapes chunk {cx},{cz} LOD{lod}: {p:?}"
                        );
                        let ground = grid_height(&world, ox, oz, divisions, &grid, p[0], p[2]);
                        // Millimetre tolerance accounts for f32 positions 16km from
                        // origin. Interior samples reject quads spanning two planes,
                        // even when every original corner touched the terrain.
                        assert!(
                        (p[1] - ground - 0.025).abs() < 0.006,
                        "road floats or sinks in chunk {cx},{cz} LOD{lod}: {p:?}, ground {ground}"
                    );
                    }
                    checked_triangles += 1;
                }
            }
        }
        assert!(
            checked_triangles > 200,
            "fixture did not exercise enough road geometry"
        );
    }
}

/// Local-space templates used by the instanced ground-cover layer.
/// Kinds: dry grass, wet grass, fern, flowers, heather. Every recipe fits 8 triangles.
pub(crate) fn cover_template(kind: u32, variant: u32) -> MeshData {
    let mut mesh = MeshData::default();
    let seed = hash(0x434f5645, kind as i32, variant as i32);
    let base = [0.; 3];
    let tint = [1.; 3];
    match kind {
        1 => grass_clump(&mut mesh, base, 1., seed, tint, true),
        2 => fern(&mut mesh, base, 1., seed, tint),
        3 => flowers(&mut mesh, base, 1., seed, tint),
        4 => heather(&mut mesh, base, 1., seed),
        _ => grass_clump(&mut mesh, base, 1., seed, tint, false),
    }
    mesh
}
