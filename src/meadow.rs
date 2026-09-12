//! Continuous near-field sward. Workers send ecological cells; the GPU expands
//! only visible, nearby tufts. Flowers and ferns remain independent cover accents.
use crate::{
    ecology::Ecology,
    world::{Biome, Sample, ShoreKind},
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub const DISTANCE: f32 = 52.;
const INSTANCES_PER_CELL: u32 = 64;
const INSTANCE_BYTES: u64 = 32;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeadowCell {
    // Seed, RGB8 pigment, quantized height/coverage, grid index + 16-bit mask.
    pub data: [u32; 4],
}
impl MeadowCell {
    pub fn valid(&self) -> bool {
        self.data[3] & 65535 < 1024
            && self.data[3] >> 16 != 0
            && self.data[2] & 255 != 0
            && self.data[2] >> 8 <= 255
    }
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn dry_ground(s: &Sample, root: f32) -> bool {
    !s.ocean
        && s.shore == ShoreKind::None
        && s.road < 0.015
        && s.water_height < s.height - 0.22
        && s.water_height < root - 0.16
}
pub fn cell(
    seed: u32,
    ix: u32,
    iz: u32,
    s: &Sample,
    e: &Ecology,
    slope: f32,
) -> Option<MeadowCell> {
    if s.ocean || s.shore != ShoreKind::None {
        return None;
    }
    let open = 1. - smooth(0.08, 0.75, e.tree_density);
    let vigor = smooth(0.10, 0.70, e.grass_density);
    let slope_factor = 1. - smooth(0.45, 1.10, slope);
    let coverage = match s.biome {
        Biome::Grassland | Biome::Forest => {
            let shaded = 0.30 + 0.20 * vigor;
            let meadow = 0.90 + 0.10 * vigor;
            (shaded + (meadow - shaded) * open) * (1. - 0.20 * e.litter)
        }
        Biome::PineForest => {
            (0.14 + 0.14 * vigor + open * (0.62 + 0.10 * vigor)) * (1. - 0.35 * e.litter)
        }
        Biome::Wetland => 0.83 + 0.17 * vigor,
        Biome::Moor => 0.52 + 0.24 * vigor,
        Biome::Alpine | Biome::Desert => 0.,
    } * slope_factor
        * (1. - e.undergrowth * 0.38 - e.ferns * 0.14);
    if coverage < 0.04 {
        return None;
    }
    let height = match s.biome {
        Biome::PineForest => 0.09 + 0.13 * open + 0.025 * vigor,
        Biome::Wetland => 0.18 + 0.10 * vigor,
        Biome::Moor => 0.14 + 0.08 * vigor,
        _ => 0.12 + 0.16 * open + 0.03 * vigor,
    };
    let height = (height * e.grass_height).clamp(0.07, 0.34);
    let c = e
        .cover_color
        .map(|v| (v.clamp(0., 1.) * 255.).round() as u32);
    Some(MeadowCell {
        data: [
            seed,
            c[0] | c[1] << 8 | c[2] << 16,
            (height * 255.).round() as u32 | ((coverage.clamp(0., 1.) * 255.).round() as u32) << 8,
            ix + iz * 32 | 0xffff0000,
        ],
    })
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Frame {
    projection: [[f32; 4]; 4],
    eye: [f32; 4],
    settings: [f32; 4],
}
struct Tile {
    output: wgpu::Buffer,
    indirect: wgpu::Buffer,
    group: wgpu::BindGroup,
    count: u32,
    bytes: u64,
    frame: u64,
    distance: f32,
}
pub struct MeadowRenderer {
    pipeline: wgpu::RenderPipeline,
    compute: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    frame_buffer: wgpu::Buffer,
    tiles: HashMap<(i32, i32), Tile>,
    frame: u64,
}
impl MeadowRenderer {
    pub fn new(
        device: &wgpu::Device,
        render_layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        format: wgpu::TextureFormat,
    ) -> Self {
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Opaque meadow ribbons"),
            layout: Some(render_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_meadow"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x4,1=>Uint32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_meadow"),
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
        let entries: Vec<_> = (0..6)
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: if i == 0 || i == 3 {
                        wgpu::BufferBindingType::Uniform
                    } else {
                        wgpu::BufferBindingType::Storage { read_only: i < 4 }
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            })
            .collect();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Meadow GPU expansion"),
            entries: &entries,
        });
        let compute_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Meadow compute layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Cull and compact ecological meadow"),
            source: wgpu::ShaderSource::Wgsl(include_str!("meadow_compute.wgsl").into()),
        });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Expand visible meadow cells"),
            layout: Some(&compute_layout),
            module: &compute_shader,
            entry_point: Some("expand"),
            compilation_options: Default::default(),
            cache: None,
        });
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Meadow view"),
            size: std::mem::size_of::<Frame>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            compute,
            layout,
            frame_buffer,
            tiles: HashMap::new(),
            frame: 0,
        }
    }
    pub fn begin(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        eye: Vec3,
        projection: Mat4,
        focal_pixels: f32,
        density: f32,
    ) {
        self.frame = self.frame.wrapping_add(1);
        queue.write_buffer(
            &self.frame_buffer,
            0,
            bytemuck::bytes_of(&Frame {
                projection: projection.to_cols_array_2d(),
                eye: eye.extend(1.).to_array(),
                settings: [density.clamp(0., 4.) / 4., DISTANCE, focal_pixels, 0.],
            }),
        );
    }
    pub fn prepare_tile(
        &mut self,
        device: &wgpu::Device,
        key: (i32, i32),
        count: u32,
        cells: &wgpu::Buffer,
        heights: &wgpu::Buffer,
        uniform: &wgpu::Buffer,
        distance: f32,
    ) {
        let tile = self.tiles.entry(key).or_insert_with(|| {
            let bytes = count as u64 * INSTANCES_PER_CELL as u64 * INSTANCE_BYTES;
            let output = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Visible meadow instances"),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            });
            let indirect = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Meadow indirect draw"),
                contents: bytemuck::cast_slice(&[12u32, 0, 0, 0]),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::INDIRECT
                    | wgpu::BufferUsages::COPY_DST,
            });
            let buffers = [
                &self.frame_buffer,
                cells,
                heights,
                uniform,
                &output,
                &indirect,
            ];
            let entries: Vec<_> = buffers
                .into_iter()
                .enumerate()
                .map(|(i, b)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource: b.as_entire_binding(),
                })
                .collect();
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Nearby meadow tile"),
                layout: &self.layout,
                entries: &entries,
            });
            Tile {
                output,
                indirect,
                group,
                count,
                bytes: bytes + 16,
                frame: 0,
                distance,
            }
        });
        tile.frame = self.frame;
        tile.distance = distance;
    }
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) {
        // Keep nearby allocations briefly through turns; never retain the full
        // 300m accent-cover area's expanded blades in GPU memory.
        self.tiles
            .retain(|_, t| self.frame.wrapping_sub(t.frame) < 120);
        for t in self.tiles.values().filter(|t| t.frame == self.frame) {
            encoder.clear_buffer(&t.indirect, 4, Some(4));
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Meadow distance and frustum culling"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.compute);
        for t in self.tiles.values().filter(|t| t.frame == self.frame) {
            pass.set_bind_group(0, &t.group, &[]);
            pass.dispatch_workgroups((t.count * INSTANCES_PER_CELL).div_ceil(64), 1, 1);
        }
    }
    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        group: impl Fn((i32, i32)) -> &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);
        let mut visible: Vec<_> = self
            .tiles
            .iter()
            .filter(|(_, t)| t.frame == self.frame)
            .collect();
        visible.sort_unstable_by(|a, b| a.1.distance.total_cmp(&b.1.distance));
        for (&key, t) in visible {
            pass.set_bind_group(2, group(key), &[]);
            pass.set_vertex_buffer(0, t.output.slice(..));
            pass.draw_indirect(&t.indirect, 0);
        }
    }
    pub fn remove(&mut self, key: (i32, i32)) {
        self.tiles.remove(&key);
    }
    pub fn clear(&mut self) {
        self.tiles.clear();
    }
    pub fn buffer_bytes(&self) -> u64 {
        self.tiles.values().map(|t| t.bytes).sum::<u64>() + 96
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cover, world::World};
    #[test]
    fn sward_is_continuous_below_accents_and_avoids_roads_water_and_cliffs() {
        let world = World::new(1337);
        let mut lush = 0;
        for (x, z) in [(-57269., 20719.), (-10879., 58547.), (9499.871, 2954.6006)] {
            let tx = (x / 48.0_f32).floor() as i32;
            let tz = (z / 48.0_f32).floor() as i32;
            let tile = cover::tile_data(&world, tx, tz);
            let repeat = cover::tile_data(&world, tx, tz);
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&tile.meadow),
                bytemuck::cast_slice::<_, u8>(&repeat.meadow)
            );
            assert!(tile.meadow.len() <= 1024);
            for c in &tile.meadow {
                assert!(c.valid());
                let index = c.data[3] & 65535;
                let start = [(index % 32) as f32 * 1.5, (index / 32) as f32 * 1.5];
                for iz in 0..4 {
                    for ix in 0..4 {
                        let local = [
                            start[0] + (ix as f32 + 0.5) * 0.375,
                            start[1] + (iz as f32 + 0.5) * 0.375,
                        ];
                        let s = world.sample(tile.origin[0] + local[0], tile.origin[1] + local[1]);
                        let root = cover::surface_height(1337, tile.origin, &tile.heights, local);
                        assert!(!s.ocean && s.road < 0.02 && s.water_height < root - 0.1);
                        assert!(!matches!(s.biome, Biome::Desert | Biome::Alpine));
                    }
                }
                if c.data[2] >> 8 > 220 {
                    lush += 1;
                }
            }
            if x == -57269. {
                assert!(
                    tile.meadow.len() > 900,
                    "meadow must not inherit sparse accent occupancy"
                );
                assert!(tile.meadow.len() > tile.instances.len());
            }
        }
        assert!(lush > 800, "healthy meadow cells keep at least14of16tufts");
    }
    #[test]
    fn transfer_cell_rejects_out_of_range_fields() {
        assert!(MeadowCell {
            data: [0, 0, 128 | 255 << 8, 0xffff0000]
        }
        .valid());
        for data in [
            [0, 0, 128 | 255 << 8, 0xffff0400],
            [0, 0, 128, 0],
            [0, 0, 0, 0xffff0000],
            [0, 0, 65536, 0xffff0000],
        ] {
            assert!(!MeadowCell { data }.valid());
        }
    }
}
