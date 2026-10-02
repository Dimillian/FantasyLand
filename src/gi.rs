//! Local, one-bounce diffuse lighting. Geometry generation runs in the existing
//! world worker; the GPU updates a bounded number of probes, independently of
//! the main render resolution. No hardware ray tracing or native-only features.
use crate::{geometry, world::World};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub const CELL_SIZE: f32 = 0.5;
pub const VOXEL_DIMS: [u32; 3] = [128, 48, 128];
pub const PROBE_DIMS: [u32; 3] = [16, 6, 16];
pub const PROBE_COUNT: u32 = PROBE_DIMS[0] * PROBE_DIMS[1] * PROBE_DIMS[2];
pub const VOXEL_COUNT: usize = (VOXEL_DIMS[0] * VOXEL_DIMS[1] * VOXEL_DIMS[2]) as usize;
pub const SAMPLING_SHADER: &str = concat!(
    include_str!("gi_common.wgsl"),
    "\n",
    include_str!("gi_sampling.wgsl")
);
const COMMON_SHADER: &str = include_str!("gi_common.wgsl");
const COMPUTE_SHADER: &str = include_str!("gi_compute.wgsl");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VolumeKey {
    pub origin: [f32; 3],
}
impl VolumeKey {
    pub fn for_eye(eye: [f32; 3]) -> Self {
        Self {
            origin: [
                (eye[0] / 8.).floor() * 8. - 32.,
                (eye[1] / 4.).floor() * 4. - 8.,
                (eye[2] / 8.).floor() * 8. - 32.,
            ],
        }
    }
    pub fn valid(self) -> bool {
        self.origin
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
    }
}

/// Packed linear RGB8, dominant normal axis (3 bits), opacity (4 bits).
/// Zero is empty. Foliage is deliberately porous; water/glass/flames are omitted.
#[derive(Clone)]
pub struct ProxyData {
    pub key: VolumeKey,
    pub voxels: Vec<u32>,
}
impl ProxyData {
    pub fn empty(key: VolumeKey) -> Self {
        Self {
            key,
            voxels: vec![0; VOXEL_COUNT],
        }
    }
    fn index(&self, point: Vec3) -> Option<usize> {
        let p = (point - Vec3::from_array(self.key.origin)) / CELL_SIZE;
        let c = p.floor().as_ivec3();
        (c.x >= 0
            && c.y >= 0
            && c.z >= 0
            && c.x < VOXEL_DIMS[0] as i32
            && c.y < VOXEL_DIMS[1] as i32
            && c.z < VOXEL_DIMS[2] as i32)
            .then_some(
                (c.x + c.y * VOXEL_DIMS[0] as i32 + c.z * (VOXEL_DIMS[0] * VOXEL_DIMS[1]) as i32)
                    as usize,
            )
    }
    pub fn occupied(&self, point: [f32; 3]) -> bool {
        self.index(Vec3::from_array(point))
            .is_some_and(|i| self.voxels[i] >> 27 > 0)
    }
    /// Rasterize on the dominant plane, with conservative edge coverage. This
    /// preserves architectural apertures instead of filling building AABBs.
    pub fn voxelize(&mut self, mesh: &geometry::MeshData) {
        let origin = Vec3::from_array(self.key.origin);
        let extent = Vec3::new(64., 24., 64.);
        for triangle in mesh.indices.chunks_exact(3) {
            let vertices =
                [triangle[0], triangle[1], triangle[2]].map(|i| mesh.vertices[i as usize]);
            let material = vertices[0].material;
            if (3.5..4.5).contains(&material)
                || (7.5..8.5).contains(&material)
                || (9.5..10.5).contains(&material)
                || (vertices[0].texture >= 200.)
            {
                continue;
            }
            let a = Vec3::from_array(vertices[0].position);
            let b = Vec3::from_array(vertices[1].position);
            let c = Vec3::from_array(vertices[2].position);
            let lo = a.min(b).min(c) - origin;
            let hi = a.max(b).max(c) - origin;
            if lo.cmpge(extent).any() || hi.cmplt(Vec3::ZERO).any() {
                continue;
            }
            let raw = (b - a).cross(c - a);
            if raw.length_squared() < 1e-10 {
                continue;
            }
            let n = raw.normalize();
            let axis = if n.x.abs() >= n.y.abs() && n.x.abs() >= n.z.abs() {
                0
            } else if n.y.abs() >= n.z.abs() {
                1
            } else {
                2
            };
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            let normal_code = (axis * 2 + usize::from(n[axis] < 0.)) as u32;
            let porous = (0.5..1.5).contains(&material)
                || (5.5..6.5).contains(&material)
                || (8.5..9.5).contains(&material);
            let opacity = if porous { 5 } else { 15 };
            let color: [u32; 3] = std::array::from_fn(|i| {
                let gamma = ((vertices[0].color[i] + vertices[1].color[i] + vertices[2].color[i])
                    / 3.)
                    .clamp(0., 1.);
                (gamma.powf(2.2) * 255.).round() as u32
            });
            let packed =
                color[0] | color[1] << 8 | color[2] << 16 | normal_code << 24 | opacity << 27;
            let min_u = (lo[u] / CELL_SIZE).floor().max(0.) as u32;
            let max_u = (hi[u] / CELL_SIZE).floor().min(VOXEL_DIMS[u] as f32 - 1.) as u32;
            let min_v = (lo[v] / CELL_SIZE).floor().max(0.) as u32;
            let max_v = (hi[v] / CELL_SIZE).floor().min(VOXEL_DIMS[v] as f32 - 1.) as u32;
            for iv in min_v..=max_v {
                for iu in min_u..=max_u {
                    let mut p = Vec3::ZERO;
                    p[u] = origin[u] + (iu as f32 + 0.5) * CELL_SIZE;
                    p[v] = origin[v] + (iv as f32 + 0.5) * CELL_SIZE;
                    p[axis] = a[axis] - (n[u] * (p[u] - a[u]) + n[v] * (p[v] - a[v])) / n[axis];
                    // A half-cell square can touch an edge even when its centre
                    // is outside; project that extent on each edge normal.
                    let inside = [(a, b), (b, c), (c, a)].iter().all(|&(start, end)| {
                        let e = end - start;
                        let signed = e[u] * (p[v] - start[v]) - e[v] * (p[u] - start[u]);
                        let tolerance = (e[u].abs() + e[v].abs()) * CELL_SIZE * 0.48;
                        signed * n[axis].signum() >= -tolerance
                    });
                    if inside {
                        if let Some(index) = self.index(p) {
                            // Solid walls win over crossing foliage cards.
                            if packed >> 27 >= self.voxels[index] >> 27 {
                                self.voxels[index] = packed;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Worker-safe proxy generation. Surface geometry, including apertures and
/// partitions, comes from the same recipes as the visible world. The reflection
/// mesh uses cheap tree silhouettes and excludes grass without changing buildings.
pub fn generate_static_proxy(world: &World, key: VolumeKey) -> ProxyData {
    ProxyCache::default().static_proxy(world, key)
}

/// A bounded worker/native cache avoids rebuilding procedural trees/buildings
/// when the probe volume moves by eight metres or a door changes angle.
#[derive(Default)]
pub struct ProxyCache {
    seed: Option<u32>,
    chunks: Vec<((i32, i32), geometry::MeshData, geometry::MeshData)>,
    static_data: Option<ProxyData>,
}
impl ProxyCache {
    pub fn generate(&mut self, world: &World, key: VolumeKey, doors: &[(u32, f32)]) -> ProxyData {
        let mut data = self.static_proxy(world, key);
        add_doors(world, &mut data, doors);
        data
    }
    fn static_proxy(&mut self, world: &World, key: VolumeKey) -> ProxyData {
        if self.seed != Some(world.seed) {
            self.chunks.clear();
            self.static_data = None;
            self.seed = Some(world.seed);
        }
        if let Some(data) = &self.static_data {
            if data.key == key {
                return data.clone();
            }
        }
        let mut data = ProxyData::empty(key);
        if !key.valid() {
            return data;
        }
        // Include roofs/crowns whose owners lie outside the volume itself.
        let min_x = ((key.origin[0] - 24.) / geometry::CHUNK_SIZE).floor() as i32;
        let min_z = ((key.origin[2] - 24.) / geometry::CHUNK_SIZE).floor() as i32;
        let max_x = ((key.origin[0] + 88.) / geometry::CHUNK_SIZE).floor() as i32;
        let max_z = ((key.origin[2] + 88.) / geometry::CHUNK_SIZE).floor() as i32;
        self.chunks
            .retain(|((x, z), _, _)| *x >= min_x && *x <= max_x && *z >= min_z && *z <= max_z);
        for z in min_z..=max_z {
            for x in min_x..=max_x {
                if !self
                    .chunks
                    .iter()
                    .any(|(coordinate, _, _)| *coordinate == (x, z))
                {
                    self.chunks.push((
                        (x, z),
                        geometry::terrain_chunk(world, x, z, 0),
                        geometry::reflection_props_chunk(world, x, z, 0),
                    ));
                }
                let (_, terrain, props) = self
                    .chunks
                    .iter()
                    .find(|(coordinate, _, _)| *coordinate == (x, z))
                    .unwrap();
                data.voxelize(terrain);
                data.voxelize(props);
            }
        }
        self.static_data = Some(data.clone());
        data
    }
}
pub fn add_doors(world: &World, data: &mut ProxyData, doors: &[(u32, f32)]) {
    let key = data.key;
    let mut mesh = geometry::MeshData::default();
    for layout in
        world
            .settlements
            .layouts_near(world, key.origin[0] + 32., key.origin[2] + 32., 65.)
    {
        for building in &layout.buildings {
            if building.door {
                let angle = doors
                    .iter()
                    .find(|(id, _)| *id == building.id)
                    .map_or(0., |(_, a)| a.clamp(0., 1.));
                crate::settlement_mesh::door_mesh(&mut mesh, building, angle);
            }
        }
    }
    data.voxelize(&mesh);
}
pub fn generate_proxy(world: &World, key: VolumeKey, doors: &[(u32, f32)]) -> ProxyData {
    let mut data = generate_static_proxy(world, key);
    add_doors(world, &mut data, doors);
    data
}

pub fn encode_proxy(data: &ProxyData) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32 + VOXEL_COUNT * 4);
    bytes.extend(0x46474931u32.to_le_bytes());
    bytes.extend(bytemuck::cast_slice(&data.key.origin));
    bytes.extend(bytemuck::cast_slice(&VOXEL_DIMS));
    bytes.extend(CELL_SIZE.to_le_bytes());
    bytes.extend(bytemuck::cast_slice(&data.voxels));
    bytes
}
pub fn decode_proxy(bytes: &[u8]) -> Option<ProxyData> {
    if bytes.len() != 32 + VOXEL_COUNT * 4
        || u32::from_le_bytes(bytes[..4].try_into().ok()?) != 0x46474931
    {
        return None;
    }
    let key = VolumeKey {
        origin: std::array::from_fn(|i| {
            f32::from_le_bytes(bytes[4 + i * 4..8 + i * 4].try_into().unwrap())
        }),
    };
    if !key.valid() {
        return None;
    }
    for (i, dim) in VOXEL_DIMS.iter().enumerate() {
        if u32::from_le_bytes(bytes[16 + i * 4..20 + i * 4].try_into().ok()?) != *dim {
            return None;
        }
    }
    if f32::from_le_bytes(bytes[28..32].try_into().ok()?) != CELL_SIZE {
        return None;
    }
    let voxels = bytes[32..]
        .chunks_exact(4)
        .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
        .collect::<Vec<_>>();
    if voxels
        .iter()
        .any(|v| (v >> 24) & 7 > 5 || v & 0x8000_0000 != 0)
    {
        return None;
    }
    Some(ProxyData { key, voxels })
}

#[derive(Clone, Copy, Debug)]
pub struct Lighting {
    pub sun: [f32; 3],
    pub direct: [f32; 3],
    pub ambient: [f32; 3],
    pub sun_strength: f32,
    pub hearths: [[f32; 4]; 8],
}
impl Lighting {
    fn changed(self, previous: Self) -> bool {
        Vec3::from_array(self.sun).dot(Vec3::from_array(previous.sun)) < 0.9998
            || self
                .direct
                .iter()
                .zip(previous.direct)
                .any(|(a, b)| (a - b).abs() > 0.008)
            || self
                .ambient
                .iter()
                .zip(previous.ambient)
                .any(|(a, b)| (a - b).abs() > 0.008)
            || (self.sun_strength - previous.sun_strength).abs() > 0.008
            || self.hearths != previous.hearths
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    origin_cell: [f32; 4],
    voxel_dims: [u32; 4],
    probe_dims: [u32; 4],
    parameters: [f32; 4], // spacing, enabled, replacement blend, reserved
    sun: [f32; 4],
    direct: [f32; 4],
    ambient: [f32; 4],
    update: [u32; 4], // first index, count, retained-history, reserved
    scroll: [i32; 4], // source grid offset, retain overlapping volume
    hearths: [[f32; 4]; 8],
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Probe {
    position: [f32; 4], // position relative to volume origin; W is valid
    sh0: [f32; 4],
    shx: [f32; 4],
    shy: [f32; 4],
    shz: [f32; 4],
    distance0: [f32; 4], // +X, -X, +Y, -Y first-hit means
    distance1: [f32; 4], // +Z, -Z, reserved
    moment0: [f32; 4],
    moment1: [f32; 4],
}

pub struct GiGpu {
    pub uniform_buffer: wgpu::Buffer,
    pub probe_buffer: wgpu::Buffer,
    voxels: wgpu::Buffer,
    previous_probes: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
    scroll_pipeline: wgpu::ComputePipeline,
    key: Option<VolumeKey>,
    pending_scroll: Option<[i32; 4]>,
    last_lighting: Option<Lighting>,
    next_probe: u32,
    remaining: u32,
    initialized: u32,
    uniform: Uniform,
    published_uniform: Option<Uniform>,
}

fn compose_pending_scroll(
    previous: Option<VolumeKey>,
    next: VolumeKey,
    pending: Option<[i32; 4]>,
) -> [i32; 4] {
    let Some(previous) = previous else {
        return [0; 4];
    };
    let mut offset = [0; 4];
    for (i, value) in offset[..3].iter_mut().enumerate() {
        *value = ((next.origin[i] - previous.origin[i]) / 4.).round() as i32;
        if let Some(pending) = pending {
            *value += pending[i];
        }
    }
    // Until update() dispatches, the probes still belong to the origin before
    // the first queued upload. Compose moves against that origin, and never
    // let a later same-origin upload undo a fresh-volume/full-clear request.
    offset[3] = i32::from(
        pending.is_none_or(|pending| pending[3] != 0)
            && (0..3).all(|i| offset[i].abs() < PROBE_DIMS[i] as i32),
    );
    offset
}

impl GiGpu {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Local GI parameters"),
            contents: bytemuck::bytes_of(&Uniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                mapped_at_creation: false,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
            })
        };
        let voxels = buffer("Local GI packed geometry", (VOXEL_COUNT * 4) as u64);
        let probe_buffer = buffer(
            "Local GI directional irradiance",
            PROBE_COUNT as u64 * std::mem::size_of::<Probe>() as u64,
        );
        let previous_probes = buffer("Local GI scroll history", probe_buffer.size());
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Local GI compute layout"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
                entry(2, wgpu::BufferBindingType::Storage { read_only: false }),
                entry(3, wgpu::BufferBindingType::Storage { read_only: true }),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Local GI compute resources"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: voxels.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: probe_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: previous_probes.as_entire_binding(),
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Local GI voxel ray marching"),
            source: wgpu::ShaderSource::Wgsl(format!("{COMMON_SHADER}\n{COMPUTE_SHADER}").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Local GI compute"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            uniform_buffer,
            probe_buffer,
            voxels,
            previous_probes,
            bind_group,
            pipeline: pipeline("update_probes"),
            scroll_pipeline: pipeline("scroll_probes"),
            key: None,
            pending_scroll: None,
            last_lighting: None,
            next_probe: 0,
            remaining: 0,
            initialized: 0,
            uniform: Uniform::zeroed(),
            published_uniform: None,
        }
    }
    pub fn key(&self) -> Option<VolumeKey> {
        self.key
    }
    pub fn ready_fraction(&self) -> f32 {
        self.initialized as f32 / PROBE_COUNT as f32
    }
    pub fn buffer_bytes(&self) -> u64 {
        self.voxels.size()
            + self.probe_buffer.size()
            + self.previous_probes.size()
            + self.uniform_buffer.size()
    }
    pub fn updating(&self) -> bool {
        self.key.is_some() && (self.remaining > 0 || self.pending_scroll.is_some())
    }
    pub fn upload(&mut self, queue: &wgpu::Queue, data: ProxyData) {
        if !data.key.valid() || data.voxels.len() != VOXEL_COUNT {
            return;
        }
        let offset = compose_pending_scroll(self.key, data.key, self.pending_scroll);
        queue.write_buffer(&self.voxels, 0, bytemuck::cast_slice(&data.voxels));
        self.key = Some(data.key);
        self.pending_scroll = Some(offset);
        self.next_probe = 0;
        self.remaining = PROBE_COUNT;
        self.initialized = 0;
        self.uniform.origin_cell = [
            data.key.origin[0],
            data.key.origin[1],
            data.key.origin[2],
            CELL_SIZE,
        ];
    }
    /// `budget` is the maximum updated probes this frame. Zero dispatches once
    /// geometry and lighting converge. Captures may use PROBE_COUNT to warm up.
    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        lighting: Lighting,
        enabled: bool,
        budget: u32,
        profile: Option<&crate::gpu_profile::GpuProfile>,
    ) {
        let enabled = enabled && self.key.is_some();
        if self
            .last_lighting
            .is_none_or(|previous| lighting.changed(previous))
        {
            self.last_lighting = Some(lighting);
            self.remaining = PROBE_COUNT;
        }
        let lighting = self.last_lighting.unwrap();
        let scroll = self.pending_scroll.take();
        let count = if enabled {
            budget
                .min(self.remaining)
                .min(PROBE_COUNT - self.next_probe)
        } else {
            0
        };
        self.uniform.voxel_dims = [VOXEL_DIMS[0], VOXEL_DIMS[1], VOXEL_DIMS[2], 0];
        self.uniform.probe_dims = [PROBE_DIMS[0], PROBE_DIMS[1], PROBE_DIMS[2], PROBE_COUNT];
        self.uniform.parameters = [4., if enabled { 1. } else { 0. }, 1., 0.];
        self.uniform.sun = [
            lighting.sun[0],
            lighting.sun[1],
            lighting.sun[2],
            lighting.sun_strength,
        ];
        self.uniform.direct = [
            lighting.direct[0],
            lighting.direct[1],
            lighting.direct[2],
            0.,
        ];
        self.uniform.ambient = [
            lighting.ambient[0],
            lighting.ambient[1],
            lighting.ambient[2],
            0.,
        ];
        self.uniform.hearths = lighting.hearths;
        self.uniform.update = [
            self.next_probe,
            count,
            u32::from(self.initialized == PROBE_COUNT),
            0,
        ];
        self.uniform.scroll = scroll.unwrap_or([0; 4]);
        // Compare with the last GPU publication, not the staging value: upload()
        // may already have changed origin_cell before this frame's update.
        if self.published_uniform.as_ref().is_none_or(|published| {
            bytemuck::bytes_of(published) != bytemuck::bytes_of(&self.uniform)
        }) {
            queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniform));
            self.published_uniform = Some(self.uniform);
        }
        if scroll.is_some() {
            encoder.copy_buffer_to_buffer(
                &self.probe_buffer,
                0,
                &self.previous_probes,
                0,
                self.probe_buffer.size(),
            );
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Local GI preserve overlapping probes"),
                timestamp_writes: profile.and_then(|p| p.compute_pass(27)),
            });
            pass.set_pipeline(&self.scroll_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(PROBE_COUNT.div_ceil(64), 1, 1);
        }
        if count > 0 {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Local GI bounded first bounce"),
                timestamp_writes: profile.and_then(|p| p.compute_pass(23)),
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(32), 1, 1);
            self.next_probe = (self.next_probe + count) % PROBE_COUNT;
            self.remaining -= count;
            self.initialized = (self.initialized + count).min(PROBE_COUNT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_positions_use_floor_and_scroll_on_probe_grid() {
        let a = VolumeKey::for_eye([-0.1, -0.1, -0.1]);
        assert_eq!(a.origin, [-40., -12., -40.]);
        let b = VolumeKey::for_eye([-8.1, -4.1, -8.1]);
        assert_eq!(b.origin, [-48., -16., -48.]);
        assert_eq!(a.origin.map(|v| v % 4.), [0.; 3]);
    }
    #[test]
    fn uploads_before_dispatch_compose_scroll_and_preserve_full_clear() {
        let a = VolumeKey { origin: [0.; 3] };
        let b = VolumeKey {
            origin: [8., 4., -8.],
        };
        let c = VolumeKey {
            origin: [16., 8., -16.],
        };
        // Two valid uploads before update() must shift the original probes by
        // the total displacement, rather than by only the second displacement.
        let first = compose_pending_scroll(Some(a), b, None);
        assert_eq!(first, [2, 1, -2, 1]);
        let second = compose_pending_scroll(Some(b), c, Some(first));
        assert_eq!(second, [4, 2, -4, 1]);
        // A repeated upload is common between native teleport() and tick().
        // It must preserve the pending clear, not relocate the old world field.
        let far = VolumeKey {
            origin: [400., 0., 0.],
        };
        let teleport = compose_pending_scroll(Some(a), far, None);
        assert_eq!(teleport, [100, 0, 0, 0]);
        assert_eq!(
            compose_pending_scroll(Some(far), far, Some(teleport)),
            teleport
        );
        // First-ever uploads likewise retain no uninitialized GPU history.
        let fresh = compose_pending_scroll(None, a, None);
        assert_eq!(fresh, [0; 4]);
        assert_eq!(compose_pending_scroll(Some(a), a, Some(fresh)), [0; 4]);
        // A same-origin door update after the previous dispatch can retain the
        // already initialized probes while the new occupancy is recomputed.
        assert_eq!(compose_pending_scroll(Some(a), a, None), [0, 0, 0, 1]);
        // Small successive moves can cumulatively exceed the overlap bounds.
        let edge = VolumeKey {
            origin: [60., 0., 0.],
        };
        let outside = VolumeKey {
            origin: [64., 0., 0.],
        };
        let overlap = compose_pending_scroll(Some(a), edge, None);
        assert_eq!(overlap, [15, 0, 0, 1]);
        assert_eq!(
            compose_pending_scroll(Some(edge), outside, Some(overlap)),
            [16, 0, 0, 0]
        );
    }
    #[test]
    fn triangle_proxy_preserves_wall_apertures() {
        let mut mesh = geometry::MeshData::default();
        // Solid wall either side of a two-metre window.
        mesh.quad(
            [4., 0., 2.],
            [4., 0., 4.],
            [4., 4., 4.],
            [4., 4., 2.],
            [0.7, 0.4, 0.2],
            2.,
        );
        mesh.quad(
            [4., 0., 6.],
            [4., 0., 8.],
            [4., 4., 8.],
            [4., 4., 6.],
            [0.7, 0.4, 0.2],
            2.,
        );
        let mut data = ProxyData::empty(VolumeKey { origin: [0.; 3] });
        data.voxelize(&mesh);
        assert!(data.occupied([4.1, 1.2, 3.]));
        assert!(!data.occupied([4.1, 1.2, 5.]));
        assert!(!data.occupied([5.1, 1.2, 3.]));
    }
    #[test]
    fn proxy_transfer_checks_origin_shape_and_truncation() {
        let mut data = ProxyData::empty(VolumeKey {
            origin: [-64., -8., 120.],
        });
        data.voxels[97] = 15 << 27 | 2 << 24 | 0x886644;
        let bytes = encode_proxy(&data);
        let result = decode_proxy(&bytes).unwrap();
        assert_eq!(result.key, data.key);
        assert_eq!(result.voxels[97], data.voxels[97]);
        assert!(decode_proxy(&bytes[..bytes.len() - 4]).is_none());
        let mut bad = bytes.clone();
        bad[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode_proxy(&bad).is_none());
        bad = bytes;
        bad[16..20].copy_from_slice(&127u32.to_le_bytes());
        assert!(decode_proxy(&bad).is_none());
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn gpu_duplicate_teleport_upload_clears_previously_lit_probes() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .unwrap();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    ..Default::default()
                })
                .await
                .unwrap();
            let lighting = Lighting {
                sun: [0., 1., 0.],
                direct: [1.; 3],
                ambient: [0.4; 3],
                sun_strength: 1.,
                hearths: [[0.; 4]; 8],
            };
            let mut gi = GiGpu::new(&device);
            gi.upload(&queue, ProxyData::empty(VolumeKey { origin: [0.; 3] }));
            let mut encoder = device.create_command_encoder(&Default::default());
            gi.update(&queue, &mut encoder, lighting, false, 0, None);
            queue.submit(Some(encoder.finish()));
            // Seed an already published, valid field without depending on ray
            // convergence. Every probe has nonzero incident light and validity.
            let history: Vec<Probe> = (0..PROBE_COUNT)
                .map(|index| Probe {
                    position: [
                        (index % PROBE_DIMS[0]) as f32 * 4. + 0.75,
                        ((index / PROBE_DIMS[0]) % PROBE_DIMS[1]) as f32 * 4. + 0.75,
                        (index / (PROBE_DIMS[0] * PROBE_DIMS[1])) as f32 * 4. + 0.75,
                        1.,
                    ],
                    sh0: [1., 0.4, 0.1, 0.],
                    ..Probe::zeroed()
                })
                .collect();
            queue.write_buffer(&gi.probe_buffer, 0, bytemuck::cast_slice(&history));
            // Native teleport() and its following tick() can both upload before
            // the next render. The duplicate must not cancel the full clear.
            let far = VolumeKey {
                origin: [400., 0., 0.],
            };
            gi.upload(&queue, ProxyData::empty(far));
            gi.upload(&queue, ProxyData::empty(far));
            let mut encoder = device.create_command_encoder(&Default::default());
            gi.update(&queue, &mut encoder, lighting, false, 0, None);
            // Disabled evaluation and zero budget isolate the real scroll pass:
            // no freshly computed probes can hide incorrectly retained history.
            let size = gi.probe_buffer.size();
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GI duplicate teleport regression readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            encoder.copy_buffer_to_buffer(&gi.probe_buffer, 0, &readback, 0, size);
            queue.submit(Some(encoder.finish()));
            let (sender, receiver) = std::sync::mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    sender.send(result).unwrap();
                });
            device.poll(wgpu::PollType::Wait).unwrap();
            receiver.recv().unwrap().unwrap();
            let mapped = readback.slice(..).get_mapped_range();
            for (index, bytes) in mapped
                .chunks_exact(std::mem::size_of::<Probe>())
                .enumerate()
            {
                let probe: Probe = bytemuck::pod_read_unaligned(bytes);
                assert_eq!(
                    probe.position[3], 0.,
                    "probe {index} retained the old origin"
                );
                assert_eq!(
                    probe.sh0, [0.; 4],
                    "probe {index} retained old incident light"
                );
            }
            drop(mapped);
            readback.unmap();
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn gpu_first_bounce_picks_up_surface_color_and_wall_visibility() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .unwrap();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    ..Default::default()
                })
                .await
                .unwrap();
            // Validate the independently included fragment sampling helpers,
            // including tetrahedral ordering, on the same portable WGSL path.
            let _sampling = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("GI sampling shader validation"),
                source: wgpu::ShaderSource::Wgsl(SAMPLING_SHADER.into()),
            });
            let mut gi = GiGpu::new(&device);
            let mut proxy = ProxyData::empty(VolumeKey { origin: [0.; 3] });
            let mut mesh = geometry::MeshData::default();
            mesh.quad(
                [36., 0., 0.],
                [36., 24., 0.],
                [36., 24., 64.],
                [36., 0., 64.],
                [1., 0.03, 0.02],
                2.,
            );
            proxy.voxelize(&mesh);
            gi.upload(&queue, proxy);
            // Test one probe whose +X hemisphere faces the red wall.
            let index = 8 + 2 * PROBE_DIMS[0] + 8 * PROBE_DIMS[0] * PROBE_DIMS[1];
            gi.next_probe = index;
            gi.remaining = 1;
            let mut encoder = device.create_command_encoder(&Default::default());
            gi.update(
                &queue,
                &mut encoder,
                Lighting {
                    sun: [-1., 0., 0.],
                    direct: [2.; 3],
                    ambient: [0.4; 3],
                    sun_strength: 1.,
                    hearths: [[0.; 4]; 8],
                },
                true,
                1,
                None,
            );
            let size = std::mem::size_of::<Probe>() as u64;
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GI test irradiance readback"),
                size,
                mapped_at_creation: false,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            });
            encoder.copy_buffer_to_buffer(&gi.probe_buffer, index as u64 * size, &staging, 0, size);
            queue.submit(Some(encoder.finish()));
            let (sender, receiver) = std::sync::mpsc::channel();
            staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    sender.send(result).unwrap();
                });
            device.poll(wgpu::PollType::Wait).unwrap();
            receiver.recv().unwrap().unwrap();
            let mapped = staging.slice(..).get_mapped_range();
            let probe: Probe = bytemuck::pod_read_unaligned(&mapped);
            assert!(probe.position[3] > 0.5);
            assert!(
                probe.sh0[0] > probe.sh0[1] + 0.2,
                "red wall must produce red indirect light: {:?}",
                probe.sh0
            );
            assert!(
                probe.distance0[0] < 4.,
                "wall must block +X visibility: {:?}",
                probe.distance0
            );
            assert!(
                probe.distance0[1] > 20.,
                "opposite hemisphere must remain open: {:?}",
                probe.distance0
            );
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn point_emitter_visibility_ignores_only_its_source_voxel() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .unwrap();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                    ..Default::default()
                })
                .await
                .unwrap();
            let mut gi = GiGpu::new(&device);
            let mut proxy = ProxyData::empty(VolumeKey { origin: [0.; 3] });
            let source_index = proxy.index(Vec3::new(34.25, 8.25, 32.25)).unwrap();
            let barrier_index = proxy.index(Vec3::new(32.25, 8.25, 32.25)).unwrap();
            proxy.voxels[source_index] = 15 << 27;
            gi.upload(&queue, proxy);
            let mut encoder = device.create_command_encoder(&Default::default());
            gi.update(
                &queue,
                &mut encoder,
                Lighting {
                    sun: [0., 1., 0.],
                    direct: [0.; 3],
                    ambient: [0.; 3],
                    sun_strength: 0.,
                    hearths: [[0.; 4]; 8],
                },
                true,
                0,
                None,
            );
            queue.submit(Some(encoder.finish()));
            let output = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Emitter visibility fixture"),
                size: 16,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Emitter visibility readback"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let result_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Emitter visibility output layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
            let result_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Emitter visibility output group"),
                layout: &result_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output.as_entire_binding(),
                }],
            });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Emitter source-cell regression fixture"), source: wgpu::ShaderSource::Wgsl(format!("{COMMON_SHADER}\n{COMPUTE_SHADER}\n{}", r#"
                    @group(1) @binding(0) var<storage, read_write> fixture: array<vec4<f32>>;
                    @compute @workgroup_size(1) fn emitter_fixture() {
                        let receiver = vec3<f32>(30.25, 8.25, 32.25);
                        let direction = vec3<f32>(1.0, 0.0, 0.0);
                        fixture[0] = vec4<f32>(gi_trace(receiver, direction, 4.0, true).visibility,
                            gi_trace_ignoring_source_cell(receiver, direction, 4.0, true, vec3<i32>(68,16,64)).visibility, 0.0, 0.0);
                    }
                "#).into()),
            });
            let compute_layout = gi.pipeline.get_bind_group_layout(0);
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Emitter fixture pipeline layout"),
                bind_group_layouts: &[&compute_layout, &result_layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Emitter visibility regression"),
                layout: Some(&layout),
                module: &module,
                entry_point: Some("emitter_fixture"),
                compilation_options: Default::default(),
                cache: None,
            });
            for barrier in [false, true] {
                if barrier {
                    queue.write_buffer(
                        &gi.voxels,
                        barrier_index as u64 * 4,
                        bytemuck::bytes_of(&(15u32 << 27)),
                    );
                }
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Emitter fixture trace"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &gi.bind_group, &[]);
                    pass.set_bind_group(1, &result_group, &[]);
                    pass.dispatch_workgroups(1, 1, 1);
                }
                encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 16);
                queue.submit(Some(encoder.finish()));
                let (sender, receiver) = std::sync::mpsc::channel();
                readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        sender.send(result).unwrap();
                    });
                device.poll(wgpu::PollType::Wait).unwrap();
                receiver.recv().unwrap().unwrap();
                let mapped = readback.slice(..).get_mapped_range();
                let values: [f32; 4] = bytemuck::pod_read_unaligned(&mapped);
                assert_eq!(
                    values[0], 0.,
                    "ordinary visibility should detect the enclosing source cell"
                );
                assert_eq!(
                    values[1],
                    if barrier { 0. } else { 1. },
                    "only the enclosing emitter cell may be ignored"
                );
                drop(mapped);
                readback.unmap();
            }
        });
    }
}
