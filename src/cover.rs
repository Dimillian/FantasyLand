//! Small, immutable ground-cover tiles; density only changes an instance prefix.
use crate::{
    ecology, geometry,
    world::{hash, rand01, ShoreKind, World},
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use serde::Serialize;
use std::{
    cell::Cell,
    collections::{HashMap, VecDeque},
};
use wgpu::util::DeviceExt;

pub const TILE_SIZE: f32 = 48.;
pub const COVER_DISTANCE: f32 = 300.;
const GRID_SIZE: usize = 11;
const HEIGHT_STEP: f32 = 6.;
const DIVISIONS: i32 = 32;
const VARIANTS: u32 = 8;
const TEMPLATE_KINDS: u32 = crate::plants::KINDS;
const TEMPLATE_VERTICES: u32 = crate::plants::TRIANGLES * 3;
const LOD_VERTICES: [u32; 3] = [72, 36, 24];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct CoverInstance {
    // Tile-local XZ and uniform scale; CPU sin/cos removes vertex shader trig.
    pub placement: [f32; 3],
    pub rotation: [f32; 2],
    // Packed RGB8 tint and template index.
    pub data: [u32; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TemplateVertex {
    position: [f32; 4],
    normal: [f32; 4],
    // W=1 multiplies the biome tint; W=0 preserves flower/heather color.
    color: [f32; 4],
    surface: [f32; 4], // UV, atlas layer, reserved
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileUniform {
    origin: [f32; 4],
    // World seed and signed global 6m grid origin, stored as bit patterns.
    grid: [u32; 4],
}
pub struct TileData {
    pub instances: Vec<CoverInstance>,
    pub ranks: Vec<f32>,
    pub meadow: Vec<crate::meadow::MeadowCell>,
    pub heights: Vec<f32>,
    pub origin: [f32; 2],
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
    pub seed: u32,
}
impl TileData {
    pub fn prefix_count(&self, density: f32) -> u32 {
        prefix_count(&self.ranks, density)
    }
    pub fn buffer_bytes(&self) -> u64 {
        (self.instances.len() * std::mem::size_of::<CoverInstance>()
            + self.meadow.len() * 16
            + self.heights.len() * 4
            + std::mem::size_of::<TileUniform>()) as u64
    }
}
fn random(seed: u32, salt: u32) -> f32 {
    rand01(hash(
        seed ^ salt.wrapping_mul(747796405),
        salt as i32,
        (seed >> 16) as i32,
    ))
}
fn prefix_count(ranks: &[f32], density: f32) -> u32 {
    if !density.is_finite() || density <= 0. {
        return 0;
    }
    if density >= 4. {
        return ranks.len() as u32;
    }
    ranks.partition_point(|&rank| rank < density) as u32
}
fn color_mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
fn pack_color(color: [f32; 3]) -> u32 {
    let c = color.map(|v| (v.clamp(0., 1.) * 255.).round() as u32);
    c[0] | c[1] << 8 | c[2] << 16
}
/// CPU equivalent of the vertex shader's exact LOD0 triangle lookup.
pub fn surface_height(seed: u32, origin: [f32; 2], heights: &[f32], local: [f32; 2]) -> f32 {
    let fx = ((local[0] + HEIGHT_STEP) / HEIGHT_STEP).clamp(0., 9.99999);
    let fz = ((local[1] + HEIGHT_STEP) / HEIGHT_STEP).clamp(0., 9.99999);
    let ix = fx.floor() as usize;
    let iz = fz.floor() as usize;
    let u = fx - ix as f32;
    let v = fz - iz as f32;
    let a = heights[iz * GRID_SIZE + ix];
    let b = heights[(iz + 1) * GRID_SIZE + ix];
    let c = heights[(iz + 1) * GRID_SIZE + ix + 1];
    let d = heights[iz * GRID_SIZE + ix + 1];
    let gx = (origin[0] / HEIGHT_STEP).round() as i32 - 1 + ix as i32;
    let gz = (origin[1] / HEIGHT_STEP).round() as i32 - 1 + iz as i32;
    if hash(seed, gx, gz) & 1 == 0 {
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
pub fn tile_data(world: &World, tx: i32, tz: i32) -> TileData {
    let origin = [tx as f32 * TILE_SIZE, tz as f32 * TILE_SIZE];
    let monuments = crate::natural::landmarks_near(world, origin[0] + 24., origin[1] + 24., 36.);
    let mut heights = Vec::with_capacity(GRID_SIZE * GRID_SIZE);
    for z in 0..GRID_SIZE {
        for x in 0..GRID_SIZE {
            heights.push(
                world
                    .sample(
                        origin[0] + (x as f32 - 1.) * HEIGHT_STEP,
                        origin[1] + (z as f32 - 1.) * HEIGHT_STEP,
                    )
                    .height,
            );
        }
    }
    let low = heights.iter().copied().fold(f32::INFINITY, f32::min);
    let high = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut plants = Vec::with_capacity(768);
    let mut meadow = Vec::with_capacity(1024);
    // Shared boundary samples cost one additional sample per cell, not sixteen
    // new world queries per GPU tuft. A carpet cell requires all four corners
    // to be dry, outside the road and on gentle terrain.
    let mut dry = [false; 33 * 33];
    for iz in 0..=32 {
        for ix in 0..=32 {
            let local = [ix as f32 * 1.5, iz as f32 * 1.5];
            let sample = world.sample(origin[0] + local[0], origin[1] + local[1]);
            let root = surface_height(world.seed, origin, &heights, local);
            dry[iz * 33 + ix] = crate::meadow::dry_ground(&sample, root);
        }
    }
    for iz in 0..DIVISIONS {
        for ix in 0..DIVISIONS {
            let seed = hash(
                world.seed ^ 0x47524153,
                tx * DIVISIONS + ix,
                tz * DIVISIONS + iz,
            );
            let local = [
                (ix as f32 + 0.12 + random(seed, 301) * 0.76) * 1.5,
                (iz as f32 + 0.12 + random(seed, 302) * 0.76) * 1.5,
            ];
            let x = origin[0] + local[0];
            let z = origin[1] + local[1];
            let sample = world.sample(x, z);
            let rendered_root = surface_height(world.seed, origin, &heights, local) - 0.018;
            if sample.ocean || sample.road > 0.10 || sample.water_height > sample.height + 0.15 {
                continue;
            }
            if sample.water_height > rendered_root + 0.12 {
                continue;
            }
            if monuments
                .iter()
                .any(|n| n.blocks(x, rendered_root, z, 0.65, 1.8))
            {
                continue;
            }
            let ecology = ecology::sample(world.seed, x, z, &sample);
            let dx = surface_height(world.seed, origin, &heights, [local[0] + 0.5, local[1]])
                - surface_height(world.seed, origin, &heights, [local[0] - 0.5, local[1]]);
            let dz = surface_height(world.seed, origin, &heights, [local[0], local[1] + 0.5])
                - surface_height(world.seed, origin, &heights, [local[0], local[1] - 0.5]);
            if dx * dx + dz * dz > 1.44 {
                continue;
            }
            let corner = iz as usize * 33 + ix as usize;
            if [corner, corner + 1, corner + 33, corner + 34]
                .iter()
                .all(|&i| dry[i])
                && monuments.iter().all(|n| {
                    !n.blocks(
                        origin[0] + (ix as f32 + 0.5) * 1.5,
                        rendered_root,
                        origin[1] + (iz as f32 + 0.5) * 1.5,
                        1.7,
                        1.8,
                    )
                })
            {
                if let Some(cell) = crate::meadow::cell(
                    seed,
                    ix as u32,
                    iz as u32,
                    &sample,
                    &ecology,
                    (dx * dx + dz * dz).sqrt(),
                ) {
                    meadow.push(cell);
                }
            }
            if random(seed, 304) > ecology.grass_density {
                continue;
            }
            let r = random(seed, 306);
            let kind = if r < ecology.ferns {
                2
            } else if r < ecology.ferns + ecology.flowers {
                3
            } else if r < ecology.ferns + ecology.flowers + ecology.heather {
                4
            } else if r < ecology.ferns + ecology.flowers + ecology.heather + ecology.seedheads {
                5
            } else if r < ecology.ferns
                + ecology.flowers
                + ecology.heather
                + ecology.seedheads
                + ecology.shrubs
            {
                6
            } else if r < ecology.ferns
                + ecology.flowers
                + ecology.heather
                + ecology.seedheads
                + ecology.shrubs
                + ecology.litter
            {
                7
            } else if r < ecology.ferns
                + ecology.flowers
                + ecology.heather
                + ecology.seedheads
                + ecology.shrubs
                + ecology.litter
                + ecology.reeds
            {
                1
            } else {
                0
            };
            let color = if kind == 5 {
                color_mix(ecology.cover_color, [0.64, 0.53, 0.29], 0.52)
            } else {
                ecology.cover_color
            };
            // A colony shares flower shape/palette; small variant changes avoid
            // copies without scattering unrelated species across every cell.
            let variant = if kind == 3 {
                match ecology.flower_group {
                    0 => hash(seed, 403, 409) % 3,
                    1 => 3 + hash(seed, 403, 409) % 3,
                    _ => 6 + hash(seed, 403, 409) % 2,
                }
            } else {
                hash(seed, 403, 409) % VARIANTS
            };
            let instance = CoverInstance {
                placement: [
                    local[0],
                    local[1],
                    (0.62 + random(seed, 305) * 0.80) * ecology.grass_height,
                ],
                rotation: {
                    let angle = random(seed, 401) * std::f32::consts::TAU;
                    [angle.sin(), angle.cos()]
                },
                data: [pack_color(color), kind * VARIANTS + variant],
            };
            plants.push((random(seed, 397) * 4., seed, instance));
        }
    }
    plants.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut instances = Vec::with_capacity(plants.len());
    let mut ranks = Vec::with_capacity(plants.len());
    for (rank, _, instance) in plants {
        ranks.push(rank);
        instances.push(instance);
    }
    TileData {
        instances,
        ranks,
        meadow,
        heights,
        origin,
        seed: world.seed,
        bounds_min: [origin[0] - 2.5, low - 0.1, origin[1] - 2.5],
        bounds_max: [
            origin[0] + TILE_SIZE + 2.5,
            high + 2.5,
            origin[1] + TILE_SIZE + 2.5,
        ],
    }
}

fn templates() -> Vec<TemplateVertex> {
    let mut result = Vec::with_capacity((TEMPLATE_KINDS * VARIANTS * TEMPLATE_VERTICES) as usize);
    for kind in 0..TEMPLATE_KINDS {
        for variant in 0..VARIANTS {
            let mesh = geometry::cover_template(kind, variant);
            assert!(mesh.vertices.len() <= TEMPLATE_VERTICES as usize);
            for vertex in &mesh.vertices {
                let tint = ((vertex.color[0] - vertex.color[1]).abs() < 0.0001
                    && (vertex.color[1] - vertex.color[2]).abs() < 0.0001)
                    as u32 as f32;
                result.push(TemplateVertex {
                    position: [
                        vertex.position[0],
                        vertex.position[1],
                        vertex.position[2],
                        1.,
                    ],
                    normal: [vertex.normal[0], vertex.normal[1], vertex.normal[2], 0.],
                    color: [vertex.color[0], vertex.color[1], vertex.color[2], tint],
                    surface: [vertex.uv[0], vertex.uv[1], vertex.texture, 0.],
                });
            }
            for _ in mesh.vertices.len()..TEMPLATE_VERTICES as usize {
                result.push(TemplateVertex::zeroed());
            }
        }
    }
    let near = result.clone();
    for lod in 1..=2 {
        for kind in 0..TEMPLATE_KINDS {
            for variant in 0..VARIANTS {
                let first = ((kind * VARIANTS + variant) * TEMPLATE_VERTICES) as usize;
                let source = &near[first..first + TEMPLATE_VERTICES as usize];
                let quads: &[usize] = match (kind, lod) {
                    (0, 1) => &[0, 2, 3, 5],
                    (0, _) => &[0, 3],
                    (1 | 5, 1) => &[0, 2, 4, 6, 7, 8],
                    (1 | 5, _) => &[0, 3, 6, 8],
                    (2, 1) => &[0, 1, 4, 5, 8, 9],
                    (2, _) => &[0, 1, 6, 7],
                    (3 | 4 | 6, 1) => &[0, 1, 2, 6, 7, 8],
                    (3 | 4 | 6, _) => &[0, 1, 2],
                    (_, 1) => &[0, 2, 4, 6],
                    (_, _) => &[0, 4],
                };
                for &q in quads {
                    result.extend_from_slice(&source[q * 6..q * 6 + 6]);
                }
                result.resize(
                    result.len() + TEMPLATE_VERTICES as usize - quads.len() * 6,
                    TemplateVertex::zeroed(),
                );
            }
        }
    }
    result
}
struct GpuTile {
    instances: wgpu::Buffer,
    group: wgpu::BindGroup,
    heights: wgpu::Buffer,
    uniform: wgpu::Buffer,
    meadow_cells: wgpu::Buffer,
    meadow_count: u32,
    ranks: Vec<f32>,
    lod: Cell<usize>,
    bounds_min: Vec3,
    bounds_max: Vec3,
    bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct CoverStats {
    pub tile_count: usize,
    pub loaded_instances: usize,
    pub buffer_bytes: u64,
    pub pending_tiles: usize,
    pub drawn_tiles: u32,
    pub drawn_instances: u32,
    pub drawn_triangles: u32,
}
pub struct CoverLayer {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    templates: wgpu::Buffer,
    template_bytes: u64,
    tiles: HashMap<(i32, i32), GpuTile>,
    pending: VecDeque<(i32, i32)>,
    center: Option<(i32, i32)>,
    drawn: Cell<(u32, u32, u32)>,
    meadow: crate::meadow::MeadowRenderer,
}
impl CoverLayer {
    pub fn new(
        device: &wgpu::Device,
        uniform_layout: &wgpu::BindGroupLayout,
        water_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Cover tile layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cover pipeline layout"),
            bind_group_layouts: &[uniform_layout, water_layout, &layout, material_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Instanced ground cover"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_cover"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CoverInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 12,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32x2,
                            offset: 20,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let data = templates();
        let template_bytes = (data.len() * std::mem::size_of::<TemplateVertex>()) as u64;
        let templates = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shared cover templates"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let meadow = crate::meadow::MeadowRenderer::new(device, &pipeline_layout, shader, format);
        Self {
            meadow,
            pipeline,
            layout,
            templates,
            template_bytes,
            tiles: HashMap::new(),
            pending: VecDeque::new(),
            center: None,
            drawn: Cell::new((0, 0, 0)),
        }
    }
    pub fn prepare(&mut self, position: Vec3) {
        let center = (
            (position.x / TILE_SIZE).floor() as i32,
            (position.z / TILE_SIZE).floor() as i32,
        );
        if self.center == Some(center) {
            return;
        }
        self.center = Some(center);
        let radius = (COVER_DISTANCE / TILE_SIZE).ceil() as i32 + 1;
        self.tiles.retain(|&(x, z), _| {
            (x - center.0).pow(2) + (z - center.1).pow(2) <= (radius + 1).pow(2)
        });
        let mut requested = Vec::new();
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if (dx * dx + dz * dz) as f32 > (radius as f32 + 0.5).powi(2) {
                    continue;
                }
                let key = (center.0 + dx, center.1 + dz);
                if !self.tiles.contains_key(&key) {
                    requested.push((dx * dx + dz * dz, key));
                }
            }
        }
        requested.sort_unstable_by_key(|&(distance, (x, z))| (distance, z, x));
        self.pending = requested.into_iter().map(|(_, key)| key).collect();
    }
    pub fn build_next(&mut self, world: &World, device: &wgpu::Device) -> bool {
        let Some((x, z)) = self.pending.pop_front() else {
            return false;
        };
        let data = tile_data(world, x, z);
        self.install(device, x, z, data);
        true
    }
    pub fn next_job(&mut self) -> Option<(i32, i32)> {
        self.pending.pop_front()
    }
    pub fn invalidate_requests(&mut self) {
        self.center = None;
    }
    pub fn install(&mut self, device: &wgpu::Device, x: i32, z: i32, data: TileData) {
        let bytes = data.buffer_bytes();
        // Empty tiles still count as complete so they are not regenerated on movement.
        let empty = CoverInstance::zeroed();
        let contents = if data.instances.is_empty() {
            bytemuck::bytes_of(&empty)
        } else {
            bytemuck::cast_slice(&data.instances)
        };
        let instances = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cover instances"),
            contents,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let heights = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cover terrain heights"),
            contents: bytemuck::cast_slice(&data.heights),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let uniform = TileUniform {
            origin: [data.origin[0], data.origin[1], 0., 0.],
            grid: [data.seed, (x * 8 - 1) as u32, (z * 8 - 1) as u32, 0],
        };
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cover tile origin"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let meadow_cells = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Ecological meadow cells"),
            contents: if data.meadow.is_empty() {
                &[0u8; 16]
            } else {
                bytemuck::cast_slice(&data.meadow)
            },
            usage: wgpu::BufferUsages::STORAGE,
        });
        self.meadow.remove((x, z));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cover tile"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.templates.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: heights.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        self.tiles.insert(
            (x, z),
            GpuTile {
                instances,
                group,
                heights,
                uniform,
                meadow_cells,
                meadow_count: data.meadow.len() as u32,
                ranks: data.ranks,
                lod: Cell::new(0),
                bounds_min: Vec3::from_array(data.bounds_min),
                bounds_max: Vec3::from_array(data.bounds_max),
                bytes: bytes
                    + if data.instances.is_empty() {
                        std::mem::size_of::<CoverInstance>() as u64
                    } else {
                        0
                    },
            },
        );
    }
    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        eye: Vec3,
        view_projection: Mat4,
        density: f32,
        materials: &'a wgpu::BindGroup,
    ) -> u32 {
        let mut drawn_tiles = 0;
        let mut drawn_instances = 0;
        let mut drawn_triangles = 0;
        if density > 0. && density.is_finite() {
            pass.set_pipeline(&self.pipeline);
            let mut ordered: Vec<_> = self
                .tiles
                .values()
                .map(|tile| {
                    (
                        (eye.clamp(tile.bounds_min, tile.bounds_max) - eye).length(),
                        tile,
                    )
                })
                .filter(|(d, t)| {
                    *d < COVER_DISTANCE && visible(t.bounds_min, t.bounds_max, eye, view_projection)
                })
                .collect();
            ordered.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            for (_, tile) in ordered {
                let nearest = eye.clamp(tile.bounds_min, tile.bounds_max);
                let distance = (nearest - eye).length();
                let count = prefix_count(&tile.ranks, density * distance_density(distance));
                if count == 0 || !visible(tile.bounds_min, tile.bounds_max, eye, view_projection) {
                    continue;
                }
                pass.set_bind_group(2, &tile.group, &[]);
                pass.set_bind_group(3, materials, &[]);
                pass.set_vertex_buffer(0, tile.instances.slice(..));
                let lod = cover_lod(distance, tile.lod.get());
                tile.lod.set(lod);
                let bank = lod as u32 * TEMPLATE_KINDS * VARIANTS * TEMPLATE_VERTICES;
                pass.draw(bank..bank + LOD_VERTICES[lod], 0..count);
                drawn_triangles += count * LOD_VERTICES[lod] / 3;
                drawn_tiles += 1;
                drawn_instances += count;
            }
        }
        self.drawn
            .set((drawn_tiles, drawn_instances, drawn_triangles));
        drawn_instances
    }
    pub fn encode_meadow(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        eye: Vec3,
        projection: Mat4,
        focal_pixels: f32,
        density: f32,
    ) {
        self.meadow
            .begin(device, queue, eye, projection, focal_pixels, density);
        if density > 0. {
            for (&key, tile) in &self.tiles {
                let distance = (eye.clamp(tile.bounds_min, tile.bounds_max) - eye).length();
                if tile.meadow_count > 0
                    && distance < crate::meadow::DISTANCE + 2.
                    && visible(tile.bounds_min, tile.bounds_max, eye, projection)
                {
                    self.meadow.prepare_tile(
                        device,
                        key,
                        tile.meadow_count,
                        &tile.meadow_cells,
                        &tile.heights,
                        &tile.uniform,
                        distance,
                    );
                }
            }
        }
        self.meadow.encode(encoder);
    }
    pub fn draw_meadow<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.meadow.draw(pass, |key| &self.tiles[&key].group);
    }
    pub fn clear(&mut self) {
        self.meadow.clear();
        self.tiles.clear();
        self.pending.clear();
        self.center = None;
        self.drawn.set((0, 0, 0));
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn stats(&self) -> CoverStats {
        let (drawn_tiles, drawn_instances, drawn_triangles) = self.drawn.get();
        CoverStats {
            tile_count: self.tiles.len(),
            loaded_instances: self.tiles.values().map(|t| t.ranks.len()).sum(),
            buffer_bytes: self.template_bytes
                + self.tiles.values().map(|t| t.bytes).sum::<u64>()
                + self.meadow.buffer_bytes(),
            pending_tiles: self.pending.len(),
            drawn_tiles,
            drawn_instances,
            drawn_triangles,
        }
    }
}
fn cover_lod(distance: f32, previous: usize) -> usize {
    match previous {
        0 if distance <= 108. => 0,
        1 if (92.0..=188.0).contains(&distance) => 1,
        2 if distance >= 172. => 2,
        _ if distance < 100. => 0,
        _ if distance < 180. => 1,
        _ => 2,
    }
}
fn distance_density(distance: f32) -> f32 {
    let t = ((distance - 80.) / (COVER_DISTANCE - 80.)).clamp(0., 1.);
    1.0 - t * t * (3.0 - 2.0 * t)
}
fn visible(min: Vec3, max: Vec3, eye: Vec3, projection: Mat4) -> bool {
    let nearest = eye.clamp(min, max);
    if (nearest - eye).length_squared() > COVER_DISTANCE * COVER_DISTANCE {
        return false;
    }
    let mut clips = [Vec4::ZERO; 8];
    for (i, clip) in clips.iter_mut().enumerate() {
        let p = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        *clip = projection * (p - eye).extend(1.);
    }
    !(clips.iter().all(|p| p.x < -p.w)
        || clips.iter().all(|p| p.x > p.w)
        || clips.iter().all(|p| p.y < -p.w)
        || clips.iter().all(|p| p.y > p.w)
        || clips.iter().all(|p| p.z < 0.)
        || clips.iter().all(|p| p.z > p.w))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distance_lods_preserve_foreground_and_taper_to_zero() {
        assert_eq!(distance_density(80.), 1.);
        assert_eq!(distance_density(COVER_DISTANCE), 0.);
        for d in 0..300 {
            assert!(distance_density(d as f32) >= distance_density(d as f32 + 1.));
        }
        assert_eq!(cover_lod(104., 0), 0);
        assert_eq!(cover_lod(96., 1), 1);
        assert_eq!(cover_lod(182., 1), 1);
        assert_eq!(cover_lod(175., 2), 2);
        let vertices = templates();
        let stride = (TEMPLATE_KINDS * VARIANTS * TEMPLATE_VERTICES) as usize;
        for kind in 0..TEMPLATE_KINDS {
            for variant in 0..VARIANTS {
                let source = geometry::cover_template(kind, variant);
                let start = ((kind * VARIANTS + variant) * TEMPLATE_VERTICES) as usize;
                for (v, original) in vertices[start..start + source.vertices.len()]
                    .iter()
                    .zip(&source.vertices)
                {
                    assert_eq!(&v.position[..3], &original.position);
                    assert_eq!(&v.surface[..2], &original.uv);
                }
                for lod in 1..=2 {
                    let bank = &vertices
                        [lod * stride + start..lod * stride + start + TEMPLATE_VERTICES as usize];
                    assert!(bank[LOD_VERTICES[lod] as usize..]
                        .iter()
                        .all(|v| v.position[3] == 0.));
                    // Every reduced triangle is still one of the original cards.
                    for v in bank.iter().filter(|v| v.position[3] > 0.5) {
                        assert!(vertices[start..start + TEMPLATE_VERTICES as usize]
                            .iter()
                            .any(|n| bytemuck::bytes_of(n) == bytemuck::bytes_of(v)));
                    }
                }
            }
        }
    }
    #[test]
    fn shoreline_plants_clear_the_rendered_water_surface() {
        let mut count = 0;
        for seed in [1337, 42] {
            let world = World::new(seed);
            let before = count;
            // Follow retained lake outlets: fixed coordinates cease to test a
            // shoreline when a terrain revision moves the drainage network.
            for lake in world.lakes().iter().filter(|l| l.surface < 650.).take(4) {
                let tx = (lake.outlet[0] / TILE_SIZE).floor() as i32;
                let tz = (lake.outlet[1] / TILE_SIZE).floor() as i32;
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        let tile = tile_data(&world, tx + dx, tz + dz);
                        for plant in &tile.instances {
                            let local = [plant.placement[0], plant.placement[1]];
                            let s =
                                world.sample(tile.origin[0] + local[0], tile.origin[1] + local[1]);
                            if s.water_height > -999. {
                                let floor =
                                    surface_height(seed, tile.origin, &tile.heights, local) - 0.018;
                                assert!(
                                    s.water_height - floor <= 0.121,
                                    "plant rooted below water in a triangulated bank"
                                );
                                count += 1;
                            }
                        }
                    }
                }
                if count - before > 60 {
                    break;
                }
            }
            assert!(
                count - before > 30,
                "no populated actual shore for seed{seed}"
            );
        }
        assert!(count > 30);
    }
    #[test]
    fn templates_and_instances_are_bounded() {
        assert_eq!(std::mem::size_of::<CoverInstance>(), 28);
        let vertices = templates();
        assert_eq!(
            vertices.len(),
            (3 * TEMPLATE_KINDS * VARIANTS * TEMPLATE_VERTICES) as usize
        );
        assert!(vertices.iter().all(|v| v
            .position
            .iter()
            .chain(v.normal.iter())
            .chain(v.color.iter())
            .all(|f| f.is_finite())));
        assert!(vertices.iter().all(|v| v.position[0].abs() < 2.
            && v.position[2].abs() < 2.
            && v.position[1] >= 0.
            && v.position[1] < 2.));
    }
    #[test]
    fn flower_groups_preserve_petals_and_seedhead_silhouettes() {
        let vertices = templates();
        for variant in 0..VARIANTS {
            let start = ((3 * VARIANTS + variant) * TEMPLATE_VERTICES) as usize;
            let flower = &vertices[start..start + TEMPLATE_VERTICES as usize];
            assert!(
                flower
                    .iter()
                    .any(|v| v.position[1] == 0.0 && v.color[3] == 1.0),
                "Flower stems must stay rooted and take biome tint"
            );
            let petals: Vec<_> = flower
                .iter()
                .filter(|v| v.color[3] == 0.0 && v.color[0] > 0.0)
                .collect();
            assert!(!petals.is_empty());
            if variant < 3 {
                assert!(petals.iter().any(|v| v.color[0] > 0.8 && v.color[1] > 0.8));
            } else if variant < 6 {
                assert!(petals.iter().any(|v| v.color[0] > 0.8 && v.color[2] < 0.3));
            } else {
                assert!(petals.iter().any(|v| v.color[2] > v.color[0] * 1.5));
                assert!(flower.iter().any(|v| v.position[1] > 0.9));
            }
            let start = ((5 * VARIANTS + variant) * TEMPLATE_VERTICES) as usize;
            assert!(vertices[start..start + TEMPLATE_VERTICES as usize]
                .iter()
                .any(|v| v.position[1] > 0.9 && v.color[3] == 0.0));
        }
    }

    #[test]
    fn live_prefix_is_nested_and_does_not_modify_data() {
        let world = World::new(1337);
        let p = world.spawn();
        let tx = (p[0] / TILE_SIZE).floor() as i32;
        let tz = (p[1] / TILE_SIZE).floor() as i32;
        let a = tile_data(&world, tx, tz);
        let b = tile_data(&world, tx, tz);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a.instances),
            bytemuck::cast_slice::<_, u8>(&b.instances)
        );
        assert_eq!(a.ranks, b.ranks);
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.prefix_count(0.), 0);
        assert_eq!(a.prefix_count(f32::NAN), 0);
        assert_eq!(a.prefix_count(4.), a.instances.len() as u32);
        let mut count = 0;
        for density in [0.1, 0.5, 1., 1.5, 2., 3., 4.] {
            let next = a.prefix_count(density);
            assert!(next >= count);
            count = next;
        }
        assert!(a.instances.len() <= 1024);
        assert!(a.instances.len() > 250);
        assert!(a.ranks.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(a.heights.len(), 121);
        assert_eq!(
            a.buffer_bytes(),
            (a.instances.len() * 28 + a.meadow.len() * 16 + 121 * 4 + 32) as u64
        );
    }
    #[test]
    fn blade_endpoints_match_actual_rendered_triangles_across_tile_and_chunk_seams() {
        let world = World::new(1337);
        let p = world.spawn();
        let tx = (p[0] / 192.).floor() as i32 * 4;
        let tz = (p[1] / 192.).floor() as i32 * 4;
        let templates = templates();
        let mut checked = 0;
        let mut crossed = 0;
        let mut worst = 0.0_f32;
        for (dx, dz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
            let tile = tile_data(&world, tx + dx, tz + dz);
            for instance in tile
                .instances
                .iter()
                .filter(|p| {
                    p.placement[0] < 2.
                        || p.placement[0] > 46.
                        || p.placement[1] < 2.
                        || p.placement[1] > 46.
                })
                .take(64)
            {
                let first = instance.data[1] as usize * TEMPLATE_VERTICES as usize;
                for v in templates[first..first + TEMPLATE_VERTICES as usize]
                    .iter()
                    .filter(|v| v.position[3] > 0.5)
                {
                    let [s, c] = instance.rotation;
                    let scale = instance.placement[2];
                    let x = tile.origin[0]
                        + instance.placement[0]
                        + (v.position[0] * c - v.position[2] * s) * scale;
                    let z = tile.origin[1]
                        + instance.placement[1]
                        + (v.position[0] * s + v.position[2] * c) * scale;
                    let local = [x - tile.origin[0], z - tile.origin[1]];
                    let height = surface_height(world.seed, tile.origin, &tile.heights, local);
                    let expected = geometry::terrain_surface_height_lod(&world, x, z, 0);
                    worst = worst.max((height - expected).abs());
                    assert!(
                        (height - expected).abs() < 0.002,
                        "surface mismatch at {x},{z}: {height} vs {expected}"
                    );
                    if local[0] < 0. || local[0] > 48. || local[1] < 0. || local[1] > 48. {
                        crossed += 1;
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000);
        assert!(crossed > 0);
        println!("anchors checked={checked} outside-own-tile={crossed} max_error={worst}");
    }
    #[test]
    fn generated_plants_preserve_water_roads_and_shore_communities() {
        let world = World::new(1337);
        let p = world.spawn();
        let tx = (p[0] / 48.).floor() as i32;
        let tz = (p[1] / 48.).floor() as i32;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let tile = tile_data(&world, tx + dx, tz + dz);
                for plant in tile.instances {
                    let s = world.sample(
                        tile.origin[0] + plant.placement[0],
                        tile.origin[1] + plant.placement[1],
                    );
                    assert!(!s.ocean && s.road <= 0.10 && s.water_height <= s.height + 0.15);
                    if s.shore != ShoreKind::None {
                        assert_eq!(
                            plant.data[1] / VARIANTS,
                            1,
                            "Only salt-tolerant tall grass is selected on shores"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn coastal_tiles_choose_salt_tolerant_grass_only() {
        let world = World::new(1337);
        let mut plants_checked = 0;
        for ray in 0..16 {
            let angle = ray as f32 * std::f32::consts::TAU / 16.0;
            let direction = [angle.sin(), angle.cos()];
            let mut inside = 0.0;
            let mut outside = 190000.0;
            for _ in 0..22 {
                let middle = (inside + outside) * 0.5;
                if world.landmass_id(direction[0] * middle, direction[1] * middle) == Some(0) {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            for inland in [12.0, 48.0, 100.0] {
                let p = [
                    direction[0] * (inside - inland),
                    direction[1] * (inside - inland),
                ];
                let tile = tile_data(
                    &world,
                    (p[0] / TILE_SIZE).floor() as i32,
                    (p[1] / TILE_SIZE).floor() as i32,
                );
                for plant in &tile.instances {
                    let terrain = world.sample(
                        tile.origin[0] + plant.placement[0],
                        tile.origin[1] + plant.placement[1],
                    );
                    assert!(!terrain.ocean);
                    if terrain.shore != ShoreKind::None {
                        assert_eq!(plant.data[1] / VARIANTS, 0);
                        assert!(
                            terrain.height > 2.0 && terrain.water_height < terrain.height - 0.3
                        );
                        plants_checked += 1;
                    }
                }
            }
        }
        assert!(
            plants_checked > 30,
            "coastal grass candidates checked {plants_checked}"
        );
    }

    #[test]
    fn culling_rejects_far_tiles_and_keeps_near_crossing_boxes() {
        let eye = Vec3::new(0., 2., 0.);
        let projection = Mat4::perspective_rh(1.3, 1.5, 0.08, 2000.);
        assert!(visible(
            Vec3::new(-3., -2., -30.),
            Vec3::new(3., 4., -20.),
            eye,
            projection
        ));
        assert!(visible(
            Vec3::new(-3., -2., -COVER_DISTANCE - 5.),
            Vec3::new(3., 4., -COVER_DISTANCE + 5.),
            eye,
            projection
        ));
        assert!(!visible(
            Vec3::new(-3., -2., -COVER_DISTANCE - 30.),
            Vec3::new(3., 4., -COVER_DISTANCE - 20.),
            eye,
            projection
        ));
        assert!(!visible(
            Vec3::new(90., -2., -20.),
            Vec3::new(100., 4., -10.),
            eye,
            projection
        ));
        assert!(!visible(
            Vec3::new(-3., -2., 20.),
            Vec3::new(3., 4., 30.),
            eye,
            projection
        ));
    }
}
