use crate::{
    cover::{CoverLayer, CoverStats},
    geometry::{self, MeshData, Vertex, CHUNK_SIZE},
    horizon::{self, PATCH_SIZE},
    postprocess::PostProcess,
    shadow::{ShadowMap, SHADOW_RADIUS, SHADOW_SIZE},
    world::World,
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3, Vec4};
use std::collections::{HashMap, VecDeque};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    light: [f32; 4],
    fog: [f32; 4],
    params: [f32; 4],
    settings: [f32; 4],
    shadow_matrix: [[f32; 4]; 4],
    shadow_origin: [f32; 4],
    shadow_params: [f32; 4],
    distant: [f32; 4],
    climate: [f32; 4],
    air: [f32; 4],
}
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    bounds: [Vec3; 2],
    bytes: u64,
}
struct Chunk {
    lod: u32,
    prop_detail: u8,
    terrain: Option<GpuMesh>,
    props: Option<GpuMesh>,
    water: Option<GpuMesh>,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    config: wgpu::SurfaceConfiguration,
    world_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    post: PostProcess,
    shadow: ShadowMap,
    shadows_enabled: bool,
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    capture: wgpu::Texture,
    capture_view: wgpu::TextureView,
    chunks: HashMap<(i32, i32), Chunk>,
    pending: VecDeque<(i32, i32, u32, u8)>,
    center: Option<(i32, i32)>,
    horizon: HashMap<(i32, i32), GpuMesh>,
    horizon_pending: VecDeque<(i32, i32)>,
    canopies: HashMap<(i32, i32), Option<GpuMesh>>,
    canopy_pending: VecDeque<(i32, i32)>,
    canopy_turn: bool,
    horizon_center: Option<(i32, i32)>,
    quality: u32,
    resolution: u32,
    elapsed: f32,
    climate: [f32; 4],
    air: [f32; 4],
    atmosphere_position: Option<Vec3>,
    cover: CoverLayer,
    ground_cover_density: f32,
    cover_drawn_instances: u32,
    pub width: u32,
    pub height: u32,
}

impl Renderer {
    #[cfg(target_arch = "wasm32")]
    pub async fn browser(canvas: web_sys::HtmlCanvasElement) -> Result<Self, String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("WebGPU adapter unavailable: {e}"))?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        Self::new(adapter, Some(surface), format, width, height).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn headless(width: u32, height: u32) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| e.to_string())?;
        Self::new(
            adapter,
            None,
            wgpu::TextureFormat::Rgba8Unorm,
            width,
            height,
        )
        .await
    }
    async fn new(
        adapter: wgpu::Adapter,
        surface: Option<wgpu::Surface<'static>>,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, String> {
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("FantasyLand GPU"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| e.to_string())?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        if let Some(s) = &surface {
            s.configure(&device, &config);
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain, atmosphere and materials"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("world.wgsl"),
                    "\n",
                    include_str!("cover.wgsl"),
                    "\n",
                    include_str!("lighting.wgsl")
                )
                .into(),
            ),
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera and sunlight"),
            contents: bytemuck::bytes_of(&Globals::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let shadow = ShadowMap::new(&device);
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("World uniforms"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow.sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("World"),
            bind_group_layouts: &[&uniform_layout],
            push_constant_ranges: &[],
        });
        let attributes =
            wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x3,3=>Float32];
        let world_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Flat shaded wilderness"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
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
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let (scene, scene_view, depth, depth_view) = Self::targets(&device, width, height, 1, 0);
        let post = PostProcess::new(
            &device,
            &scene_view,
            [scene.width(), scene.height()],
            [width, height],
            format,
        );
        let (capture, capture_view) = Self::capture_target(&device, width, height, format);
        let cover = CoverLayer::new(
            &device,
            &uniform_layout,
            &shader,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        Ok(Self {
            device,
            queue,
            surface,
            config,
            world_pipeline,
            sky_pipeline,
            post,
            shadow,
            shadows_enabled: true,
            uniform,
            uniform_group,
            scene,
            scene_view,
            depth,
            depth_view,
            capture,
            capture_view,
            chunks: HashMap::new(),
            pending: VecDeque::new(),
            center: None,
            horizon: HashMap::new(),
            horizon_pending: VecDeque::new(),
            canopies: HashMap::new(),
            canopy_pending: VecDeque::new(),
            canopy_turn: false,
            horizon_center: None,
            quality: 1,
            resolution: 0,
            elapsed: 0.0,
            climate: [0.; 4],
            air: [0., 0.4, 0.5, 0.],
            atmosphere_position: None,
            cover,
            ground_cover_density: 4.0,
            cover_drawn_instances: 0,
            width,
            height,
        })
    }
    fn targets(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        quality: u32,
        resolution: u32,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
    ) {
        let [rw, rh] = scene_dimensions(
            width,
            height,
            quality,
            resolution,
            device.limits().max_texture_dimension_2d.min(4096),
        );
        let make = |format, usage, label| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: rw,
                    height: rh,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let scene = make(
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            "Low resolution world",
        );
        let depth = make(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
            "World depth",
        );
        let sv = scene.create_view(&Default::default());
        let dv = depth.create_view(&Default::default());
        (scene, sv, depth, dv)
    }
    fn capture_target(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Native verification target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let v = t.create_view(&Default::default());
        (t, v)
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        let limit = self.device.limits().max_texture_dimension_2d.min(4096) as f32;
        let scale = (limit / width.max(1) as f32)
            .min(limit / height.max(1) as f32)
            .min(1.0);
        let width = (width.max(1) as f32 * scale).round().max(1.0) as u32;
        let height = (height.max(1) as f32 * scale).round().max(1.0) as u32;
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        if let Some(s) = &self.surface {
            s.configure(&self.device, &self.config);
        }
        (self.scene, self.scene_view, self.depth, self.depth_view) =
            Self::targets(&self.device, width, height, self.quality, self.resolution);
        self.post.resize(
            &self.device,
            &self.scene_view,
            [self.scene.width(), self.scene.height()],
            [self.width, self.height],
        );
        (self.capture, self.capture_view) =
            Self::capture_target(&self.device, width, height, self.config.format);
    }
    pub fn set_render_resolution(&mut self, resolution: u32) {
        let resolution = if resolution <= 1 {
            resolution
        } else {
            resolution.clamp(90, 2160)
        };
        if self.resolution != resolution {
            self.resolution = resolution;
            self.rebuild_scene();
        }
    }
    pub fn render_resolution(&self) -> [u32; 2] {
        [self.scene.width(), self.scene.height()]
    }
    pub fn set_ascii(&mut self, scale: u32, palette: u32) {
        if self.post.set_ascii(scale, palette) {
            self.post.resize(
                &self.device,
                &self.scene_view,
                self.render_resolution(),
                [self.width, self.height],
            );
        }
    }
    fn rebuild_scene(&mut self) {
        (self.scene, self.scene_view, self.depth, self.depth_view) = Self::targets(
            &self.device,
            self.width,
            self.height,
            self.quality,
            self.resolution,
        );
        self.post.resize(
            &self.device,
            &self.scene_view,
            self.render_resolution(),
            [self.width, self.height],
        );
    }
    pub fn set_filter(&mut self, mode: u32, strength: f32) {
        self.post.set_filter(mode, strength);
    }
    pub fn set_shadows(&mut self, enabled: bool) {
        self.shadows_enabled = enabled;
    }
    pub fn shadows_enabled(&self) -> bool {
        self.shadows_enabled
    }
    pub fn set_ground_cover_density(&mut self, density: f32) {
        // Resident instances are sorted by density threshold. Only each draw's
        // prefix length changes, preserving terrain, trees and all tile buffers.
        self.ground_cover_density = if density.is_finite() {
            density.clamp(0.0, 4.0)
        } else {
            4.0
        };
    }
    pub fn cover_stats(&self) -> CoverStats {
        self.cover.stats()
    }
    pub fn cover_drawn_instances(&self) -> u32 {
        self.cover_drawn_instances
    }
    pub fn ground_cover_density(&self) -> f32 {
        self.ground_cover_density
    }
    pub fn mesh_bytes(&self) -> u64 {
        self.chunks
            .values()
            .flat_map(|c| [&c.terrain, &c.props, &c.water])
            .flatten()
            .chain(self.horizon.values())
            .chain(self.canopies.values().flatten())
            .map(|m| m.bytes)
            .sum::<u64>()
            + self.cover.stats().buffer_bytes
    }
    pub fn set_quality(&mut self, q: u32) {
        let q = q.min(2);
        if q != self.quality {
            self.quality = q;
            self.center = None;
            self.horizon_center = None;
            self.resize(self.width, self.height);
        }
    }
    pub fn clear_chunks(&mut self) {
        self.atmosphere_position = None;
        self.cover.clear();
        self.cover_drawn_instances = 0;
        self.horizon.clear();
        self.canopies.clear();
        self.canopy_pending.clear();
        self.horizon_pending.clear();
        self.horizon_center = None;
        self.chunks.clear();
        self.pending.clear();
        self.center = None;
    }
    fn upload(&self, data: MeshData) -> Option<GpuMesh> {
        if data.indices.is_empty() {
            return None;
        }
        let mut bounds = [Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)];
        for vertex in &data.vertices {
            let p = Vec3::from_array(vertex.position);
            bounds[0] = bounds[0].min(p);
            bounds[1] = bounds[1].max(p);
        }
        // Wind and the most distant foliage must not disappear on frustum edges.
        bounds[0] -= Vec3::ONE;
        bounds[1] += Vec3::ONE;
        Some(GpuMesh {
            bounds,
            bytes: (data.vertices.len() * std::mem::size_of::<Vertex>() + data.indices.len() * 4)
                as u64,
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Generated vertices"),
                    contents: bytemuck::cast_slice(&data.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Generated indices"),
                    contents: bytemuck::cast_slice(&data.indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            count: data.indices.len() as u32,
        })
    }
    fn prepare_horizon(&mut self, position: Vec3) {
        let cx = (position.x / PATCH_SIZE).floor() as i32;
        let cz = (position.z / PATCH_SIZE).floor() as i32;
        let radius = [8, 14, 16][self.quality as usize];
        if self.horizon_center != Some((cx, cz)) {
            self.horizon_center = Some((cx, cz));
            self.horizon
                .retain(|&(x, z), _| (x - cx).abs() <= radius + 1 && (z - cz).abs() <= radius + 1);
            let mut requested = Vec::new();
            for z in cz - radius..=cz + radius {
                for x in cx - radius..=cx + radius {
                    let dist = (x - cx).pow(2) + (z - cz).pow(2);
                    if dist > (radius + 1) * (radius + 1) || self.horizon.contains_key(&(x, z)) {
                        continue;
                    }
                    requested.push((x, z, dist));
                }
            }
            requested.sort_by_key(|p| p.2);
            self.horizon_pending = requested.into_iter().map(|p| (p.0, p.1)).collect();
            let canopy_radius = (self.canopy_distance() / PATCH_SIZE).ceil() as i32 + 1;
            self.canopies.retain(|&(x, z), _| {
                (x - cx).abs() <= canopy_radius + 1 && (z - cz).abs() <= canopy_radius + 1
            });
            let mut canopy_jobs = Vec::new();
            for z in cz - canopy_radius..=cz + canopy_radius {
                for x in cx - canopy_radius..=cx + canopy_radius {
                    let d = (x - cx).pow(2) + (z - cz).pow(2);
                    if d <= (canopy_radius + 1).pow(2) && !self.canopies.contains_key(&(x, z)) {
                        canopy_jobs.push((x, z, d));
                    }
                }
            }
            canopy_jobs.sort_by_key(|p| p.2);
            self.canopy_pending = canopy_jobs.into_iter().map(|p| (p.0, p.1)).collect();
        }
    }
    fn canopy_distance(&self) -> f32 {
        [5000., 8500., 11000.][self.quality as usize]
    }
    fn build_canopy(&mut self, world: &World) -> bool {
        let Some((x, z)) = self.canopy_pending.pop_front() else {
            return false;
        };
        let mesh = self.upload(horizon::canopy(world, x, z));
        self.canopies.insert((x, z), mesh);
        true
    }
    fn build_horizon(&mut self, world: &World) -> bool {
        let Some((x, z)) = self.horizon_pending.pop_front() else {
            return false;
        };
        if let Some(mesh) = self.upload(horizon::patch(world, x, z)) {
            self.horizon.insert((x, z), mesh);
        }
        true
    }
    fn update_atmosphere(&mut self, world: &World, position: Vec3) {
        let travel = self.atmosphere_position.map_or(f32::INFINITY, |p| {
            (position.x - p.x).hypot(position.z - p.z)
        });
        if travel < 3.0 {
            return;
        }
        let sample = world.natural_sample(position.x, position.z);
        let region = crate::regions::sample(world.seed, position.x, position.z, &sample);
        let substrate = crate::regions::base(world.seed, position.x, position.z);
        let mut forest =
            world.vegetation_density_from_sample(position.x, position.z, &sample) * 2.0;
        let mut reference = sample.height.max(sample.water_height);
        for (dx, dz) in [(-120., 0.), (120., 0.), (0., -120.), (0., 120.)] {
            let s = world.natural_sample(position.x + dx, position.z + dz);
            forest += world.vegetation_density_from_sample(position.x + dx, position.z + dz, &s);
            reference = reference.min(s.height.max(s.water_height));
        }
        if sample.ocean {
            reference = 0.;
        }
        let wet = (region.wetness
            * if sample.biome == crate::world::Biome::Wetland {
                1.0
            } else {
                0.42
            })
        .max(
            if sample.water_height > sample.height - 6. && !sample.ocean {
                0.75
            } else {
                0.0
            },
        );
        let climate = [
            (forest / 6.).clamp(0., 1.),
            wet,
            substrate.weights[2],
            region.exposure,
        ];
        let air = [
            reference,
            (0.23 + region.exposure * 0.62).clamp(0., 1.),
            sample.moisture,
            0.,
        ];
        // Distance-based blending is stable at different frame rates. Teleports
        // adopt their destination immediately; walking changes air gradually.
        let blend = if travel > 300. {
            1.
        } else {
            1. - (-travel / 55.).exp()
        };
        for i in 0..4 {
            self.climate[i] += (climate[i] - self.climate[i]) * blend;
            self.air[i] += (air[i] - self.air[i]) * blend;
        }
        self.atmosphere_position = Some(position);
    }
    pub fn update_chunks(&mut self, world: &World, position: Vec3, force: bool) {
        self.update_atmosphere(world, position);
        let clock = StreamClock::new();
        let limit_ms = if force { 12.0 } else { 4.0 };
        self.prepare_horizon(position);
        self.cover.prepare(position);
        let cx = (position.x / CHUNK_SIZE).floor() as i32;
        let cz = (position.z / CHUNK_SIZE).floor() as i32;
        let radius = [8, 12, 16][self.quality as usize];
        // Branch-level detail is useful near the player; distant trees keep
        // their rooted silhouettes without duplicating hundreds of vertices.
        let prop_radius = [1.8, 2.7, 3.7][self.quality as usize];
        if self.center != Some((cx, cz)) {
            self.center = Some((cx, cz));
            self.chunks
                .retain(|&(x, z), _| (x - cx).abs() <= radius + 1 && (z - cz).abs() <= radius + 1);
            let mut requested = Vec::new();
            for z in cz - radius..=cz + radius {
                for x in cx - radius..=cx + radius {
                    let dist = (((x - cx).pow(2) + (z - cz).pow(2)) as f32).sqrt();
                    if dist > radius as f32 + 0.5 {
                        continue;
                    }
                    let lod = terrain_lod(dist);
                    let detail = if dist < prop_radius { 1 } else { 0 };
                    if self
                        .chunks
                        .get(&(x, z))
                        .is_some_and(|c| c.lod == lod && c.prop_detail == detail)
                    {
                        continue;
                    }
                    requested.push((x, z, lod, detail, dist));
                }
            }
            requested.sort_by(|a, b| a.4.total_cmp(&b.4));
            self.pending = requested
                .into_iter()
                .map(|(x, z, l, p, _)| (x, z, l, p))
                .collect();
        }
        // A time budget supplements the job cap. No large grass mesh is baked
        // inside these 192m chunks; small cover tiles are independent jobs.
        let budget = if force { 9 } else { 2 };
        for job in 0..budget {
            if job > 0 && clock.elapsed_ms() >= limit_ms * 0.55 {
                break;
            }
            let Some((x, z, lod, detail)) = self.pending.pop_front() else {
                break;
            };
            let previous = self.chunks.remove(&(x, z));
            let (terrain, water) = if let Some(old) = previous.filter(|old| old.lod == lod) {
                (old.terrain, old.water)
            } else {
                (
                    self.upload(geometry::terrain_chunk(world, x, z, lod)),
                    self.upload(geometry::water_chunk(world, x, z, lod)),
                )
            };
            let props_mesh = if detail > 0 {
                self.upload(geometry::props_chunk_at_lod(world, x, z, false, lod))
            } else if lod <= 3 {
                self.upload(geometry::distant_props_chunk_at_lod(world, x, z, lod))
            } else {
                None
            };
            self.chunks.insert(
                (x, z),
                Chunk {
                    lod,
                    prop_detail: detail,
                    terrain,
                    props: props_mesh,
                    water,
                },
            );
        }
        // Alternate cover and horizon work so both progress during exploration.
        // At least one small cover tile can progress even after a costly terrain
        // job; subsequent work respects the shared frame budget.
        for job in 0..if force { 16 } else { 8 } {
            if job > 0 && clock.elapsed_ms() >= limit_ms {
                break;
            }
            let mut progressed = false;
            if self.ground_cover_density > 0.0 {
                progressed |= self.cover.build_next(world, &self.device);
            }
            if clock.elapsed_ms() < limit_ms
                || (self.horizon.is_empty() && self.canopies.is_empty())
            {
                // Neither distant terrain nor canopy may monopolize the budget.
                progressed |= if self.canopy_turn {
                    self.build_canopy(world) || self.build_horizon(world)
                } else {
                    self.build_horizon(world) || self.build_canopy(world)
                };
                self.canopy_turn = !self.canopy_turn;
            }
            if !progressed {
                break;
            }
        }
    }
    pub fn chunk_count(&self) -> usize {
        self.chunks.len() + self.horizon.len()
    }
    pub fn triangle_count(&self) -> usize {
        self.chunks
            .values()
            .map(|c| {
                [&c.terrain, &c.props, &c.water]
                    .into_iter()
                    .map(|m| m.as_ref().map_or(0, |m| m.count as usize / 3))
                    .sum::<usize>()
            })
            .sum::<usize>()
            + self
                .horizon
                .values()
                .map(|m| m.count as usize / 3)
                .sum::<usize>()
            + self
                .canopies
                .values()
                .flatten()
                .map(|m| m.count as usize / 3)
                .sum::<usize>()
            + self.cover.stats().loaded_instances as usize * crate::plants::TRIANGLES as usize
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
            + self.horizon_pending.len()
            + self.canopy_pending.len()
            + if self.ground_cover_density > 0.0 {
                self.cover.pending_count()
            } else {
                0
            }
    }
    pub fn advance_time(&mut self, dt: f32) {
        self.elapsed = (self.elapsed + dt.clamp(0.0, 0.1)).rem_euclid(86400.0);
    }
    pub fn render(&mut self, eye: Vec3, yaw: f32, pitch: f32, hour: f32) -> Result<(), String> {
        let dir = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let view = Mat4::look_to_rh(Vec3::ZERO, dir, Vec3::Y);
        let projection = Mat4::perspective_rh(
            72f32.to_radians(),
            self.width as f32 / self.height as f32,
            0.08,
            36000.0,
        );
        let daylight = ((hour - 6.0) / 12.0 * std::f32::consts::PI).sin().max(0.0);
        let brightness = 0.44 + daylight * 0.76;
        let sun_angle = (hour - 6.0) / 12.0 * std::f32::consts::PI;
        let sun = Vec3::new(sun_angle.cos(), sun_angle.sin(), -0.25).normalize();
        let fog_color = [0.48 * brightness, 0.64 * brightness, 0.76 * brightness];
        let view_projection = projection * view;
        let frustum = frustum_planes(view_projection);
        let shadow_matrix = self
            .shadow
            .update(&self.queue, eye, sun, self.elapsed, self.air[1]);
        let shadow_active = self.shadows_enabled && sun.y > 0.035;
        let globals = Globals {
            view_projection: view_projection.to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, self.width as f32 / self.height as f32],
            light: [sun.x, sun.y, sun.z, brightness],
            fog: [
                fog_color[0],
                fog_color[1],
                fog_color[2],
                [10500., 16500., 22000.][self.quality as usize],
            ],
            params: [self.elapsed, yaw, pitch, hour],
            settings: [
                [8., 12., 16.][self.quality as usize],
                crate::cover::COVER_DISTANCE,
                0.,
                0.,
            ],
            shadow_matrix: shadow_matrix.to_cols_array_2d(),
            shadow_origin: eye.extend(1.).to_array(),
            shadow_params: [
                1. / SHADOW_SIZE as f32,
                SHADOW_RADIUS,
                shadow_active as u32 as f32,
                0.,
            ],
            distant: [
                [8., 12., 16.][self.quality as usize] * CHUNK_SIZE - 300.,
                self.canopy_distance(),
                300.,
                0.,
            ],
            climate: self.climate,
            air: self.air,
        };
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&globals));
        let frame = if let Some(surface) = &self.surface {
            match surface.get_current_texture() {
                Ok(f) => Some(f),
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    surface.configure(&self.device, &self.config);
                    return Ok(());
                }
                Err(wgpu::SurfaceError::Timeout) => return Ok(()),
                Err(e) => return Err(e.to_string()),
            }
        } else {
            None
        };
        let output_view = frame
            .as_ref()
            .map(|f| f.texture.create_view(&Default::default()));
        let output = output_view.as_ref().unwrap_or(&self.capture_view);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("World frame"),
            });
        // Shadows only submit nearby terrain and substantial props. Grass receives
        // their shade without submitting tens of thousands of tiny shadow casters.
        if shadow_active {
            let planes = frustum_planes(shadow_matrix);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Nearby sun shadows"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow.pipeline);
            pass.set_bind_group(0, &self.shadow.group, &[]);
            for chunk in self.chunks.values() {
                for mesh in [&chunk.terrain, &chunk.props].into_iter().flatten() {
                    let nearest = eye.clamp(mesh.bounds[0], mesh.bounds[1]);
                    if (nearest - eye).length_squared() > 650.0 * 650.0
                        || !bounds_visible(&planes, mesh.bounds, eye)
                    {
                        continue;
                    }
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Wilderness"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.6,
                            b: 0.7,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.world_pipeline);
            // Reject whole mesh bounds before vertex work. This includes side,
            // vertical, near and far planes, rather than only the rear hemisphere.
            for (mesh, is_canopy) in self
                .horizon
                .values()
                .map(|m| (m, false))
                .chain(self.canopies.values().flatten().map(|m| (m, true)))
            {
                let nearest = eye.clamp(mesh.bounds[0], mesh.bounds[1]);
                if is_canopy && (nearest - eye).length_squared() > self.canopy_distance().powi(2) {
                    continue;
                }
                if !bounds_visible(&frustum, mesh.bounds, eye) {
                    continue;
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            for chunk in self.chunks.values() {
                for mesh in [&chunk.terrain, &chunk.props, &chunk.water]
                    .into_iter()
                    .flatten()
                {
                    if !bounds_visible(&frustum, mesh.bounds, eye) {
                        continue;
                    }
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
            self.cover_drawn_instances =
                self.cover
                    .draw(&mut pass, eye, view_projection, self.ground_cover_density);
        }
        self.post
            .render(&self.queue, &mut encoder, output, [self.width, self.height]);
        self.queue.submit(Some(encoder.finish()));
        if let Some(frame) = frame {
            frame.present();
        }
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn capture_rgba(&self) -> Result<Vec<u8>, String> {
        let stride = (self.width * 4 + 255) / 256 * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Verification readback"),
            size: (stride * self.height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            self.capture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait)
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in mapped.chunks(stride as usize).take(self.height as usize) {
            pixels.extend_from_slice(&row[..(self.width * 4) as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(pixels)
    }
}

// Browser time comes from Performance; std::time::Instant is unavailable on
// wasm32-unknown-unknown. Native verification uses the same millisecond budget.
struct StreamClock {
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(target_arch = "wasm32")]
    start: f64,
}
impl StreamClock {
    fn new() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            start: std::time::Instant::now(),
            #[cfg(target_arch = "wasm32")]
            start: browser_clock_ms(),
        }
    }
    fn elapsed_ms(&self) -> f64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.start.elapsed().as_secs_f64() * 1000.0
        }
        #[cfg(target_arch = "wasm32")]
        {
            browser_clock_ms() - self.start
        }
    }
}
#[cfg(target_arch = "wasm32")]
fn browser_clock_ms() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now())
}
fn frustum_planes(matrix: Mat4) -> [Vec4; 6] {
    let m = matrix.transpose();
    [
        m.w_axis + m.x_axis,
        m.w_axis - m.x_axis,
        m.w_axis + m.y_axis,
        m.w_axis - m.y_axis,
        m.z_axis,
        m.w_axis - m.z_axis,
    ]
}
fn bounds_visible(planes: &[Vec4; 6], bounds: [Vec3; 2], eye: Vec3) -> bool {
    let center = (bounds[0] + bounds[1]) * 0.5 - eye;
    let extent = (bounds[1] - bounds[0]) * 0.5;
    planes
        .iter()
        .all(|p| p.truncate().dot(center) + p.w + p.truncate().abs().dot(extent) >= 0.0)
}

// Fixed vertical resolutions preserve aspect ratio and adapter texture limits.
fn scene_dimensions(
    width: u32,
    height: u32,
    quality: u32,
    resolution: u32,
    limit: u32,
) -> [u32; 2] {
    let width = width.max(1) as f64;
    let height = height.max(1) as f64;
    let requested = match resolution {
        0 => height.min([270., 450., 720.][quality.min(2) as usize]),
        1 => height,
        n => n.clamp(90, 2160) as f64,
    };
    let rw = width / height * requested;
    let scale = (limit.max(1) as f64 / rw.max(requested)).min(1.0);
    [
        (rw * scale).round().max(1.0) as u32,
        (requested * scale).round().max(1.0) as u32,
    ]
}

#[cfg(test)]
mod resolution_tests {
    use super::scene_dimensions;

    #[test]
    fn fixed_resolution_is_independent_of_world_quality() {
        for quality in 0..=2 {
            assert_eq!(scene_dimensions(1280, 720, quality, 240, 4096), [427, 240]);
            assert_eq!(
                scene_dimensions(1280, 720, quality, 1080, 4096),
                [1920, 1080]
            );
            assert_eq!(scene_dimensions(1280, 720, quality, 1, 4096), [1280, 720]);
        }
        assert_eq!(scene_dimensions(1280, 720, 0, 0, 4096), [480, 270]);
        assert_eq!(scene_dimensions(1280, 720, 1, 0, 4096), [800, 450]);
    }

    #[test]
    fn extreme_aspects_fit_texture_limits_without_zero_dimensions() {
        for (width, height) in [(0, 0), (20000, 1), (1, 20000), (5120, 1440)] {
            for resolution in [0, 1, 240, 1080, u32::MAX] {
                let size = scene_dimensions(width, height, 2, resolution, 4096);
                assert!(size
                    .into_iter()
                    .all(|dimension| (1..=4096).contains(&dimension)));
            }
        }
        assert_eq!(scene_dimensions(5120, 1440, 1, 1080, 4096), [3840, 1080]);
    }
}

#[cfg(test)]
mod frustum_tests {
    use super::*;
    #[test]
    fn rejects_outside_meshes_but_keeps_bounds_crossing_a_plane() {
        let planes = frustum_planes(Mat4::perspective_rh(
            72_f32.to_radians(),
            16. / 9.,
            0.08,
            100.,
        ));
        let bounds = |center: Vec3, half: Vec3| [center - half, center + half];
        assert!(bounds_visible(
            &planes,
            bounds(Vec3::new(0., 0., -10.), Vec3::ONE),
            Vec3::ZERO
        ));
        for center in [
            Vec3::new(0., 0., 10.),
            Vec3::new(100., 0., -10.),
            Vec3::new(0., 100., -10.),
            Vec3::new(0., 0., -110.),
        ] {
            assert!(!bounds_visible(
                &planes,
                bounds(center, Vec3::ONE),
                Vec3::ZERO
            ));
        }
        assert!(bounds_visible(
            &planes,
            bounds(Vec3::new(0., 0., -101.), Vec3::splat(3.)),
            Vec3::ZERO
        ));
        assert!(bounds_visible(
            &planes,
            bounds(Vec3::ZERO, Vec3::ONE),
            Vec3::ZERO
        ));
        // Camera-relative testing must give identical results on remote islands.
        let eye = Vec3::new(154000., 200., -87000.);
        assert!(bounds_visible(
            &planes,
            bounds(eye + Vec3::new(0., 0., -10.), Vec3::ONE),
            eye
        ));
    }
}

fn terrain_lod(chunk_distance: f32) -> u32 {
    if chunk_distance < 3.8 {
        0
    } else if chunk_distance < 5.5 {
        1
    } else if chunk_distance < 8.5 {
        2
    } else {
        3
    }
}
#[cfg(test)]
mod cover_lod_tests {
    use super::*;
    #[test]
    fn visible_cover_uses_the_same_six_metre_terrain_surface() {
        for x in [0.0, 96.0, 191.999] {
            for z in [0.0, 96.0, 191.999] {
                let eye = Vec3::new(x, 0., z);
                for cz in -5i32..=5 {
                    for cx in -5i32..=5 {
                        let min = Vec3::new(cx as f32 * CHUNK_SIZE, 0., cz as f32 * CHUNK_SIZE);
                        let max = min + Vec3::new(CHUNK_SIZE, 0., CHUNK_SIZE);
                        if (eye.clamp(min, max) - eye).length() < crate::cover::COVER_DISTANCE {
                            assert_eq!(terrain_lod(((cx*cx+cz*cz)as f32).sqrt()),0,
                        "cover would float on a coarser terrain chunk at {cx},{cz} from {eye:?}");
                        }
                    }
                }
            }
        }
    }
}
