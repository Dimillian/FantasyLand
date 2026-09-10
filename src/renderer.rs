use crate::{
    geometry::{self, MeshData, Vertex, CHUNK_SIZE},
    horizon::{self, PATCH_SIZE},
    world::World,
};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
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
}
struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
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
    blit_pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    blit_layout: wgpu::BindGroupLayout,
    blit_group: wgpu::BindGroup,
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
    horizon_center: Option<(i32, i32)>,
    quality: u32,
    elapsed: f32,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("world.wgsl").into()),
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Pixel presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera and sunlight"),
            contents: bytemuck::bytes_of(&Globals::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("World uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
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
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Pixel source"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&blit_layout],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Nearest pixel upscale"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let (scene, scene_view, depth, depth_view) = Self::targets(&device, width, height, 1);
        let blit_group = Self::blit_group(&device, &blit_layout, &scene_view);
        let (capture, capture_view) = Self::capture_target(&device, width, height, format);
        Ok(Self {
            device,
            queue,
            surface,
            config,
            world_pipeline,
            sky_pipeline,
            blit_pipeline,
            uniform,
            uniform_group,
            blit_layout,
            blit_group,
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
            horizon_center: None,
            quality: 1,
            elapsed: 0.0,
            width,
            height,
        })
    }
    fn targets(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        quality: u32,
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        wgpu::Texture,
        wgpu::TextureView,
    ) {
        let rh = height.min([270, 450, 720][quality as usize]).max(1);
        let rw = ((width as f32 / height.max(1) as f32) * rh as f32)
            .round()
            .max(1.) as u32;
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
    fn blit_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        })
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
            Self::targets(&self.device, width, height, self.quality);
        self.blit_group = Self::blit_group(&self.device, &self.blit_layout, &self.scene_view);
        (self.capture, self.capture_view) =
            Self::capture_target(&self.device, width, height, self.config.format);
    }
    pub fn set_quality(&mut self, q: u32) {
        if q != self.quality {
            self.quality = q;
            self.center = None;
            self.horizon_center = None;
            self.resize(self.width, self.height);
        }
    }
    pub fn clear_chunks(&mut self) {
        self.horizon.clear();
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
        Some(GpuMesh {
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
    fn update_horizon(&mut self, world: &World, position: Vec3, force: bool) {
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
        }
        for _ in 0..if force { 12 } else { 4 } {
            let Some((x, z)) = self.horizon_pending.pop_front() else {
                break;
            };
            if let Some(mesh) = self.upload(horizon::patch(world, x, z)) {
                self.horizon.insert((x, z), mesh);
            }
        }
    }
    pub fn update_chunks(&mut self, world: &World, position: Vec3, force: bool) {
        self.update_horizon(world, position, force);
        let cx = (position.x / CHUNK_SIZE).floor() as i32;
        let cz = (position.z / CHUNK_SIZE).floor() as i32;
        let radius = [8, 12, 16][self.quality as usize];
        let prop_radius = [2.5, 3.7, 4.7][self.quality as usize];
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
                    let lod = if dist < 2.8 {
                        0
                    } else if dist < 5.5 {
                        1
                    } else if dist < 8.5 {
                        2
                    } else {
                        3
                    };
                    let detail = if dist < 2.4 {
                        2
                    } else if dist < prop_radius {
                        1
                    } else {
                        0
                    };
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
        let budget = if force { 9 } else { 2 };
        for _ in 0..budget {
            let Some((x, z, lod, detail)) = self.pending.pop_front() else {
                break;
            };
            let terrain = self.upload(geometry::terrain_chunk(world, x, z, lod));
            let water = self.upload(geometry::water_chunk(world, x, z, lod));
            let props_mesh = if detail > 0 {
                self.upload(geometry::props_chunk_with_cover(world, x, z, detail == 2))
            } else if lod <= 2 {
                self.upload(geometry::distant_props_chunk(world, x, z))
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
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len() + self.horizon_pending.len()
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
        let globals = Globals {
            view_projection: (projection * view).to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, self.width as f32 / self.height as f32],
            light: [sun.x, sun.y, sun.z, brightness],
            fog: [
                fog_color[0],
                fog_color[1],
                fog_color[2],
                [10500., 16500., 22000.][self.quality as usize],
            ],
            params: [self.elapsed, yaw, pitch, hour],
            settings: [[8., 12., 16.][self.quality as usize], 250., 0., 0.],
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
            let horizontal = Vec3::new(dir.x, 0., dir.z).normalize_or_zero();
            for (&(cx, cz), mesh) in &self.horizon {
                let offset = Vec3::new(
                    (cx as f32 + 0.5) * PATCH_SIZE - eye.x,
                    0.,
                    (cz as f32 + 0.5) * PATCH_SIZE - eye.z,
                );
                if offset.dot(horizontal) < -PATCH_SIZE * 1.5 {
                    continue;
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            for (&(cx, cz), chunk) in &self.chunks {
                let offset = Vec3::new(
                    (cx as f32 + 0.5) * CHUNK_SIZE - eye.x,
                    0.,
                    (cz as f32 + 0.5) * CHUNK_SIZE - eye.z,
                );
                let distance = offset.length();
                if distance > CHUNK_SIZE * 2.0 && offset.dot(horizontal) < -CHUNK_SIZE * 1.5 {
                    continue;
                }
                for mesh in [&chunk.terrain, &chunk.props, &chunk.water]
                    .into_iter()
                    .flatten()
                {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Pixel presentation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &self.blit_group, &[]);
            pass.draw(0..3, 0..1);
        }
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
