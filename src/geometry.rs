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
    pub(crate) fn triangle(
        &mut self,
        a: [f32; 3],
        b: [f32; 3],
        c: [f32; 3],
        color: [f32; 3],
        material: f32,
    ) {
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
    pub(crate) fn quad(
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
    normal: [f32; 3],
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
        normal: [0., 1., 0.],
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
    smooth_ground_grid(world, ox, oz, step, divisions, &mut grid);
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
            let tone = 0.99 + rand01(h) * 0.02;
            // Alternating diagonals avoid a strong regular diagonal pattern on slopes.
            if h & 1 == 0 {
                terrain_triangle(&mut mesh, a, b, d, tone);
                terrain_triangle(&mut mesh, b, c, d, tone * 0.997);
            } else {
                terrain_triangle(&mut mesh, a, b, c, tone);
                terrain_triangle(&mut mesh, a, c, d, tone * 0.997);
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
fn smooth_ground_grid(
    world: &World,
    ox: f32,
    oz: f32,
    step: f32,
    divisions: usize,
    grid: &mut [GroundVertex],
) {
    let mut normals = Vec::with_capacity(grid.len());
    for z in 0..=divisions {
        for x in 0..=divisions {
            let get = |ix: i32, iz: i32| -> f32 {
                if ix >= 0 && iz >= 0 && ix <= divisions as i32 && iz <= divisions as i32 {
                    grid[iz as usize * (divisions + 1) + ix as usize].position[1]
                } else {
                    world.height(ox + ix as f32 * step, oz + iz as f32 * step)
                }
            };
            let dx = get(x as i32 + 1, z as i32) - get(x as i32 - 1, z as i32);
            let dz = get(x as i32, z as i32 + 1) - get(x as i32, z as i32 - 1);
            let n = [-dx, 2. * step, -dz];
            let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
            normals.push(n.map(|v| v / length));
        }
    }
    for (vertex, normal) in grid.iter_mut().zip(normals) {
        vertex.normal = normal;
    }
}
fn terrain_triangle(
    mesh: &mut MeshData,
    a: GroundVertex,
    b: GroundVertex,
    c: GroundVertex,
    tone: f32,
) {
    let first = mesh.vertices.len();
    mesh.triangle(a.position, b.position, c.position, mul(a.color, tone), 0.);
    for (vertex, ground) in mesh.vertices[first..].iter_mut().zip([a, b, c]) {
        vertex.color = mul(ground.color, tone);
        let t = ((ground.normal[1] - 0.52) / 0.38).clamp(0., 1.);
        let smooth = t * t * (3. - 2. * t);
        let n = mix(vertex.normal, ground.normal, smooth);
        let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
        vertex.normal = n.map(|v| v / length);
    }
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
            // Blue stores signed depth (+sea, -freshwater); XY stores flow.
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
        let ocean = [polygon[0], polygon[i], polygon[i + 1]]
            .iter()
            .all(|p| p.0[1] <= crate::world::SEA_LEVEL + 0.01);
        for (position, depth) in [polygon[0], polygon[i], polygon[i + 1]] {
            mesh.vertices.push(Vertex {
                position,
                normal: [0.0, 1.0, 0.0],
                color: [
                    0.0,
                    0.0,
                    if ocean {
                        depth.max(0.0) + 1.0
                    } else {
                        -(depth.max(0.0) + 1.0)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    style: PropStyle,
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
    let region = crate::regions::sample(world.seed, x, z, &sample);
    let density = ecology::tree_density(world.seed, x, z, &sample);
    let formation = region.rockiness > 0.44
        && region.formation_strength > 0.18
        && formation_cell(world.seed, gx, gz)
        && sample.river < 0.10
        && sample.water_height < sample.height - 1.0
        && sample.shore == crate::world::ShoreKind::None;
    let species = random(seed, 6);
    let detail = random(seed, 8);
    let mut kind = if sample.shore != crate::world::ShoreKind::None {
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
    } else if formation {
        PropKind::Boulder
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
    } else if detail > 0.976 - region.rockiness * 0.16 {
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
    if matches!(
        kind,
        PropKind::Broadleaf | PropKind::Birch | PropKind::Pine | PropKind::Fir
    ) && region.pale > 0.5
        && species < 0.70
    {
        kind = PropKind::Birch;
    }
    if matches!(sample.biome, Biome::Grassland | Biome::Forest)
        && region.ancient > 0.62
        && matches!(
            kind,
            PropKind::Broadleaf | PropKind::Birch | PropKind::Pine | PropKind::Fir
        )
        && species < 0.57
    {
        kind = PropKind::Broadleaf;
    }
    // Shape, palette and growth are shared by rendering, distant proxies and collision.
    let grove_scale = if matches!(
        kind,
        PropKind::Pine
            | PropKind::Fir
            | PropKind::Broadleaf
            | PropKind::Birch
            | PropKind::Willow
            | PropKind::DeadTree
    ) {
        (1.0 + density * 0.32) * region.tree_scale
    } else {
        1.0
    };
    let stone_axis = if formation {
        region.formation_axis[1].atan2(region.formation_axis[0])
    } else if kind == PropKind::Boulder {
        let dx = world.height(x + 6., z) - world.height(x - 6., z);
        let dz = world.height(x, z + 6.) - world.height(x, z - 6.);
        if dx * dx + dz * dz > 0.01 {
            dx.atan2(-dz)
        } else {
            random(seed, 809) * TAU
        }
    } else {
        0.
    };
    let prop = Prop {
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
        style: PropStyle {
            ancient: region.ancient,
            pale: region.pale,
            exposure: region.exposure,
            wind_yaw: 1.995,
            geology: region.geology,
            stone: region.rock_color,
            rockiness: region.rockiness,
            stone_axis,
            river_stone: sample.river > 0.16 || region.geology == crate::regions::Geology::Alluvium,
            formation,
        },
    };
    if matches!(
        prop.kind,
        PropKind::Pine
            | PropKind::Fir
            | PropKind::Broadleaf
            | PropKind::Birch
            | PropKind::Willow
            | PropKind::DeadTree
    ) && sample.water_height > -999.
    {
        // Eligibility uses the same triangulated ground as visible roots. A
        // narrow analytic bank can disappear inside its six-metre mesh cell.
        if prop.position[1] < sample.water_height + 0.30 {
            return None;
        }
        let radius = tree_radius(prop) * 1.3;
        for i in 0..8 {
            let angle = i as f32 * TAU / 8.;
            let wx = x + angle.sin() * radius;
            let wz = z + angle.cos() * radius;
            let water = world.natural_sample(wx, wz);
            if water.ocean
                || terrain_surface_height(world, wx, wz) - 0.08 < water.water_height + 0.30
            {
                return None;
            }
        }
    }
    if prop.style.formation {
        // Check the actual footprint, not merely its dry center. At this rare
        // placement rate the extra samples are bounded and avoid coast/river dams.
        let radius = rock_radius(prop);
        let steps = (radius / 2.5).ceil() as i32;
        for iz in -steps..=steps {
            for ix in -steps..=steps {
                let wx = x + ix as f32 * 2.5;
                let wz = z + iz as f32 * 2.5;
                if !rock_blocks(prop, wx, wz, 1.25) {
                    continue;
                }
                let s = world.natural_sample(wx, wz);
                if s.ocean || s.water_height > terrain_surface_height(world, wx, wz) - 0.8 {
                    return None;
                }
            }
        }
    }
    if prop.kind == PropKind::Boulder {
        let radius = rock_radius(prop) + 0.4;
        for road in world.road_routes_near(x, z, radius + 5.) {
            let clearance = radius + road.kind.half_width();
            for segment in road.points.windows(2) {
                let dx = segment[1][0] - segment[0][0];
                let dz = segment[1][1] - segment[0][1];
                let t = (((x - segment[0][0]) * dx + (z - segment[0][1]) * dz)
                    / (dx * dx + dz * dz).max(0.001))
                .clamp(0., 1.);
                if (x - segment[0][0] - dx * t).powi(2) + (z - segment[0][1] - dz * t).powi(2)
                    < clearance * clearance
                {
                    return None;
                }
            }
        }
    }
    Some(prop)
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
                    PropKind::Boulder => landscape_rock(&mut mesh, p),
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
            if hash(world.seed ^ 0x54455252, x, z) & 1 != 0 && !formation_cell(world.seed, x, z) {
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
            let first = mesh.vertices.len();
            if matches!(
                p.kind,
                PropKind::Pine
                    | PropKind::Fir
                    | PropKind::Broadleaf
                    | PropKind::Birch
                    | PropKind::Willow
                    | PropKind::DeadTree
            ) {
                render_tree(&mut mesh, p, true);
            } else if p.kind == PropKind::Boulder {
                distant_rock(&mut mesh, p);
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
    let sites = world.sites_near(x, z, 70.0);
    let landmarks = world.landmarks_near(x, z, 40.0);
    for dz in -2..=2 {
        for dx in -2..=2 {
            let cell_x = (gx + dx) as f32 * PROP_GRID;
            let cell_z = (gz + dz) as f32 * PROP_GRID;
            if x < cell_x - 20.0
                || x > cell_x + PROP_GRID + 20.0
                || z < cell_z - 20.0
                || z > cell_z + PROP_GRID + 20.0
            {
                continue;
            }
            if let Some(p) = prop_at(world, gx + dx, gz + dz) {
                let radius = match p.kind {
                    PropKind::Pine
                    | PropKind::Fir
                    | PropKind::Broadleaf
                    | PropKind::Willow
                    | PropKind::Birch
                    | PropKind::DeadTree => tree_radius(p) + 0.35,
                    PropKind::Boulder => rock_radius(p) + 0.35,
                    PropKind::Stump => 0.48 * p.scale + 0.35,
                    _ => continue,
                };
                if p.kind == PropKind::Boulder && !rock_blocks(p, x, z, 0.35) {
                    continue;
                }
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
                        style: PropStyle::default(),
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
        let w = (0.055 + random(seed, 340 + i) * 0.040) * scale;
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
                normal: [0., 1., 0.],
            },
            GroundVertex {
                position: [0., 4., 8.],
                water: -10000.,
                color: [0.; 3],
                normal: [0., 1., 0.],
            },
            GroundVertex {
                position: [8., -4., 0.],
                water: 0.,
                color: [0.; 3],
                normal: [0., 1., 0.],
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
                style: PropStyle::default(),
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
            assert!(m.indices.len() / 3 <= 580);
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
                far.indices.len() / 3 <= 4352,
                "distant trees and rocks exceeded budget"
            );
            assert!(
                near.indices.len() / 3 < 175000,
                "near geometry exceeded budget"
            );
            assert!(far
                .vertices
                .iter()
                .all(|v| matches!(v.material, 1.0 | 2.0 | 3.0)));
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
                        style: PropStyle::default(),
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
        let yaw = random(seed, 291) * TAU;
        let scale = 1.4;
        let length = (3.0 + random(seed, 292) * 3.0) * scale;
        let r = 0.36 * scale;
        // Discover a truly tilted seam in the current generated terrain. A
        // hard-coded old-world point can legitimately become a flat meadow.
        let (x, z) = (-6..=6)
            .flat_map(|ix| (-6..=6).map(move |iz| (ix, iz)))
            .map(|(ix, iz)| {
                (
                    -74880.15 + ix as f32 * CHUNK_SIZE,
                    69900.0 + iz as f32 * CHUNK_SIZE,
                )
            })
            .find(|&(x, z)| {
                (0..=3).all(|lod| {
                    let start = terrain_surface_height_lod(&world, x, z, lod) - 0.08 + r * 0.85;
                    let end = terrain_surface_height_lod(
                        &world,
                        x + yaw.sin() * length,
                        z + yaw.cos() * length,
                        lod,
                    ) + r * 0.70
                        - 0.10;
                    (end - start).abs() > 0.5
                })
            })
            .expect("actual steep seam is needed to exercise tilted-log grounding");
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
                style: PropStyle::default(),
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
/// Eight plant communities share bounded, bent low-poly templates.
pub(crate) fn cover_template(kind: u32, variant: u32) -> MeshData {
    crate::plants::template(kind, variant)
}

#[derive(Clone, Copy, Debug)]
struct PropStyle {
    ancient: f32,
    pale: f32,
    exposure: f32,
    wind_yaw: f32,
    geology: crate::regions::Geology,
    stone: [f32; 3],
    rockiness: f32,
    stone_axis: f32,
    river_stone: bool,
    formation: bool,
}
impl Default for PropStyle {
    fn default() -> Self {
        Self {
            ancient: 0.,
            pale: 0.,
            exposure: 0.,
            wind_yaw: 0.,
            geology: crate::regions::Geology::Granite,
            stone: [0.43, 0.44, 0.40],
            rockiness: 0.3,
            stone_axis: 0.,
            river_stone: false,
            formation: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TreeAge {
    Young,
    Mature,
    Ancient,
    Windswept,
}
fn tree_age(p: Prop) -> TreeAge {
    let choice = random(p.seed, 501);
    if choice < p.style.ancient * 0.65 + 0.008 {
        TreeAge::Ancient
    } else if p.style.exposure > 0.58 && random(p.seed, 502) < p.style.exposure {
        TreeAge::Windswept
    } else if choice > 0.82 {
        TreeAge::Young
    } else {
        TreeAge::Mature
    }
}
fn tree_radius(p: Prop) -> f32 {
    let species = match p.kind {
        PropKind::Pine => 0.38,
        PropKind::Fir => 0.37,
        PropKind::Broadleaf => 0.64,
        PropKind::Willow => 0.65,
        PropKind::Birch => 0.24,
        _ => 0.34,
    };
    species
        * p.scale
        * match tree_age(p) {
            TreeAge::Young => 0.64,
            TreeAge::Ancient => 1.16,
            _ => 1.,
        }
}
#[derive(Clone, Copy)]
struct TreeLimb {
    start: [f32; 3],
    end: [f32; 3],
    r0: f32,
    r1: f32,
}
#[derive(Clone, Copy)]
struct TreeCrown {
    center: [f32; 3],
    radii: [f32; 3],
    yaw: f32,
    seed: u32,
    color: [f32; 3],
    pointed: bool,
}
struct TreeModel {
    trunk: Vec<([f32; 3], f32)>,
    limbs: Vec<TreeLimb>,
    crowns: Vec<TreeCrown>,
    bark: [f32; 3],
    height: f32,
}
fn stem_at(model: &TreeModel, y: f32) -> [f32; 3] {
    for pair in model.trunk.windows(2) {
        if y <= pair[1].0[1] {
            let t = ((y - pair[0].0[1]) / (pair[1].0[1] - pair[0].0[1])).clamp(0., 1.);
            return std::array::from_fn(|i| pair[0].0[i] + (pair[1].0[i] - pair[0].0[i]) * t);
        }
    }
    model.trunk.last().unwrap().0
}
fn tree_model(p: Prop) -> TreeModel {
    let age = tree_age(p);
    let yaw = random(p.seed, 503) * TAU;
    let age_height = match age {
        TreeAge::Young => 0.70,
        TreeAge::Ancient => 1.12,
        TreeAge::Windswept => 0.85,
        _ => 1.,
    };
    let h = (match p.kind {
        PropKind::Pine => 11. + random(p.seed, 9) * 7.,
        PropKind::Fir => 16. + random(p.seed, 241) * 9.,
        PropKind::Broadleaf => 7.5 + random(p.seed, 15) * 4.,
        PropKind::Birch => 12. + random(p.seed, 261) * 7.,
        PropKind::Willow => 8.5 + random(p.seed, 271) * 4.,
        _ => 6. + random(p.seed, 281) * 5.,
    } * p.scale
        * age_height)
        .min(if matches!(p.kind, PropKind::Pine | PropKind::Fir) {
            64.
        } else {
            48.
        });
    let radius = tree_radius(p);
    let wind = if age == TreeAge::Windswept {
        0.28
    } else {
        0.025 + random(p.seed, 504) * 0.055
    };
    let direction = if age == TreeAge::Windswept {
        p.style.wind_yaw
    } else {
        yaw
    };
    let pale = p.style.pale.clamp(0., 1.);
    let bark = if p.kind == PropKind::Birch {
        mix([0.64, 0.66, 0.57], [0.84, 0.85, 0.74], pale * 0.7 + 0.2)
    } else if p.kind == PropKind::DeadTree {
        mix([0.39, 0.34, 0.27], [0.65, 0.64, 0.55], pale)
    } else {
        mix([0.29, 0.235, 0.16], [0.53, 0.55, 0.46], pale * 0.8)
    };
    let stem_top = if matches!(
        p.kind,
        PropKind::Pine | PropKind::Fir | PropKind::Birch | PropKind::DeadTree
    ) {
        0.97
    } else {
        0.57
    };
    let mut model = TreeModel {
        trunk: Vec::new(),
        limbs: Vec::new(),
        crowns: Vec::new(),
        bark,
        height: h,
    };
    for (i, f) in [
        0.,
        0.08,
        0.24,
        if stem_top < 0.7 { 0.39 } else { 0.56 },
        stem_top,
    ]
    .into_iter()
    .enumerate()
    {
        let bend = h * wind * f * f;
        let kink = (random(p.seed, 510 + i as u32) - 0.5) * radius * f * 0.65;
        model.trunk.push((
            [
                direction.sin() * bend + yaw.cos() * kink,
                h * f,
                direction.cos() * bend - yaw.sin() * kink,
            ],
            radius * (1. - f / stem_top * 0.88),
        ));
    }
    let green = match p.kind {
        PropKind::Pine => mix([0.18, 0.29, 0.18], [0.31, 0.40, 0.23], random(p.seed, 12)),
        PropKind::Fir => mix([0.16, 0.29, 0.25], [0.25, 0.38, 0.29], random(p.seed, 243)),
        PropKind::Birch => mix([0.35, 0.46, 0.20], [0.50, 0.56, 0.28], random(p.seed, 263)),
        PropKind::Willow => mix([0.29, 0.40, 0.22], [0.43, 0.49, 0.30], random(p.seed, 273)),
        _ => mix([0.25, 0.39, 0.16], [0.42, 0.49, 0.21], random(p.seed, 17)),
    };
    let green = mix(green, [0.58, 0.64, 0.47], pale * 0.75);
    if matches!(p.kind, PropKind::Pine | PropKind::Fir) {
        let fir = p.kind == PropKind::Fir;
        let levels = match age {
            TreeAge::Young => 3,
            TreeAge::Ancient => {
                if fir {
                    6
                } else {
                    5
                }
            }
            _ => {
                if fir {
                    5
                } else {
                    4
                }
            }
        };
        let spokes = if fir { 2 } else { 3 };
        for level in 0..levels {
            let f = level as f32 / (levels as f32);
            let coastal = age == TreeAge::Windswept && !fir;
            let y = h
                * ((if fir {
                    0.18
                } else if coastal {
                    0.46
                } else {
                    0.31
                }) + f
                    * (if fir {
                        0.68
                    } else if coastal {
                        0.36
                    } else {
                        0.54
                    }));
            let start = stem_at(&model, y);
            let spread =
                h * (if fir {
                    0.18
                } else if coastal {
                    0.31
                } else {
                    0.24
                }) * (1. - f * 0.77)
                    * p.canopy;
            for arm in 0..spokes {
                let n = (level * spokes + arm) as u32;
                let a = yaw
                    + arm as f32 * TAU / spokes as f32
                    + level as f32 * 1.37
                    + (random(p.seed, 530 + n) - 0.5) * 0.7;
                let reach = spread * (0.65 + random(p.seed, 550 + n) * 0.45);
                let end = [
                    start[0] + a.sin() * reach + direction.sin() * h * wind * f * 0.4,
                    y + h * (if fir { -0.025 } else { 0.035 })
                        + (random(p.seed, 570 + n) - 0.5) * h * 0.018,
                    start[2] + a.cos() * reach + direction.cos() * h * wind * f * 0.4,
                ];
                model.limbs.push(TreeLimb {
                    start,
                    end: [end[0], end[1] + h * 0.065, end[2]],
                    r0: radius * (0.30 - f * 0.17),
                    r1: radius * 0.04,
                });
                model.crowns.push(TreeCrown {
                    center: [end[0], end[1] + h * 0.065, end[2]],
                    radii: [
                        spread * (if fir { 0.80 } else { 0.68 }),
                        h * (if fir {
                            0.15
                        } else if coastal {
                            0.055
                        } else {
                            0.11
                        }) * (1. - f * 0.36),
                        spread * (if fir { 0.65 } else { 0.53 }),
                    ],
                    yaw: a,
                    seed: hash(p.seed, n as i32, 591),
                    color: mul(green, 0.85 + f * 0.18 + random(p.seed, 595 + n) * 0.06),
                    pointed: !coastal,
                });
            }
        }
        let top = stem_at(&model, h * 0.95);
        model.crowns.push(TreeCrown {
            center: top,
            radii: [h * 0.055, h * 0.16, h * 0.045],
            yaw,
            seed: p.seed ^ 613,
            color: mul(green, 1.08),
            pointed: true,
        });
    } else {
        let arms = match age {
            TreeAge::Young => 3,
            TreeAge::Ancient => 5,
            _ => 4,
        };
        for arm in 0..arms {
            let n = arm as u32;
            let birch = p.kind == PropKind::Birch;
            let willow = p.kind == PropKind::Willow;
            let dead = p.kind == PropKind::DeadTree;
            let a = yaw + n as f32 * 2.39996 + (random(p.seed, 620 + n) - 0.5) * 0.85;
            let f = if birch || dead {
                0.35 + n as f32 * 0.095
            } else {
                0.22 + n as f32 * 0.056
            };
            let start = stem_at(&model, h * f);
            let spread =
                h * (if birch {
                    0.20
                } else if dead {
                    0.29
                } else {
                    0.43
                }) * (0.77 + random(p.seed, 630 + n) * 0.42)
                    * p.canopy;
            let elbow = [
                start[0] + a.sin() * spread * 0.62 + direction.sin() * h * wind * 0.2,
                h * (if birch {
                    0.55 + n as f32 * 0.075
                } else if willow {
                    0.68
                } else {
                    0.52 + n as f32 * 0.037
                }) + random(p.seed, 640 + n) * h * 0.055,
                start[2] + a.cos() * spread * 0.62 + direction.cos() * h * wind * 0.2,
            ];
            model.limbs.push(TreeLimb {
                start,
                end: elbow,
                r0: radius * if birch { 0.36 } else { 0.67 },
                r1: radius * if birch { 0.14 } else { 0.31 },
            });
            for fork in 0..2 {
                let j = n * 2 + fork;
                let b = a + (fork as f32 - 0.5) * (0.8 + random(p.seed, 650 + j) * 0.7);
                let end = [
                    elbow[0] + b.sin() * spread * (0.45 + random(p.seed, 660 + j) * 0.25),
                    elbow[1]
                        + h * (if willow {
                            0.055
                        } else if birch {
                            0.08
                        } else {
                            0.095
                        }) * (0.6 + random(p.seed, 670 + j) * 0.9),
                    elbow[2] + b.cos() * spread * (0.45 + random(p.seed, 680 + j) * 0.25),
                ];
                model.limbs.push(TreeLimb {
                    start: elbow,
                    end,
                    r0: radius * if birch { 0.15 } else { 0.32 },
                    r1: radius * 0.055,
                });
                if !dead {
                    let crown_size = if birch {
                        [0.10, 0.14, 0.085]
                    } else if willow {
                        [0.135, 0.25, 0.12]
                    } else {
                        [0.175, 0.102, 0.14]
                    };
                    let varied = 0.77 + random(p.seed, 690 + j) * 0.46;
                    model.crowns.push(TreeCrown {
                        center: [
                            end[0],
                            end[1] - if willow { h * 0.13 } else { -h * 0.02 },
                            end[2],
                        ],
                        radii: crown_size.map(|v| v * h * varied),
                        yaw: b,
                        seed: hash(p.seed, j as i32, 701),
                        color: mul(green, 0.87 + random(p.seed, 710 + j) * 0.22),
                        pointed: false,
                    });
                }
            }
        }
    }
    if p.kind == PropKind::Birch && age != TreeAge::Young {
        for n in 0..2u32 {
            let a = yaw + n as f32 * 2.7 + 0.7;
            let base = [a.sin() * radius * 0.55, 0., a.cos() * radius * 0.55];
            let elbow = [
                a.sin() * h * 0.045,
                h * (0.34 + n as f32 * 0.04),
                a.cos() * h * 0.045,
            ];
            let tip = [
                a.sin() * h * 0.15,
                h * (0.72 + n as f32 * 0.12),
                a.cos() * h * 0.15,
            ];
            model.limbs.push(TreeLimb {
                start: base,
                end: elbow,
                r0: radius * 0.35,
                r1: radius * 0.20,
            });
            model.limbs.push(TreeLimb {
                start: elbow,
                end: tip,
                r0: radius * 0.20,
                r1: radius * 0.035,
            });
            model.crowns.push(TreeCrown {
                center: tip,
                radii: [h * 0.09, h * 0.14, h * 0.08],
                yaw: a,
                seed: hash(p.seed, n as i32, 718),
                color: mul(green, 1.02 + n as f32 * 0.04),
                pointed: false,
            });
        }
    }
    model
}
fn tree_trunk(mesh: &mut MeshData, p: Prop, model: &TreeModel, far: bool) {
    let sides = if far { 4 } else { 6 };
    let yaw = random(p.seed, 520) * TAU;
    let rings: Vec<Vec<[f32; 3]>> = model
        .trunk
        .iter()
        .enumerate()
        .filter(|(i, _)| !far || *i == 0 || *i == 2 || *i == model.trunk.len() - 1)
        .map(|(_, &(center, radius))| {
            (0..sides)
                .map(|i| {
                    let a = yaw + i as f32 * TAU / sides as f32;
                    [
                        p.position[0] + center[0] + a.sin() * radius,
                        p.position[1] + center[1],
                        p.position[2] + center[2] + a.cos() * radius,
                    ]
                })
                .collect()
        })
        .collect();
    for (level, pair) in rings.windows(2).enumerate() {
        for i in 0..sides {
            let j = (i + 1) % sides;
            let tone = (0.83 + level as f32 * 0.05) * (0.96 + (i % 3) as f32 * 0.035);
            mesh.quad(
                pair[0][i],
                pair[0][j],
                pair[1][j],
                pair[1][i],
                mul(model.bark, tone),
                3.,
            );
        }
    }
}
fn crown_mesh(mesh: &mut MeshData, base: [f32; 3], c: TreeCrown, far: bool) {
    let center: [f32; 3] = std::array::from_fn(|i| base[i] + c.center[i]);
    if c.pointed {
        let sides = if far { 3 } else { 5 };
        let mut lower = Vec::new();
        let mut upper = Vec::new();
        for i in 0..sides {
            let a = c.yaw + i as f32 * TAU / sides as f32;
            let variation = 0.77 + random(c.seed, 730 + i as u32) * 0.42;
            lower.push([
                center[0] + a.sin() * c.radii[0] * variation,
                center[1] - c.radii[1] * 0.24,
                center[2] + a.cos() * c.radii[2] * variation,
            ]);
            upper.push([
                center[0] + a.sin() * c.radii[0] * variation * 0.54,
                center[1] + c.radii[1] * 0.25,
                center[2] + a.cos() * c.radii[2] * variation * 0.54,
            ]);
        }
        let top = [
            center[0] + c.radii[0] * 0.12,
            center[1] + c.radii[1] * 0.75,
            center[2] - c.radii[2] * 0.09,
        ];
        let bottom = [center[0], center[1] - c.radii[1] * 0.24, center[2]];
        for i in 0..sides {
            let j = (i + 1) % sides;
            if far {
                mesh.triangle(lower[i], lower[j], top, c.color, 1.);
            } else {
                mesh.quad(
                    lower[i],
                    lower[j],
                    upper[j],
                    upper[i],
                    mul(c.color, 0.90 + (i % 3) as f32 * 0.04),
                    1.,
                );
                mesh.triangle(upper[i], upper[j], top, mul(c.color, 1.04), 1.);
            }
            mesh.triangle(bottom, lower[j], lower[i], mul(c.color, 0.63), 1.);
        }
    } else if far {
        let s = c.radii;
        let points = [
            [center[0] - s[0], center[1], center[2]],
            [center[0] + s[0], center[1], center[2]],
            [center[0], center[1] - s[1], center[2]],
            [center[0], center[1] + s[1], center[2]],
            [center[0], center[1], center[2] - s[2]],
            [center[0], center[1], center[2] + s[2]],
        ];
        for f in [
            [3, 0, 4],
            [3, 4, 1],
            [3, 1, 5],
            [3, 5, 0],
            [2, 4, 0],
            [2, 1, 4],
            [2, 5, 1],
            [2, 0, 5],
        ] {
            mesh.triangle(
                points[f[0]],
                points[f[2]],
                points[f[1]],
                mul(c.color, if f[0] == 2 { 0.76 } else { 1. }),
                1.,
            );
        }
    } else {
        // Irregular layered foliage pads expose the fork structure through
        // gaps. Broad horizontal upper facets replace round polygon spheres.
        let lower: [[f32; 3]; 6] = std::array::from_fn(|i| {
            let angle = c.yaw + i as f32 * TAU / 6.;
            let v = 0.76 + random(c.seed, 750 + i as u32) * 0.40;
            [
                center[0] + angle.sin() * c.radii[0] * v,
                center[1] + c.radii[1] * (random(c.seed, 760 + i as u32) * 0.32 - 0.22),
                center[2] + angle.cos() * c.radii[2] * v,
            ]
        });
        let upper: [[f32; 3]; 6] = std::array::from_fn(|i| {
            [
                center[0] + (lower[i][0] - center[0]) * 0.62 + c.radii[0] * 0.08,
                center[1] + c.radii[1] * (0.54 + random(c.seed, 770 + i as u32) * 0.30),
                center[2] + (lower[i][2] - center[2]) * 0.63 - c.radii[2] * 0.06,
            ]
        });
        let top = [
            center[0] + c.radii[0] * 0.12,
            center[1] + c.radii[1] * 0.96,
            center[2] - c.radii[2] * 0.04,
        ];
        let bottom = [center[0], center[1] - c.radii[1] * 0.78, center[2]];
        for i in 0..6 {
            let j = (i + 1) % 6;
            mesh.quad(
                lower[i],
                lower[j],
                upper[j],
                upper[i],
                mul(c.color, 0.88 + (i % 3) as f32 * 0.045),
                1.,
            );
            mesh.triangle(upper[i], upper[j], top, mul(c.color, 1.04), 1.);
            mesh.triangle(bottom, lower[j], lower[i], mul(c.color, 0.71), 1.);
        }
    }
}
fn render_tree(mesh: &mut MeshData, p: Prop, far: bool) {
    let model = tree_model(p);
    tree_trunk(mesh, p, &model, far);
    if far {
        for crown in distant_crowns(&model, p) {
            crown_mesh(mesh, p.position, crown, true);
            if matches!(p.kind, PropKind::Broadleaf | PropKind::Willow)
                || (p.kind == PropKind::Pine && tree_age(p) == TreeAge::Windswept)
            {
                // Separated crown pads need a visible attachment even after
                // branch-detail LOD. Two crossed tapered strokes keep the actual
                // limb height and crown center without rebuilding the hierarchy.
                let nearest = model.limbs.iter().min_by(|a, b| {
                    let distance = |limb: &TreeLimb| {
                        (0..3)
                            .map(|i| (limb.end[i] - crown.center[i]).powi(2))
                            .sum::<f32>()
                    };
                    distance(a).total_cmp(&distance(b))
                });
                if let Some(limb) = nearest {
                    let stem = stem_at(&model, limb.start[1]);
                    let start: [f32; 3] = std::array::from_fn(|i| p.position[i] + stem[i]);
                    let end = std::array::from_fn(|i| p.position[i] + crown.center[i]);
                    let width = tree_radius(p) * 0.38;
                    for side in [[width, 0., 0.], [0., 0., width]] {
                        mesh.triangle(
                            std::array::from_fn(|i| start[i] - side[i]),
                            std::array::from_fn(|i| start[i] + side[i]),
                            end,
                            model.bark,
                            3.,
                        );
                    }
                }
            }
        }
        if p.kind == PropKind::Birch {
            for root in model.limbs.iter().filter(|limb| limb.start[1] < 0.01) {
                for limb in model
                    .limbs
                    .iter()
                    .filter(|limb| limb.start == root.start || limb.start == root.end)
                {
                    let a: [f32; 3] = std::array::from_fn(|i| p.position[i] + limb.start[i]);
                    let b: [f32; 3] = std::array::from_fn(|i| p.position[i] + limb.end[i]);
                    for axis in [[limb.r0, 0., 0.], [0., 0., limb.r0]] {
                        mesh.quad(
                            std::array::from_fn(|i| a[i] - axis[i]),
                            std::array::from_fn(|i| a[i] + axis[i]),
                            std::array::from_fn(|i| b[i] + axis[i] * 0.4),
                            std::array::from_fn(|i| b[i] - axis[i] * 0.4),
                            model.bark,
                            3.,
                        );
                    }
                }
            }
        }
        if p.kind == PropKind::DeadTree {
            // Four tapered silhouette strokes retain the snag's spread without
            // rebuilding any branch hierarchy in the distant mesh.
            for (axis, sign) in [(0, 1.), (0, -1.), (2, 1.), (2, -1.)] {
                if let Some(limb) = model
                    .limbs
                    .iter()
                    .max_by(|a, b| (a.end[axis] * sign).total_cmp(&(b.end[axis] * sign)))
                {
                    let start = stem_at(&model, limb.start[1]);
                    let end = limb.end;
                    let dx = end[0] - start[0];
                    let dz = end[2] - start[2];
                    let length = dx.hypot(dz).max(0.001);
                    let width = tree_radius(p) * 0.34;
                    let side = [-dz / length * width, 0., dx / length * width];
                    let a = std::array::from_fn(|i| p.position[i] + start[i] - side[i]);
                    let b = std::array::from_fn(|i| p.position[i] + start[i] + side[i]);
                    let c = std::array::from_fn(|i| p.position[i] + end[i]);
                    mesh.triangle(a, b, c, model.bark, 3.);
                }
            }
        }
        return;
    }
    if !matches!(p.kind, PropKind::Birch | PropKind::DeadTree) {
        let r = tree_radius(p);
        for i in 0..4 {
            let a = random(p.seed, 523) * TAU + i as f32 * TAU / 4.;
            let stem = stem_at(&model, r * 1.8 + 0.25);
            let top = std::array::from_fn(|j| p.position[j] + stem[j]);
            let tip = [
                p.position[0] + a.sin() * r * 1.0,
                p.position[1] - 0.04,
                p.position[2] + a.cos() * r * 1.0,
            ];
            let left = [
                p.position[0] + (a - 0.4).sin() * r * 0.8,
                p.position[1] - 0.04,
                p.position[2] + (a - 0.4).cos() * r * 0.8,
            ];
            let right = [
                p.position[0] + (a + 0.4).sin() * r * 0.8,
                p.position[1] - 0.04,
                p.position[2] + (a + 0.4).cos() * r * 0.8,
            ];
            mesh.triangle(left, tip, top, mul(model.bark, 0.90), 3.);
            mesh.triangle(tip, right, top, mul(model.bark, 0.95), 3.);
        }
    }
    if !far || p.kind == PropKind::DeadTree {
        for limb in &model.limbs {
            let start = std::array::from_fn(|i| p.position[i] + limb.start[i]);
            let end = std::array::from_fn(|i| p.position[i] + limb.end[i]);
            if far {
                let r = limb.r0;
                for axis in [[r, 0., 0.], [0., 0., r]] {
                    let a = std::array::from_fn(|i| start[i] - axis[i]);
                    let b = std::array::from_fn(|i| start[i] + axis[i]);
                    let c = std::array::from_fn(|i| end[i] + axis[i] * 0.25);
                    let d = std::array::from_fn(|i| end[i] - axis[i] * 0.25);
                    mesh.quad(a, b, c, d, model.bark, 3.);
                }
            } else {
                branch(mesh, start, end, limb.r0, limb.r1, mul(model.bark, 0.93));
            }
        }
    }
    for crown in &model.crowns {
        crown_mesh(mesh, p.position, *crown, far);
    }
    if !far && p.kind == PropKind::Birch {
        for i in 0..5 {
            let y = model.height * (0.13 + i as f32 * 0.095);
            let stem = stem_at(&model, y);
            let r = tree_radius(p) * (1. - y / model.height * 0.88);
            let x = p.position[0] + stem[0];
            let z = p.position[2] + stem[2];
            mesh.quad(
                [x - r, p.position[1] + y, z + r],
                [x + r * 0.65, p.position[1] + y, z + r],
                [x + r * 0.35, p.position[1] + y + 0.08, z + r],
                [x - r, p.position[1] + y + 0.12, z + r],
                [0.25, 0.29, 0.25],
                3.,
            );
        }
    }
}
fn pine(mesh: &mut MeshData, p: Prop) {
    render_tree(mesh, p, false);
}
fn fir(mesh: &mut MeshData, p: Prop) {
    render_tree(mesh, p, false);
}
fn broadleaf(mesh: &mut MeshData, p: Prop) {
    render_tree(mesh, p, false);
}
fn birch(mesh: &mut MeshData, p: Prop) {
    render_tree(mesh, p, false);
}
fn willow(mesh: &mut MeshData, p: Prop) {
    render_tree(mesh, p, false);
}
fn dead_tree(mesh: &mut MeshData, p: Prop) {
    render_tree(mesh, p, false);
}

#[derive(Clone, Copy)]
struct RockPiece {
    scree: bool,
    center: [f32; 3],
    size: [f32; 3],
    yaw: f32,
    seed: u32,
}
fn rock_pieces(p: Prop) -> Vec<RockPiece> {
    if p.style.formation {
        return formation_pieces(p);
    }
    let mut pieces = Vec::new();
    let cluster = p.style.rockiness > 0.55 && random(p.seed, 811) > 0.52;
    let river = p.style.river_stone;
    let count = if cluster {
        4
    } else if river {
        3
    } else {
        1
    };
    for i in 0..count {
        let n = i as u32;
        let satellite = i > 0;
        let a = p.style.stone_axis
            + if satellite {
                (i as f32 - 2.) * 0.75
            } else {
                0.
            };
        let offset = if satellite {
            p.scale * (1.20 + random(p.seed, 812 + n) * 0.65)
        } else {
            0.
        };
        let size = p.scale
            * if satellite {
                0.35 + random(p.seed, 822 + n) * 0.52
            } else {
                1.05 + random(p.seed, 832 + n) * 0.68
            };
        pieces.push(RockPiece {
            scree: false,
            center: [a.cos() * offset, 0., a.sin() * offset],
            size: [
                size * (if river { 1.1 } else { 1.0 }),
                size * (if river {
                    0.46
                } else {
                    0.85 + random(p.seed, 842 + n) * 0.35
                }),
                size * 0.84,
            ],
            yaw: p.style.stone_axis
                + (random(p.seed, 852 + n) - 0.5) * (if river { 1.5 } else { 0.32 }),
            seed: hash(p.seed, i, 861),
        });
    }
    pieces
}
fn rock_radius(p: Prop) -> f32 {
    rock_pieces(p)
        .iter()
        .map(|piece| {
            piece.center[0].hypot(piece.center[2]) + piece.size[0].hypot(piece.size[2]) * 1.14
        })
        .fold(0., f32::max)
}
// Beveled polygonal slabs expose large fracture planes rather than a jittered ball.
fn fracture_block(
    mesh: &mut MeshData,
    base: [f32; 3],
    size: [f32; 3],
    yaw: f32,
    seed: u32,
    color: [f32; 3],
    column: bool,
) {
    let footprint: Vec<[f32; 2]> = if column {
        (0..6)
            .map(|i| {
                let a = i as f32 * TAU / 6.;
                [a.cos(), a.sin()]
            })
            .collect()
    } else {
        vec![
            [-1., -0.60],
            [-0.60, -1.],
            [0.62, -1.],
            [1., -0.55],
            [1., 0.64],
            [0.55, 1.],
            [-0.62, 1.],
            [-1., 0.52],
        ]
    };
    let count = footprint.len();
    let mut bottom = Vec::new();
    let mut top = Vec::new();
    for (i, p) in footprint.iter().enumerate() {
        let cut = 0.91 + random(seed, 870 + i as u32) * 0.16;
        let x = p[0] * size[0] * cut;
        let z = p[1] * size[2] * cut;
        let dx = x * yaw.cos() - z * yaw.sin();
        let dz = x * yaw.sin() + z * yaw.cos();
        bottom.push([base[0] + dx, base[1] - size[1] * 0.16, base[2] + dz]);
        let top_y = size[1] * (0.94 + p[0] * 0.13 - p[1] * 0.09);
        top.push([
            base[0] + dx * (0.79 + random(seed, 880 + i as u32) * 0.15),
            base[1] + top_y,
            base[2] + dz * (0.82 + random(seed, 890 + i as u32) * 0.13),
        ]);
    }
    for i in 0..count {
        let j = (i + 1) % count;
        mesh.quad(
            bottom[i],
            top[i],
            top[j],
            bottom[j],
            mul(color, 0.85 + (i % 3) as f32 * 0.055),
            2.,
        );
    }
    for i in 1..count - 1 {
        mesh.triangle(top[0], top[i + 1], top[i], mul(color, 1.025), 2.);
        mesh.triangle(bottom[0], bottom[i], bottom[i + 1], mul(color, 0.7), 2.);
    }
}
// Broad stone masses remain visible beyond branch-level detail. These share
// the near rock descriptors and terrain anchoring, with at most24 triangles.
fn distant_rock(mesh: &mut MeshData, p: Prop) {
    let pieces = rock_pieces(p);
    for piece in pieces
        .iter()
        .filter(|p| !p.scree)
        .take(if p.style.formation { 3 } else { 2 })
    {
        let base: [f32; 3] = std::array::from_fn(|i| p.position[i] + piece.center[i]);
        if p.style.river_stone {
            polyhedron(
                mesh,
                [base[0], base[1] + piece.size[1] * 0.52, base[2]],
                piece.size,
                piece.seed,
                p.style.stone,
                2.,
            );
        } else {
            // Five exposed faces, with a skewed roof; twelve triangles including
            // the buried base. Same descriptor widths and heights as near masses.
            let mut footprint = [[0.; 3]; 8];
            for i in 0..4 {
                let sx = if i == 0 || i == 3 { -1. } else { 1. };
                let sz = if i < 2 { -1. } else { 1. };
                let x = sx * piece.size[0] * 0.91;
                let z = sz * piece.size[2] * 0.91;
                let dx = x * piece.yaw.cos() - z * piece.yaw.sin();
                let dz = x * piece.yaw.sin() + z * piece.yaw.cos();
                footprint[i] = [base[0] + dx, base[1] - piece.size[1] * 0.16, base[2] + dz];
                footprint[i + 4] = [
                    base[0] + dx * 0.88,
                    base[1] + piece.size[1] * (0.94 + sx * 0.13 - sz * 0.09),
                    base[2] + dz * 0.88,
                ];
            }
            for face in [
                [0, 1, 5, 4],
                [1, 2, 6, 5],
                [2, 3, 7, 6],
                [3, 0, 4, 7],
                [4, 5, 6, 7],
                [3, 2, 1, 0],
            ] {
                mesh.quad(
                    footprint[face[0]],
                    footprint[face[1]],
                    footprint[face[2]],
                    footprint[face[3]],
                    p.style.stone,
                    2.,
                );
            }
        }
    }
}
fn landscape_rock(mesh: &mut MeshData, p: Prop) {
    use crate::regions::Geology;
    for piece in rock_pieces(p) {
        let base = std::array::from_fn(|i| p.position[i] + piece.center[i]);
        let stone = mul(p.style.stone, 0.93 + random(piece.seed, 901) * 0.12);
        if piece.scree {
            fracture_block(
                mesh,
                base,
                piece.size,
                piece.yaw,
                piece.seed,
                mul(stone, 0.92),
                false,
            );
            continue;
        }
        if p.style.river_stone || p.style.geology == Geology::Alluvium {
            polyhedron(
                mesh,
                [base[0], base[1] + piece.size[1] * 0.52, base[2]],
                piece.size,
                piece.seed,
                stone,
                2.,
            );
        } else if p.style.formation {
            fracture_block(
                mesh,
                base,
                piece.size,
                piece.yaw,
                piece.seed,
                stone,
                p.style.geology == Geology::Basalt,
            );
            if p.style.geology == Geology::Granite && random(piece.seed, 924) > 0.4 {
                fracture_block(
                    mesh,
                    [
                        base[0] + piece.size[0] * 0.12,
                        base[1] + piece.size[1] * 0.63,
                        base[2],
                    ],
                    [
                        piece.size[0] * 0.70,
                        piece.size[1] * 0.54,
                        piece.size[2] * 0.68,
                    ],
                    piece.yaw + 0.12,
                    piece.seed ^ 925,
                    mul(stone, 1.035),
                    false,
                );
            }
        } else {
            match p.style.geology {
                Geology::Sandstone | Geology::Chalk => {
                    let layers = if p.style.geology == Geology::Chalk {
                        2
                    } else {
                        3
                    };
                    for layer in 0..layers {
                        let f = layer as f32;
                        let narrowing = 1. - f * 0.13;
                        let size = [
                            piece.size[0] * narrowing,
                            piece.size[1] / layers as f32 * 0.82,
                            piece.size[2] * narrowing,
                        ];
                        fracture_block(
                            mesh,
                            [
                                base[0] + f * piece.size[0] * 0.055,
                                base[1] + f * size[1] * 0.82,
                                base[2] - f * piece.size[2] * 0.045,
                            ],
                            size,
                            piece.yaw,
                            piece.seed ^ layer as u32,
                            mul(stone, 0.88 + f * 0.075),
                            false,
                        );
                    }
                }
                Geology::Basalt => {
                    for pillar in 0..3 {
                        let a = piece.yaw + pillar as f32 * TAU / 3.;
                        fracture_block(
                            mesh,
                            [
                                base[0] + a.cos() * piece.size[0] * 0.35,
                                base[1],
                                base[2] + a.sin() * piece.size[2] * 0.35,
                            ],
                            [
                                piece.size[0] * 0.49,
                                piece.size[1] * (0.7 + random(piece.seed, 911 + pillar) * 0.7),
                                piece.size[2] * 0.49,
                            ],
                            piece.yaw,
                            piece.seed ^ pillar,
                            stone,
                            true,
                        );
                    }
                }
                _ => {
                    fracture_block(mesh, base, piece.size, piece.yaw, piece.seed, stone, false);
                    if random(piece.seed, 921) > 0.65 {
                        fracture_block(
                            mesh,
                            [
                                base[0] + piece.size[0] * 0.24,
                                base[1] + piece.size[1] * 0.32,
                                base[2] - piece.size[2] * 0.16,
                            ],
                            [
                                piece.size[0] * 0.65,
                                piece.size[1] * 0.7,
                                piece.size[2] * 0.62,
                            ],
                            piece.yaw + 0.32,
                            piece.seed ^ 933,
                            mul(stone, 1.05),
                            false,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod regional_geometry_tests {
    use super::*;
    fn valid(mesh: &MeshData) {
        assert!(mesh.vertices.iter().all(|v| v
            .position
            .iter()
            .chain(v.normal.iter())
            .chain(v.color.iter())
            .all(|f| f.is_finite())));
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
    }
    #[test]
    fn regional_tree_variants_keep_bounded_deterministic_near_and_far_forms() {
        let mut high_near = 0;
        let mut high_far = 0;
        let mut ages = [0usize; 4];
        for kind in [
            PropKind::Pine,
            PropKind::Fir,
            PropKind::Broadleaf,
            PropKind::Birch,
            PropKind::Willow,
            PropKind::DeadTree,
        ] {
            for seed in 0..32 {
                for style in [
                    PropStyle::default(),
                    PropStyle {
                        ancient: 1.,
                        pale: 0.8,
                        ..PropStyle::default()
                    },
                    PropStyle {
                        exposure: 1.,
                        wind_yaw: 1.995,
                        ..PropStyle::default()
                    },
                ] {
                    let prop = Prop {
                        position: [0., 0., 0.],
                        scale: 1.2,
                        canopy: 1.2,
                        seed,
                        kind,
                        biome: Biome::Forest,
                        style,
                    };
                    let age = tree_age(prop);
                    ages[match age {
                        TreeAge::Young => 0,
                        TreeAge::Mature => 1,
                        TreeAge::Ancient => 2,
                        TreeAge::Windswept => 3,
                    }] += 1;
                    let mut near = MeshData::default();
                    let mut far = MeshData::default();
                    let mut repeat = MeshData::default();
                    render_tree(&mut near, prop, false);
                    render_tree(&mut far, prop, true);
                    render_tree(&mut repeat, prop, false);
                    valid(&near);
                    valid(&far);
                    assert_eq!(
                        bytemuck::cast_slice::<_, u8>(&near.vertices),
                        bytemuck::cast_slice::<_, u8>(&repeat.vertices)
                    );
                    assert!(near.indices.len() / 3 <= 580);
                    assert!(far.indices.len() / 3 <= 50);
                    high_near = high_near.max(near.indices.len() / 3);
                    high_far = high_far.max(far.indices.len() / 3);
                    // Both detail levels use identical crown centers/trunk skeleton; their
                    // envelope must also remain close when proxies replace crown faces.
                    let bounds = |mesh: &MeshData| -> ([f32; 3], [f32; 3]) {
                        (
                            std::array::from_fn(|a| {
                                mesh.vertices
                                    .iter()
                                    .map(|v| v.position[a])
                                    .fold(f32::INFINITY, f32::min)
                            }),
                            std::array::from_fn(|a| {
                                mesh.vertices
                                    .iter()
                                    .map(|v| v.position[a])
                                    .fold(f32::NEG_INFINITY, f32::max)
                            }),
                        )
                    };
                    let (a, b) = bounds(&near);
                    let (c, d) = bounds(&far);
                    let h = b[1] - a[1];
                    for axis in 0..3 {
                        assert!(
                            (a[axis] - c[axis]).abs() < h * 0.20 + 0.3,
                            "{kind:?} seed{seed} lowaxis{axis} h{h} near{a:?} far{c:?}"
                        );
                        assert!(
                            (b[axis] - d[axis]).abs() < h * 0.20 + 0.3,
                            "{kind:?} seed{seed} highaxis{axis} h{h} near{b:?} far{d:?}"
                        );
                    }
                }
            }
        }
        assert!(ages.iter().all(|&n| n > 0));
        println!("regional trees: max near={high_near}, far={high_far}; age coverage={ages:?}");
    }
    #[test]
    fn geological_shapes_embed_and_keep_collision_footprints_at_every_lod() {
        use crate::regions::Geology;
        let world = World::new(1337);
        let mut maximum = 0;
        for geology in [
            Geology::Granite,
            Geology::Sandstone,
            Geology::Chalk,
            Geology::Basalt,
            Geology::Alluvium,
            Geology::Loam,
        ] {
            for lod in 0..=3 {
                for seed in 0..4 {
                    let x = -16704.1;
                    let z = -12288.1;
                    let prop = Prop {
                        position: [x, terrain_surface_height_lod(&world, x, z, lod) - 0.08, z],
                        scale: 1.1,
                        canopy: 1.,
                        seed,
                        kind: PropKind::Boulder,
                        biome: Biome::Moor,
                        style: PropStyle {
                            geology,
                            rockiness: 1.,
                            stone_axis: 0.7,
                            ..PropStyle::default()
                        },
                    };
                    let mut mesh = MeshData::default();
                    landscape_rock(&mut mesh, prop);
                    valid(&mesh);
                    maximum = maximum.max(mesh.indices.len() / 3);
                    assert!(mesh.indices.len() / 3 <= 340);
                    let radius = rock_radius(prop);
                    assert!(mesh
                        .vertices
                        .iter()
                        .all(|v| (v.position[0] - x).hypot(v.position[2] - z) <= radius + 0.01));
                    let before = mesh.vertices.clone();
                    anchor_prop(&world, &mut mesh, 0, prop, lod);
                    valid(&mesh);
                    let mut roots = 0;
                    for (a, b) in before.iter().zip(&mesh.vertices) {
                        if a.position[1] <= prop.position[1] + 0.01 {
                            roots += 1;
                            let ground = terrain_surface_height_lod(
                                &world,
                                b.position[0],
                                b.position[2],
                                lod,
                            );
                            assert!(
                                b.position[1] <= ground - 0.06,
                                "floating geological base at lod {lod}"
                            );
                        }
                    }
                    assert!(roots > 5);
                }
            }
        }
        println!("geological recipe maximum={maximum} triangles, grounded across LOD0–3");
    }
    #[test]
    fn smooth_ground_normals_agree_at_chunk_edges_and_preserve_surface() {
        let world = World::new(1337);
        for lod in 0..=3 {
            let divisions = (32u32 >> lod) as usize;
            let step = 192. / divisions as f32;
            let ox = -16704.;
            let oz = -12288.;
            let build = |xorigin: f32| {
                let mut grid = Vec::new();
                for z in 0..=divisions {
                    for x in 0..=divisions {
                        grid.push(ground_vertex(
                            &world,
                            xorigin + x as f32 * step,
                            oz + z as f32 * step,
                        ));
                    }
                }
                let positions: Vec<_> = grid.iter().map(|v| v.position).collect();
                smooth_ground_grid(&world, xorigin, oz, step, divisions, &mut grid);
                assert_eq!(
                    positions,
                    grid.iter().map(|v| v.position).collect::<Vec<_>>()
                );
                grid
            };
            let left = build(ox);
            let right = build(ox + 192.);
            for z in 0..=divisions {
                let a = left[z * (divisions + 1) + divisions];
                let b = right[z * (divisions + 1)];
                assert_eq!(a.position, b.position);
                for axis in 0..3 {
                    assert!((a.normal[axis] - b.normal[axis]).abs() < 0.00001);
                }
            }
        }
    }
    #[test]
    fn flower_and_seedhead_templates_fit_the_instance_contract() {
        for variant in 0..8 {
            for kind in [3, 5] {
                let mesh = cover_template(kind, variant);
                valid(&mesh);
                assert!(mesh.vertices.len() <= (crate::plants::TRIANGLES * 3) as usize);
                assert!(mesh
                    .vertices
                    .iter()
                    .all(|v| v.position[1] >= 0. && v.position[1] < 1.4));
                if kind == 3 && variant < 6 {
                    let heads: Vec<_> = mesh
                        .vertices
                        .chunks_exact(3)
                        .filter(|tri| tri[0].color[0] > 0.9 && tri[0].color[2] < 0.8)
                        .collect();
                    assert!(!heads.is_empty());
                    assert!(heads.iter().all(|tri| tri[0].normal[1] > 0.9));
                }
            }
        }
    }
}

// Distant forms keep the growth skeleton but merge nearby crown lobes into two
// or three large readable masses, rather than repeating every near-tree branch.
fn distant_crowns(model: &TreeModel, p: Prop) -> Vec<TreeCrown> {
    if model.crowns.is_empty() {
        return Vec::new();
    }
    let conifer = matches!(p.kind, PropKind::Pine | PropKind::Fir)
        && !(p.kind == PropKind::Pine && tree_age(p) == TreeAge::Windswept);
    let groups = match p.kind {
        PropKind::Pine => 3,
        PropKind::Fir => 2,
        PropKind::Birch => 2,
        PropKind::Broadleaf => 3,
        _ => 2,
    };
    let mut crowns = model.crowns.clone();
    let yaw = random(p.seed, 503) * TAU;
    crowns.sort_by(|a, b| {
        let key = |c: &TreeCrown| {
            if conifer {
                c.center[1]
            } else {
                c.center[0] * yaw.cos() + c.center[2] * yaw.sin()
            }
        };
        key(a).total_cmp(&key(b))
    });
    let mut result = Vec::with_capacity(groups);
    for group in 0..groups {
        let members = &crowns[group * crowns.len() / groups..(group + 1) * crowns.len() / groups];
        if members.is_empty() {
            continue;
        }
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        let mut color = [0.; 3];
        for c in members {
            for axis in 0..3 {
                let lower = if conifer && axis == 1 {
                    c.radii[axis] * 0.24
                } else {
                    c.radii[axis]
                };
                let upper = if conifer && axis == 1 {
                    c.radii[axis] * 0.75
                } else {
                    c.radii[axis]
                };
                low[axis] = low[axis].min(c.center[axis] - lower);
                high[axis] = high[axis].max(c.center[axis] + upper);
                color[axis] += c.color[axis] / members.len() as f32;
            }
        }
        let mut center = std::array::from_fn(|i| (low[i] + high[i]) * 0.5);
        let mut radii = std::array::from_fn(|i| (high[i] - low[i]) * 0.5);
        if conifer {
            radii[1] = (high[1] - low[1]) / 0.99;
            center[1] = low[1] + radii[1] * 0.24;
        }
        result.push(TreeCrown {
            center,
            radii,
            yaw: members[0].yaw,
            seed: members[0].seed,
            color,
            pointed: conifer,
        });
    }
    result
}

// One nominated candidate in half the 48 m cells: a coherent landform accent,
// never an oversized rock on every ordinary 12 m vegetation candidate.
fn formation_cell(seed: u32, gx: i32, gz: i32) -> bool {
    let h = hash(seed ^ 0x544f5253, gx.div_euclid(4), gz.div_euclid(4));
    h & 1 == 0
        && gx.rem_euclid(4) == ((h >> 3) & 3) as i32
        && gz.rem_euclid(4) == ((h >> 7) & 3) as i32
}
fn formation_pieces(p: Prop) -> Vec<RockPiece> {
    use crate::regions::Geology;
    let axis = p.style.stone_axis;
    let scale = p.scale.min(1.38);
    let mut pieces = Vec::with_capacity(7);
    for i in 0..3u32 {
        let along = (i as f32 - 1.) * 3.6 * scale;
        let side = (random(p.seed, 1101 + i) - 0.5) * 2.2 * scale;
        let size = (2.9 + random(p.seed, 1111 + i) * 1.3) * scale;
        let proportions = match p.style.geology {
            Geology::Sandstone | Geology::Chalk => [1.40, 0.58, 0.66],
            Geology::Basalt => [0.95, 0.82, 0.83],
            _ => [0.87, 1.08, 0.81],
        };
        pieces.push(RockPiece {
            scree: false,
            center: [
                axis.cos() * along - axis.sin() * side,
                0.,
                axis.sin() * along + axis.cos() * side,
            ],
            size: proportions.map(|v| size * v),
            yaw: axis + (random(p.seed, 1121 + i) - 0.5) * 0.16,
            seed: hash(p.seed, i as i32, 1131),
        });
    }
    // Small detached chips collect along one side of the shared geological strike.
    for i in 0..4u32 {
        let along = (i as f32 - 1.5) * 2.3 * scale;
        let side = (3.2 + random(p.seed, 1141 + i) * 1.1) * scale;
        let size = (0.30 + random(p.seed, 1151 + i) * 0.66) * scale;
        pieces.push(RockPiece {
            scree: true,
            center: [
                axis.cos() * along - axis.sin() * side,
                0.,
                axis.sin() * along + axis.cos() * side,
            ],
            size: [
                size,
                size * (0.50 + random(p.seed, 1161 + i) * 0.38),
                size * 0.73,
            ],
            yaw: axis + (random(p.seed, 1171 + i) - 0.5) * 0.8,
            seed: hash(p.seed, i as i32, 1181),
        });
    }
    pieces
}
fn rock_blocks(p: Prop, x: f32, z: f32, padding: f32) -> bool {
    rock_pieces(p)
        .iter()
        .filter(|piece| !piece.scree)
        .any(|piece| {
            let dx = x - p.position[0] - piece.center[0];
            let dz = z - p.position[2] - piece.center[2];
            let a = dx * piece.yaw.cos() + dz * piece.yaw.sin();
            let b = -dx * piece.yaw.sin() + dz * piece.yaw.cos();
            // The bevel blocks have almost rectangular fracture faces. A rounded
            // rectangle encloses their real footprint without filling cluster gaps.
            let ex = (a.abs() - piece.size[0] * 1.07).max(0.);
            let ez = (b.abs() - piece.size[2] * 1.07).max(0.);
            ex * ex + ez * ez <= padding * padding
        })
}

#[cfg(test)]
mod character_asset_tests {
    use super::*;
    #[test]
    fn narrow_analytic_banks_cannot_spawn_submerged_tree_trunks() {
        let w = World::new(1337);
        let mut checked = 0;
        for z in 2920..2950 {
            for x in 535..560 {
                if let Some(p) = prop_at(&w, x, z) {
                    if matches!(
                        p.kind,
                        PropKind::Pine
                            | PropKind::Fir
                            | PropKind::Broadleaf
                            | PropKind::Birch
                            | PropKind::Willow
                            | PropKind::DeadTree
                    ) {
                        let s = w.natural_sample(p.position[0], p.position[2]);
                        if s.water_height > -999. {
                            assert!(p.position[1] >= s.water_height + 0.299);
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(checked > 3);
    }
    #[test]
    fn enlarged_formations_remain_grounded_bounded_and_deterministic() {
        use crate::regions::Geology;
        let w = World::new(1337);
        let mut max_near = 0;
        let mut max_far = 0;
        let mut max_radius = 0.0_f32;
        for geology in [
            Geology::Granite,
            Geology::Sandstone,
            Geology::Chalk,
            Geology::Basalt,
        ] {
            for seed in 0..16 {
                for lod in 0..=3 {
                    let [x, z] = [-16704.1, -12288.1];
                    let p = Prop {
                        position: [x, terrain_surface_height_lod(&w, x, z, lod) - 0.08, z],
                        scale: 1.52,
                        canopy: 1.,
                        seed,
                        kind: PropKind::Boulder,
                        biome: Biome::Moor,
                        style: PropStyle {
                            formation: true,
                            geology,
                            rockiness: 1.,
                            stone_axis: 0.71,
                            ..PropStyle::default()
                        },
                    };
                    let mut near = MeshData::default();
                    let mut far = MeshData::default();
                    landscape_rock(&mut near, p);
                    distant_rock(&mut far, p);
                    let before = near.vertices.clone();
                    let far_before = far.vertices.clone();
                    anchor_prop(&w, &mut near, 0, p, lod);
                    anchor_prop(&w, &mut far, 0, p, lod);
                    let radius = rock_radius(p);
                    assert!(radius < 20.0);
                    max_radius = max_radius.max(radius);
                    max_near = max_near.max(near.indices.len() / 3);
                    max_far = max_far.max(far.indices.len() / 3);
                    assert!(near.indices.len() / 3 <= 280);
                    assert!(far.indices.len() / 3 <= 36);
                    for (raw, mesh) in [(&before, &near), (&far_before, &far)] {
                        assert!(mesh
                            .indices
                            .iter()
                            .all(|&i| (i as usize) < mesh.vertices.len()));
                        for (a, b) in raw.iter().zip(&mesh.vertices) {
                            assert!(b
                                .position
                                .iter()
                                .chain(b.normal.iter())
                                .chain(b.color.iter())
                                .all(|v| v.is_finite()));
                            assert!((b.position[0] - x).hypot(b.position[2] - z) <= radius + 0.01);
                            if a.position[1] <= p.position[1] {
                                let height = terrain_surface_height_lod(
                                    &w,
                                    b.position[0],
                                    b.position[2],
                                    lod,
                                );
                                assert!(
                                    b.position[1] < height - 0.07,
                                    "formation foot floats at LOD{lod}"
                                );
                            }
                        }
                    }
                    let mut repeat = MeshData::default();
                    landscape_rock(&mut repeat, p);
                    anchor_prop(&w, &mut repeat, 0, p, lod);
                    assert_eq!(
                        bytemuck::cast_slice::<Vertex, u8>(&near.vertices),
                        bytemuck::cast_slice::<Vertex, u8>(&repeat.vertices)
                    );
                    for piece in rock_pieces(p).iter().filter(|p| !p.scree) {
                        let cx = x + piece.center[0];
                        let cz = z + piece.center[2];
                        assert!(rock_blocks(p, cx, cz, 0.35));
                        for i in 0..8 {
                            let angle = i as f32 * TAU / 8.;
                            let a = piece.size[0] * 0.9 * angle.cos();
                            let b = piece.size[2] * 0.9 * angle.sin();
                            assert!(rock_blocks(
                                p,
                                cx + a * piece.yaw.cos() - b * piece.yaw.sin(),
                                cz + a * piece.yaw.sin() + b * piece.yaw.cos(),
                                0.35
                            ));
                        }
                    }
                }
            }
        }
        println!("formation budgets: near {max_near}, far {max_far}, max enclosing radius {max_radius:.2}m");
    }
    #[test]
    fn actual_formations_clear_water_and_roads_and_are_found_across_cells() {
        let w = World::new(1337);
        let mut formations = 0;
        let mut neighbor_probes = 0;
        let mut first = None;
        for (x, z) in [
            (-74669., 70019.),
            (50031., 26519.),
            (-60169., 162819.),
            (-10869., 58419.),
        ] {
            let gx = (x / PROP_GRID).floor() as i32;
            let gz = (z / PROP_GRID).floor() as i32;
            for iz in gz - 48..=gz + 48 {
                for ix in gx - 48..=gx + 48 {
                    if !formation_cell(w.seed, ix, iz) {
                        continue;
                    }
                    let Some(p) = prop_at(&w, ix, iz) else {
                        continue;
                    };
                    if !p.style.formation {
                        continue;
                    }
                    let sites = w.sites_near(p.position[0], p.position[2], 70.);
                    let landmarks = w.landmarks_near(p.position[0], p.position[2], 40.);
                    if sites
                        .iter()
                        .any(|s| (s.x - p.position[0]).hypot(s.z - p.position[2]) < 45.)
                        || landmarks
                            .iter()
                            .any(|s| (s.x - p.position[0]).hypot(s.z - p.position[2]) < 15.)
                    {
                        continue;
                    }
                    formations += 1;
                    for piece in rock_pieces(p).iter().filter(|p| !p.scree) {
                        let x = p.position[0] + piece.center[0];
                        let z = p.position[2] + piece.center[2];
                        let sample = w.sample(x, z);
                        assert!(!sample.ocean && sample.water_height < sample.height - 0.8);
                        assert!(sample.road < 0.07, "formation blocks a road");
                        assert!(
                            blocks_player(&w, x, z),
                            "enlarged rock collider omitted its neighbor cell"
                        );
                        if (x / PROP_GRID).floor() as i32 != ix
                            || (z / PROP_GRID).floor() as i32 != iz
                        {
                            neighbor_probes += 1;
                        }
                    }
                    if first.is_none() {
                        first = Some([p.position[0], p.position[2]]);
                    }
                }
            }
        }
        assert!(
            formations > 10 && neighbor_probes > 10,
            "insufficient actual placement/collision coverage"
        );
        let [x, z] = first.unwrap();
        let now = std::time::Instant::now();
        for i in 0..100 {
            std::hint::black_box(blocks_player(&w, x + (i as f32 * 0.031), z));
        }
        println!("{formations} real formations, {neighbor_probes} neighboring-cell collider probes; representative [{x},{z}]; repeated collision mean {:.3}ms",now.elapsed().as_secs_f64()*10.);
    }
    #[test]
    fn broad_canopies_have_layers_and_birch_stems_stay_connected() {
        let p = Prop {
            position: [0.; 3],
            scale: 1.2,
            canopy: 1.,
            seed: 71,
            kind: PropKind::Broadleaf,
            biome: Biome::Forest,
            style: PropStyle {
                ancient: 1.,
                ..PropStyle::default()
            },
        };
        let model = tree_model(p);
        assert!(model.crowns.len() >= 6);
        assert!(model.crowns.iter().all(|c| c.radii[1] < c.radii[0] * 0.70));
        assert!(model
            .limbs
            .iter()
            .any(|l| l.start[1] < model.height * 0.3 && l.r0 > tree_radius(p) * 0.5));
        let birch = tree_model(Prop {
            kind: PropKind::Birch,
            ..p
        });
        let roots: Vec<_> = birch.limbs.iter().filter(|l| l.start[1] == 0.).collect();
        assert_eq!(roots.len(), 2);
        for root in roots {
            let continuation = birch
                .limbs
                .iter()
                .find(|l| l.start == root.end)
                .expect("clump trunk must continue to its crown");
            assert!(birch.crowns.iter().any(|c| c.center == continuation.end));
        }
    }
}

/// Actual generated outcrop centers for read-only camera/verification tools.
/// Uses the same placement and settlement exclusions as the rendered prop layer;
/// callers should query a local region, rather than scanning the whole continent.
pub fn formation_locations(world: &World, x: f32, z: f32, radius: f32) -> Vec<[f32; 3]> {
    let radius = radius.max(0.0);
    let sites = world.sites_near(x, z, radius + 50.);
    let landmarks = world.landmarks_near(x, z, radius + 20.);
    let mut found = Vec::new();
    for gz in ((z - radius) / PROP_GRID).floor() as i32..=((z + radius) / PROP_GRID).floor() as i32
    {
        for gx in
            ((x - radius) / PROP_GRID).floor() as i32..=((x + radius) / PROP_GRID).floor() as i32
        {
            if !formation_cell(world.seed, gx, gz) {
                continue;
            }
            let Some(p) = prop_at(world, gx, gz) else {
                continue;
            };
            if !p.style.formation || (p.position[0] - x).hypot(p.position[2] - z) > radius {
                continue;
            }
            if sites
                .iter()
                .any(|s| (s.x - p.position[0]).hypot(s.z - p.position[2]) < 45.)
                || landmarks
                    .iter()
                    .any(|s| (s.x - p.position[0]).hypot(s.z - p.position[2]) < 15.)
            {
                continue;
            }
            found.push(p.position);
        }
    }
    found
}
