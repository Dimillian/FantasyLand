use crate::{
    cover::{CoverLayer, CoverStats},
    geometry::{self, MeshData, CHUNK_SIZE},
    horizon::{self, PATCH_SIZE},
    postprocess::PostProcess,
    shadow::{ShadowMap, SHADOW_RADIUS},
    water_sim::WaterSim,
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
    solenne: [f32; 4],
    aster: [f32; 4],
    vey: [f32; 4],
    direct: [f32; 4],
    ambient: [f32; 4],
    weather: [f32; 4],
    storm: [f32; 4],
    surface: [f32; 4],
    precipitation_offset: [f32; 4],
    reflection_matrix: [[f32; 4]; 4],
    reflection_params: [f32; 4],
    shelter_matrix: [[f32; 4]; 4],
    shelter_origin: [f32; 4],
    shelter_params: [f32; 4],
    hearths: [[f32; 4]; 8],
    cloud_shadow: [f32; 4],
    rooms_a: [[f32; 4]; 12],
    rooms_b: [[f32; 4]; 12],
    rooms_c: [[f32; 4]; 12],
    hearth_rooms: [[f32; 4]; 2],
    wind_field: crate::wind::WindUniform,
    ao_params: [f32; 4],
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
    // Props can survive one terrain update while their replacement is queued.
    prop_lod: u32,
    prop_detail: u8,
    camera_proxy: bool,
    terrain: Option<GpuMesh>,
    props: Option<GpuMesh>,
    reflected_props: Option<GpuMesh>,
    water: Option<GpuMesh>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkPhase {
    Terrain,
    Props,
}
#[derive(Clone, Copy)]
struct ChunkJob {
    x: i32,
    z: i32,
    lod: u32,
    detail: u8,
    phase: ChunkPhase,
}
fn chunk_phase(current: Option<(u32, u32, u8)>, lod: u32, detail: u8) -> Option<ChunkPhase> {
    match current {
        Some((terrain_lod, prop_lod, prop_detail)) if terrain_lod == lod => {
            (prop_lod != lod || prop_detail != detail).then_some(ChunkPhase::Props)
        }
        _ => Some(ChunkPhase::Terrain),
    }
}

struct ReflectionTarget {
    _color: wgpu::Texture,
    view: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth: wgpu::TextureView,
}
impl ReflectionTarget {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let make = |format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Quarter-area landscape reflection"),
                size: wgpu::Extent3d {
                    width,
                    height,
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
        let color = make(
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
        );
        let depth = make(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
        );
        Self {
            view: color.create_view(&Default::default()),
            depth: depth.create_view(&Default::default()),
            _color: color,
            _depth: depth,
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GiRequest {
    pub ticket: u32,
    pub origin: [f32; 3],
    pub door_ids: Vec<u32>,
    pub door_angles: Vec<f32>,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    gpu_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    surface: Option<wgpu::Surface<'static>>,
    config: wgpu::SurfaceConfiguration,
    world_pipeline: wgpu::RenderPipeline,
    world_reflection_pipeline: wgpu::RenderPipeline,
    world_ao_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    glass_pipeline: wgpu::RenderPipeline,
    glass: Option<GpuMesh>,
    water_sim: WaterSim,
    materials: crate::materials::MaterialLibrary,
    cloud_shadow: crate::cloud_shadow::CloudShadow,
    empty_group: wgpu::BindGroup,
    precipitation: crate::precipitation::PrecipitationMotion,
    sky_pipeline: wgpu::RenderPipeline,
    post: PostProcess,
    gpu_profile: crate::gpu_profile::GpuProfile,
    rays: crate::rays::Rays,
    shadow: ShadowMap,
    cascades: crate::shadow::CascadedShadows,
    ao: crate::ao::AmbientOcclusion,
    ao_depth: wgpu::Texture,
    ao_depth_view: wgpu::TextureView,
    ao_depth_pipeline: wgpu::RenderPipeline,
    gi_screen: crate::gi_screen::GiScreen,
    lighting_mode: u32,
    gi: crate::gi::GiGpu,
    gi_requested: Option<crate::gi::VolumeKey>,
    gi_ticket: u32,
    gi_request: Option<GiRequest>,
    gi_doors: Vec<(u32, f32)>,
    gi_proxy_cache: crate::gi::ProxyCache,
    shadows_enabled: bool,
    shelter: ShadowMap,
    shelter_origin: Vec3,
    shelter_matrix: Mat4,
    shelter_elapsed: f32,
    shelter_valid: bool,
    reflections_enabled: bool,
    enclosure_enabled: bool,
    reflection: ReflectionTarget,
    refraction: ReflectionTarget,
    _reflection_fallback: ReflectionTarget,
    reflection_sampler: wgpu::Sampler,
    uniform_layout: wgpu::BindGroupLayout,
    reflected_uniform: wgpu::Buffer,
    reflected_group: wgpu::BindGroup,
    reflection_plane: Option<f32>,
    reflection_eye: Vec3,
    reflection_yaw: f32,
    reflection_pitch: f32,
    reflection_hour: f32,
    reflection_elapsed: f32,
    reflection_projection: Mat4,
    reflection_origin: Vec3,
    reflection_valid: bool,
    reflection_draws: u32,
    precip_pipeline: wgpu::RenderPipeline,
    weather_system: Option<crate::weather::WeatherSystem>,
    weather_state: crate::weather::WeatherState,
    weather_seed: u32,
    weather_mode: u32,
    weather_speed: f32,
    weather_paused: bool,
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    capture: wgpu::Texture,
    capture_view: wgpu::TextureView,
    chunks: HashMap<(i32, i32), Chunk>,
    pending: VecDeque<ChunkJob>,
    async_streaming: bool,
    stream_ticket: u32,
    stream_turn: u32,
    in_flight: HashMap<u32, crate::streaming::Job>,
    center: Option<(i32, i32)>,
    horizon: HashMap<(i32, i32), [Option<GpuMesh>; 2]>,
    horizon_pending: VecDeque<(i32, i32)>,
    canopies: HashMap<(i32, i32), Option<GpuMesh>>,
    canopy_pending: VecDeque<(i32, i32)>,
    canopy_turn: bool,
    horizon_center: Option<(i32, i32)>,
    quality: u32,
    resolution: u32,
    elapsed: f32,
    wind: crate::wind::Wind,
    climate: [f32; 4],
    air: [f32; 4],
    atmosphere_position: Option<Vec3>,
    hearths: [[f32; 4]; 8],
    dynamic: Option<GpuMesh>,
    people_light_origin: Option<Vec3>,
    room_doors: [u32; 12],
    rooms_a: [[f32; 4]; 12],
    rooms_b: [[f32; 4]; 12],
    rooms_c: [[f32; 4]; 12],
    hearth_rooms: [[f32; 4]; 2],
    cover: CoverLayer,
    ground_cover_density: f32,
    meadow_enabled: bool,
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
                required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| e.to_string())?;
        // WebGPU validation is asynchronous: surface the first failure instead
        // of silently presenting a black frame while the simulation keeps running.
        let gpu_error = std::sync::Arc::new(std::sync::Mutex::new(None));
        let errors = gpu_error.clone();
        device.on_uncaptured_error(Box::new(move |error: wgpu::Error| {
            if let Ok(mut slot) = errors.lock() {
                if slot.is_none() {
                    *slot = Some(error.to_string());
                }
            }
        }));
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
        let scene_shader_source = concat!(
            include_str!("wind.wgsl"),
            include_str!("world.wgsl"),
            "\n",
            include_str!("cover.wgsl"),
            "\n",
            include_str!("lighting.wgsl"),
            "\n",
            include_str!("environment.wgsl"),
            "\n",
            include_str!("water_sampling.wgsl"),
            "\n",
            include_str!("materials.wgsl"),
            "\n",
            include_str!("fire_lighting.wgsl"),
            "\n",
            include_str!("interiors.wgsl"),
            "\n",
            include_str!("room_probe.wgsl"),
            "\n",
            include_str!("gi_common.wgsl"),
            "\n",
            include_str!("gi_sampling.wgsl")
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain, atmosphere and materials"),
            source: wgpu::ShaderSource::Wgsl(scene_shader_source.into()),
        });
        // Separate constant source variants avoid carrying probe-ray lookup
        // into the main foliage shader. Some native backends fail to specialize
        // unrelated water functions through pipeline override constants.
        let reflected_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Reflected landscape with direct GI probes"),
            source: wgpu::ShaderSource::Wgsl(
                scene_shader_source
                    .replace(
                        "const DIRECT_PROBES:bool=false;",
                        "const DIRECT_PROBES:bool=true;",
                    )
                    .into(),
            ),
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera and sunlight"),
            contents: bytemuck::bytes_of(&Globals::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cloud_shadow = crate::cloud_shadow::CloudShadow::new(&device, &uniform, &shader);
        let water_sim = WaterSim::new(&device);
        let materials = crate::materials::MaterialLibrary::new(&device, &queue);
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Unused slot"),
            entries: &[],
        });
        let empty_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Unused slot"),
            layout: &empty_layout,
            entries: &[],
        });
        let shadow = ShadowMap::new(&device, &materials.layout, &empty_layout);
        let cascades =
            crate::shadow::CascadedShadows::new(&device, &materials.layout, &empty_layout);
        let gi = crate::gi::GiGpu::new(&device);
        let (scene, scene_view, depth, depth_view) = Self::targets(&device, width, height, 1, 0);
        let ao_depth = depth.clone();
        let ao_depth_view = depth_view.clone();
        let ao = crate::ao::AmbientOcclusion::new(
            &device,
            scene.width(),
            scene.height(),
            &ao_depth_view,
        );
        let gi_screen = crate::gi_screen::GiScreen::new(
            &device,
            scene.width(),
            scene.height(),
            &ao_depth_view,
            &gi,
        );
        let shelter = ShadowMap::with_size(&device, 512, &materials.layout, &empty_layout);
        let reflection = ReflectionTarget::new(&device, 320, 180);
        let refraction_size = scene_dimensions(
            width,
            height,
            1,
            0,
            device.limits().max_texture_dimension_2d.min(4096),
        );
        let refraction = ReflectionTarget::new(&device, refraction_size[0], refraction_size[1]);
        let reflection_fallback = ReflectionTarget::new(&device, 1, 1);
        let reflection_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Water reflection sampler"),
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let reflected_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mirrored water camera"),
            contents: bytemuck::bytes_of(&Globals::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 12,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 13,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 14,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 15,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let uniform_group = Self::environment_group(
            &device,
            &uniform_layout,
            &uniform,
            &shadow,
            &shelter,
            &reflection.view,
            &reflection_sampler,
            &refraction,
            &cloud_shadow.view,
            &ao.output_view,
            &gi,
            &cascades,
            &gi_screen,
        );
        let reflected_group = Self::environment_group(
            &device,
            &uniform_layout,
            &reflected_uniform,
            &shadow,
            &shelter,
            &reflection_fallback.view,
            &reflection_sampler,
            &refraction,
            &cloud_shadow.view,
            &ao.output_view,
            &gi,
            &cascades,
            &gi_screen,
        );
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("World"),
            bind_group_layouts: &[
                &uniform_layout,
                &water_sim.layout,
                &empty_layout,
                &materials.layout,
            ],
            push_constant_ranges: &[],
        });
        let attributes = crate::vertex::ATTRIBUTES;
        let surface_pipeline = |entry, glass: bool, equal: bool, direct_probes: bool| {
            let shader = if direct_probes {
                &reflected_shader
            } else {
                &shader
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Flat shaded wilderness"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<crate::vertex::PackedVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &attributes,
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba16Float,
                        blend: if glass {
                            Some(wgpu::BlendState::ALPHA_BLENDING)
                        } else {
                            None
                        },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: !glass,
                    depth_compare: if equal {
                        wgpu::CompareFunction::LessEqual
                    } else {
                        wgpu::CompareFunction::Less
                    },
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let ao_depth_pipeline = crate::ao::AmbientOcclusion::depth_pipeline(
            &device,
            &pipeline_layout,
            &shader,
            &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<crate::vertex::PackedVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            }],
            "vs_main",
            "fs_ao_depth",
        );
        let world_pipeline = surface_pipeline("fs_land", false, false, false);
        let world_reflection_pipeline = surface_pipeline("fs_land", false, false, true);
        let world_ao_pipeline = surface_pipeline("fs_land", false, true, false);
        let water_pipeline = surface_pipeline("fs_water", false, false, false);
        let glass_pipeline = surface_pipeline("fs_glass", true, false, false);
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
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });

        let rays = crate::rays::Rays::new(
            &device,
            &depth_view,
            &shadow.view,
            &shadow.sampler,
            [scene.width(), scene.height()],
        );
        let post = PostProcess::new(
            &device,
            &scene_view,
            [scene.width(), scene.height()],
            format,
        );
        let (capture, capture_view) = Self::capture_target(&device, width, height, format);
        let cover = CoverLayer::new(
            &device,
            &uniform_layout,
            &water_sim.layout,
            &materials.layout,
            &shader,
            wgpu::TextureFormat::Rgba16Float,
        );
        let precip_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("World-space rain and snow"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_precip"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_precip"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let gpu_profile = crate::gpu_profile::GpuProfile::new(&device, &queue);
        let mut renderer = Self {
            device,
            queue,
            gpu_error,
            surface,
            config,
            world_pipeline,
            world_reflection_pipeline,
            world_ao_pipeline,
            water_pipeline,
            glass_pipeline,
            glass: None,
            water_sim,
            materials,
            cloud_shadow,
            empty_group,
            precipitation: Default::default(),
            sky_pipeline,
            post,
            gpu_profile,
            rays,
            shadow,
            cascades,
            ao,
            ao_depth,
            ao_depth_view,
            ao_depth_pipeline,
            gi_screen,
            gi,
            lighting_mode: 0,
            gi_requested: None,
            gi_ticket: 0,
            gi_request: None,
            gi_doors: Vec::new(),
            gi_proxy_cache: Default::default(),
            shadows_enabled: true,
            shelter,
            shelter_origin: Vec3::ZERO,
            shelter_matrix: Mat4::IDENTITY,
            shelter_elapsed: -100.0,
            shelter_valid: false,
            reflections_enabled: true,
            enclosure_enabled: true,
            reflection,
            refraction,
            _reflection_fallback: reflection_fallback,
            reflection_sampler,
            uniform_layout,
            reflected_uniform,
            reflected_group,
            reflection_plane: None,
            reflection_eye: Vec3::ZERO,
            reflection_yaw: 0.,
            reflection_pitch: 0.,
            reflection_hour: 0.,
            reflection_elapsed: -100.,
            reflection_projection: Mat4::IDENTITY,
            reflection_origin: Vec3::ZERO,
            reflection_valid: false,
            reflection_draws: 0,
            precip_pipeline,
            weather_system: None,
            weather_state: crate::weather::WeatherState::default(),
            weather_seed: 0,
            weather_mode: 0,
            weather_speed: 1.,
            weather_paused: false,
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
            async_streaming: false,
            stream_ticket: 0,
            stream_turn: 0,
            in_flight: HashMap::new(),
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
            wind: crate::wind::Wind::default(),
            climate: [0.; 4],
            air: [0., 0.4, 0.5, 0.],
            atmosphere_position: None,
            hearths: [[0.0; 4]; 8],
            dynamic: None,
            people_light_origin: None,
            room_doors: [0; 12],
            rooms_a: [[0.; 4]; 12],
            rooms_b: [[0.; 4]; 12],
            rooms_c: [[0.; 4]; 12],
            hearth_rooms: [[-1.; 4]; 2],
            cover,
            ground_cover_density: 4.0,
            meadow_enabled: true,
            cover_drawn_instances: 0,
            width,
            height,
        };
        renderer.set_lighting_mode(7);
        Ok(renderer)
    }
    fn resize_ao(&mut self) {
        self.ao_depth = self.depth.clone();
        self.ao_depth_view = self.depth_view.clone();
        self.ao = crate::ao::AmbientOcclusion::new(
            &self.device,
            self.scene.width(),
            self.scene.height(),
            &self.ao_depth_view,
        );
        self.gi_screen = crate::gi_screen::GiScreen::new(
            &self.device,
            self.scene.width(),
            self.scene.height(),
            &self.ao_depth_view,
            &self.gi,
        );
    }
    pub fn set_lighting_mode(&mut self, mode: u32) {
        self.reflection_valid = false;
        self.lighting_mode = mode & 7;
        if self.lighting_mode & 4 != 0 {
            self.lighting_mode |= 1;
        }
        if self
            .cascades
            .set_enabled(&self.device, &mut self.shadow, mode & 2 != 0)
        {
            self.resize_reflections();
            self.rays.set_cascade_shadow(
                &self.device,
                &self.depth_view,
                &self.shadow.view,
                &self.cascades.far.view,
                &self.shadow.sampler,
            );
        }
        self.gi_requested = None;
        self.gi_request = None;
        self.gi_ticket = self.gi_ticket.wrapping_add(1);
    }
    pub fn lighting_mode(&self) -> u32 {
        self.lighting_mode
    }
    fn gi_active(&self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            self.lighting_mode & 4 != 0 && self.async_streaming
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.lighting_mode & 4 != 0
        }
    }
    pub fn gi_ready_fraction(&self) -> f32 {
        if self.gi_active() {
            self.gi.ready_fraction()
        } else {
            0.0
        }
    }

    pub fn lighting_bytes(&self) -> u64 {
        self.ao.memory_bytes()
            + self.gi.buffer_bytes()
            + self.gi_screen.buffer_bytes()
            + (self.shadow.size() as u64).pow(2) * 4
            + (self.cascades.far.size() as u64).pow(2) * 4
    }
    pub fn next_gi_job(&mut self) -> Option<GiRequest> {
        self.gi_request.take()
    }
    pub fn accept_gi_result(&mut self, ticket: u32, bytes: &[u8]) -> bool {
        if ticket != self.gi_ticket || self.lighting_mode & 4 == 0 {
            return true;
        }
        let Some(data) = crate::gi::decode_proxy(bytes) else {
            return false;
        };
        self.gi.upload(&self.queue, data);
        true
    }
    fn prepare_gi(&mut self, world: &World, eye: Vec3) {
        if self.lighting_mode & 4 == 0 {
            return;
        }
        let key = crate::gi::VolumeKey::for_eye(eye.to_array());
        let mut doors: Vec<_> = world
            .doors
            .borrow()
            .iter()
            .filter(|(id, _)| self.room_doors.contains(id))
            .map(|(&id, d)| (id, (d.angle * 16.).round() / 16.))
            .collect();
        doors.sort_by_key(|d| d.0);
        if self.gi_requested == Some(key) && doors == self.gi_doors {
            return;
        }
        self.gi_requested = Some(key);
        self.gi_doors = doors;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let data = self.gi_proxy_cache.generate(world, key, &self.gi_doors);
            self.gi.upload(&self.queue, data);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.gi_ticket = self.gi_ticket.wrapping_add(1);
            self.gi_request = Some(GiRequest {
                ticket: self.gi_ticket,
                origin: key.origin,
                door_ids: self.gi_doors.iter().map(|d| d.0).collect(),
                door_angles: self.gi_doors.iter().map(|d| d.1).collect(),
            });
        }
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
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            "Linear HDR world",
        );
        let depth = make(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            "World depth",
        );
        let sv = scene.create_view(&Default::default());
        let dv = depth.create_view(&Default::default());
        (scene, sv, depth, dv)
    }
    fn environment_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        shadow: &ShadowMap,
        shelter: &ShadowMap,
        reflection: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        refraction: &ReflectionTarget,
        cloud_shadow: &wgpu::TextureView,
        ao: &wgpu::TextureView,
        gi: &crate::gi::GiGpu,
        cascades: &crate::shadow::CascadedShadows,
        gi_screen: &crate::gi_screen::GiScreen,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shared landscape environment"),
            layout,
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shelter.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(reflection),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&refraction.view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&refraction.depth),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(cloud_shadow),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(ao),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: gi.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: gi.probe_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: cascades.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::TextureView(&cascades.far.view),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&gi_screen.output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: wgpu::BindingResource::TextureView(&gi_screen.depth_view),
                },
            ],
        })
    }
    fn resize_reflections(&mut self) {
        let longest = self.scene.width().max(self.scene.height()) as f32;
        let scale = (640.0 / longest).min(0.5);
        let width = (self.scene.width() as f32 * scale).round().max(1.0) as u32;
        let height = (self.scene.height() as f32 * scale).round().max(1.0) as u32;
        self.reflection = ReflectionTarget::new(&self.device, width, height);
        self.refraction =
            ReflectionTarget::new(&self.device, self.scene.width(), self.scene.height());
        self.uniform_group = Self::environment_group(
            &self.device,
            &self.uniform_layout,
            &self.uniform,
            &self.shadow,
            &self.shelter,
            &self.reflection.view,
            &self.reflection_sampler,
            &self.refraction,
            &self.cloud_shadow.view,
            &self.ao.output_view,
            &self.gi,
            &self.cascades,
            &self.gi_screen,
        );
        self.reflected_group = Self::environment_group(
            &self.device,
            &self.uniform_layout,
            &self.reflected_uniform,
            &self.shadow,
            &self.shelter,
            &self._reflection_fallback.view,
            &self.reflection_sampler,
            &self.refraction,
            &self.cloud_shadow.view,
            &self.ao.output_view,
            &self.gi,
            &self.cascades,
            &self.gi_screen,
        );
        self.reflection_valid = false;
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
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
        );
        self.rays.resize(
            &self.device,
            &self.depth_view,
            &self.shadow.view,
            &self.shadow.sampler,
            [self.scene.width(), self.scene.height()],
        );
        self.resize_ao();
        self.resize_reflections();
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
    fn rebuild_scene(&mut self) {
        (self.scene, self.scene_view, self.depth, self.depth_view) = Self::targets(
            &self.device,
            self.width,
            self.height,
            self.quality,
            self.resolution,
        );
        self.post
            .resize(&self.device, &self.scene_view, self.render_resolution());
        self.rays.resize(
            &self.device,
            &self.depth_view,
            &self.shadow.view,
            &self.shadow.sampler,
            [self.scene.width(), self.scene.height()],
        );
        self.resize_ao();
        self.resize_reflections();
    }
    pub fn set_reflections(&mut self, enabled: bool) {
        self.reflections_enabled = enabled;
        self.reflection_valid = false;
    }
    pub fn set_enclosure(&mut self, enabled: bool) {
        self.enclosure_enabled = enabled;
    }
    pub fn reflection_draws(&self) -> u32 {
        self.reflection_draws
    }
    pub fn set_weather_mode(&mut self, mode: u32) {
        self.weather_mode = mode.min(8);
        if let Some(w) = &mut self.weather_system {
            w.set_mode(self.weather_mode);
        }
    }
    pub fn set_weather_speed(&mut self, speed: f32) {
        self.weather_speed = if speed.is_finite() {
            speed.clamp(0.25, 20.)
        } else {
            1.
        };
        if let Some(w) = &mut self.weather_system {
            w.set_speed(self.weather_speed);
        }
    }
    pub fn set_weather_paused(&mut self, paused: bool) {
        self.weather_paused = paused;
        if let Some(w) = &mut self.weather_system {
            w.set_paused(paused);
        }
    }
    pub fn weather_state(&self) -> &crate::weather::WeatherState {
        &self.weather_state
    }
    pub fn update_weather(&mut self, world: &World, position: Vec3, hour: f32, dt: f32) {
        if self.weather_system.is_none() || self.weather_seed != world.seed {
            let mut w = crate::weather::WeatherSystem::new(world.seed);
            w.set_mode(self.weather_mode);
            w.set_speed(self.weather_speed);
            w.set_paused(self.weather_paused);
            self.weather_system = Some(w);
            self.weather_seed = world.seed;
        }
        self.weather_state = self
            .weather_system
            .as_mut()
            .unwrap()
            .update(dt, world, position.x, position.y, position.z, hour);
    }
    pub fn set_filter(&mut self, mode: u32, strength: f32) {
        self.post.set_filter(mode, strength);
    }
    pub fn set_antialiasing(&mut self, mode: u32) {
        self.post.set_antialiasing(&self.device, &self.queue, mode);
    }
    pub fn set_shadows(&mut self, enabled: bool) {
        self.shadows_enabled = enabled;
    }
    pub fn shadows_enabled(&self) -> bool {
        self.shadows_enabled
    }
    pub fn set_meadow(&mut self, enabled: bool) {
        self.meadow_enabled = enabled;
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
    pub fn gpu_sample_id(&self) -> u32 {
        self.gpu_profile.sample_id()
    }
    pub fn gpu_render_ms(&self) -> Option<f32> {
        self.gpu_profile.span_ms()
    }
    pub fn gpu_timings(&self) -> Vec<crate::gpu_profile::PassTime> {
        self.gpu_profile.latest()
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
            .flat_map(|c| [&c.terrain, &c.props, &c.water, &c.reflected_props])
            .flatten()
            .chain(self.horizon.values().flatten().flatten())
            .chain(self.canopies.values().flatten())
            .map(|m| m.bytes)
            .sum::<u64>()
            + self.cover.stats().buffer_bytes
    }
    pub fn set_quality(&mut self, q: u32) {
        let q = q.min(2);
        if q != self.quality {
            self.quality = q;
            self.in_flight.clear();
            self.center = None;
            self.horizon_center = None;
            self.resize(self.width, self.height);
        }
    }
    pub fn clear_chunks(&mut self) {
        self.people_light_origin = None;
        self.glass = None;
        self.atmosphere_position = None;
        self.hearths = [[0.0; 4]; 8];
        self.shelter_valid = false;
        self.reflection_valid = false;
        self.in_flight.clear();
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
        self.upload_packed(crate::streaming::PackedMesh::new(data))
    }
    fn upload_packed(&self, data: crate::streaming::PackedMesh) -> Option<GpuMesh> {
        if data.indices.is_empty() {
            return None;
        }
        let bounds = data.bounds;
        let vertices = data.vertices;
        let indices = data.indices;
        Some(GpuMesh {
            bounds,
            bytes: (vertices.len() + indices.len()) as u64,
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Generated vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Generated indices"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            count: indices.len() as u32 / 4,
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
        let parts = horizon::patch_parts(world, x, z).map(|mesh| self.upload(mesh));
        self.horizon.insert((x, z), parts);
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
            * if matches!(
                sample.biome,
                crate::world::Biome::Wetland
                    | crate::world::Biome::Swamp
                    | crate::world::Biome::Jungle
            ) {
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
        let mut nearest = f32::INFINITY;
        let mut plane = None;
        for lake in world.lakes() {
            let dx = (position.x - position.x.clamp(lake.bounds[0], lake.bounds[2])).abs();
            let dz = (position.z - position.z.clamp(lake.bounds[1], lake.bounds[3])).abs();
            let distance = dx.hypot(dz);
            if distance < nearest && distance < 2200. && position.y > lake.surface - 1.0 {
                nearest = distance;
                plane = Some(lake.surface);
            }
        }
        let coast_distance = world.coast_info(position.x, position.z).distance;
        if coast_distance < 5000. && coast_distance.max(0.) < nearest {
            plane = Some(0.);
        }
        if plane != self.reflection_plane {
            self.reflection_valid = false;
        }
        self.reflection_plane = plane;
        self.atmosphere_position = Some(position);
    }
    pub fn update_chunks(&mut self, world: &World, position: Vec3, force: bool) {
        self.prepare_gi(world, position + Vec3::Y * 1.72);
        let clock = StreamClock::new();
        self.water_sim
            .update_world(&self.queue, world, position + Vec3::Y * 1.72);
        self.update_atmosphere(world, position);
        if self.weather_system.is_none() || self.weather_seed != world.seed {
            self.update_weather(world, position, 9.0, 0.0);
        }
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
            self.in_flight.clear();
            self.cover.invalidate_requests();
            self.gi_requested = None;
            self.gi_request = None;
            self.gi_ticket = self.gi_ticket.wrapping_add(1);
            self.reflection_valid = false;
            self.horizon_center = None;
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
                    let current = self
                        .chunks
                        .get(&(x, z))
                        .map(|c| (c.lod, c.prop_lod, c.prop_detail));
                    let Some(phase) = chunk_phase(current, lod, detail) else {
                        continue;
                    };
                    requested.push((
                        ChunkJob {
                            x,
                            z,
                            lod,
                            detail,
                            phase,
                        },
                        dist,
                    ));
                }
            }
            requested.sort_by(|a, b| a.1.total_cmp(&b.1));
            // Rebuild the whole queue from actual mesh metadata: a turn back or
            // quality change must not leave a stale props job marked complete.
            self.pending = requested.into_iter().map(|(job, _)| job).collect();
        }
        if self.async_streaming {
            return;
        }
        // Terrain and props are separate queued phases. The first phase retains
        // old props and yields, so a dense replacement cannot stack on the same
        // walking frame as terrain/water generation and packing.
        let budget = if force { 9 } else { 2 };
        for job_index in 0..budget {
            if clock.elapsed_ms() >= limit_ms * 0.55 && (!force || job_index > 0) {
                break;
            }
            let Some(job) = self.pending.pop_front() else {
                break;
            };
            let ChunkJob {
                x,
                z,
                lod,
                detail,
                phase,
            } = job;
            match phase {
                ChunkPhase::Terrain => {
                    let terrain = self.upload(geometry::terrain_chunk(world, x, z, lod));
                    let water = self.upload(geometry::water_chunk(world, x, z, lod));
                    let previous = self.chunks.remove(&(x, z));
                    let (props, reflected_props, prop_lod, prop_detail, camera_proxy) = previous
                        .map(|old| {
                            (
                                old.props,
                                old.reflected_props,
                                old.prop_lod,
                                old.prop_detail,
                                old.camera_proxy,
                            )
                        })
                        .unwrap_or((None, None, u32::MAX, u8::MAX, false));
                    self.chunks.insert(
                        (x, z),
                        Chunk {
                            lod,
                            prop_lod,
                            prop_detail,
                            camera_proxy,
                            terrain,
                            props,
                            reflected_props,
                            water,
                        },
                    );
                    self.pending.push_front(ChunkJob {
                        phase: ChunkPhase::Props,
                        ..job
                    });
                    // Even during warmup, props never run in the same call as
                    // their terrain job. pending_count includes this next phase.
                    break;
                }
                ChunkPhase::Props => {
                    if !self.chunks.get(&(x, z)).is_some_and(|c| c.lod == lod) {
                        // Defensive retry if the terrain was invalidated before
                        // this phase. A new center normally rebuilds the queue.
                        self.pending.push_front(ChunkJob {
                            phase: ChunkPhase::Terrain,
                            ..job
                        });
                        continue;
                    }
                    let props = if detail > 0 {
                        self.upload(geometry::props_chunk_at_lod(world, x, z, false, lod))
                    } else if lod <= 3 {
                        self.upload(geometry::distant_props_chunk_at_lod(world, x, z, lod))
                    } else {
                        None
                    };
                    let reflected_props = if detail > 0 {
                        self.upload(geometry::reflection_props_chunk(world, x, z, lod))
                    } else {
                        None
                    };
                    let chunk = self.chunks.get_mut(&(x, z)).unwrap();
                    chunk.props = props;
                    chunk.reflected_props = reflected_props;
                    chunk.prop_lod = lod;
                    chunk.prop_detail = detail;
                }
            }
        }
        // Alternate cover and horizon work so both progress during exploration.
        // No extra cover job is forced after an expensive terrain/props phase.
        // This budget includes water-patch and atmosphere updates above.
        for _job in 0..if force { 16 } else { 8 } {
            if clock.elapsed_ms() >= limit_ms {
                break;
            }
            let mut progressed = false;
            if self.ground_cover_density > 0.0 {
                progressed |= self.cover.build_next(world, &self.device);
            }
            if clock.elapsed_ms() < limit_ms {
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
    pub fn set_async_streaming(&mut self, enabled: bool) {
        if enabled != self.async_streaming {
            self.async_streaming = enabled;
            self.in_flight.clear();
            self.center = None;
            self.horizon_center = None;
            self.cover.invalidate_requests();
        }
    }
    pub fn next_stream_job(&mut self) -> Vec<i32> {
        use crate::streaming::Job;
        if !self.async_streaming || self.center.is_none() || self.in_flight.len() >= 2 {
            return vec![];
        }
        let mut selected = None;
        // Interleave near terrain, plants and far silhouettes. The background
        // worker cannot spend the entire warmup drawing an empty horizon first.
        for _ in 0..4 {
            let turn = self.stream_turn % 4;
            self.stream_turn = self.stream_turn.wrapping_add(1);
            selected = match turn {
                0 | 2 => self.pending.pop_front().map(|j| Job {
                    kind: if j.phase == ChunkPhase::Terrain { 0 } else { 1 },
                    x: j.x,
                    z: j.z,
                    lod: j.lod,
                    detail: j.detail,
                }),
                1 if self.ground_cover_density > 0. => self.cover.next_job().map(|(x, z)| Job {
                    kind: 4,
                    x,
                    z,
                    lod: 0,
                    detail: 0,
                }),
                _ => {
                    let kind = if self.canopy_turn { 3 } else { 2 };
                    self.canopy_turn = !self.canopy_turn;
                    let primary = if kind == 3 {
                        &mut self.canopy_pending
                    } else {
                        &mut self.horizon_pending
                    };
                    if let Some((x, z)) = primary.pop_front() {
                        Some(Job {
                            kind,
                            x,
                            z,
                            lod: 0,
                            detail: 0,
                        })
                    } else {
                        let kind = 5 - kind;
                        let other = if kind == 3 {
                            &mut self.canopy_pending
                        } else {
                            &mut self.horizon_pending
                        };
                        other.pop_front().map(|(x, z)| Job {
                            kind,
                            x,
                            z,
                            lod: 0,
                            detail: 0,
                        })
                    }
                }
            };
            if selected.is_some() {
                break;
            }
        }
        let Some(job) = selected else { return vec![] };
        self.stream_ticket = self.stream_ticket.wrapping_add(1);
        self.in_flight.insert(self.stream_ticket, job);
        vec![
            self.stream_ticket as i32,
            job.kind as i32,
            job.x,
            job.z,
            job.lod as i32,
            job.detail as i32,
        ]
    }
    pub fn accept_stream_result(&mut self, ticket: u32, bytes: &[u8]) -> bool {
        use crate::streaming::Payload;
        let Some(job) = self.in_flight.remove(&ticket) else {
            return true;
        }; // stale teleport/quality result
        let Some(payload) = crate::streaming::decode(bytes) else {
            return false;
        };
        match payload {
            Payload::Cover(tile) if job.kind == 4 => {
                if tile.seed != self.weather_seed
                    || tile.origin
                        != [
                            job.x as f32 * crate::cover::TILE_SIZE,
                            job.z as f32 * crate::cover::TILE_SIZE,
                        ]
                {
                    return false;
                }
                self.cover.install(&self.device, job.x, job.z, tile)
            }
            Payload::Meshes(meshes) => {
                let expected = if job.kind <= 2 { 2 } else { 1 };
                if meshes.len() != expected || job.kind > 3 {
                    return false;
                }
                let mut parts = meshes
                    .into_iter()
                    .map(|m| self.upload_packed(m))
                    .collect::<Vec<_>>()
                    .into_iter();
                match job.kind {
                    0 => {
                        let terrain = parts.next().unwrap();
                        let water = parts.next().unwrap();
                        let old = self.chunks.remove(&(job.x, job.z));
                        let (props, reflected_props, prop_lod, prop_detail, camera_proxy) = old
                            .map(|c| {
                                (
                                    c.props,
                                    c.reflected_props,
                                    c.prop_lod,
                                    c.prop_detail,
                                    c.camera_proxy,
                                )
                            })
                            .unwrap_or((None, None, u32::MAX, u8::MAX, false));
                        self.chunks.insert(
                            (job.x, job.z),
                            Chunk {
                                lod: job.lod,
                                prop_lod,
                                prop_detail,
                                camera_proxy,
                                terrain,
                                water,
                                props,
                                reflected_props,
                            },
                        );
                        self.pending.push_front(ChunkJob {
                            x: job.x,
                            z: job.z,
                            lod: job.lod,
                            detail: job.detail,
                            phase: ChunkPhase::Props,
                        });
                    }
                    1 => {
                        if let Some(c) = self.chunks.get_mut(&(job.x, job.z)) {
                            if c.lod == job.lod {
                                c.props = parts.next().unwrap();
                                c.reflected_props = parts.next().unwrap();
                                c.prop_lod = job.lod;
                                c.prop_detail = job.detail;
                            }
                        }
                    }
                    2 => {
                        self.horizon.insert(
                            (job.x, job.z),
                            [parts.next().unwrap(), parts.next().unwrap()],
                        );
                    }
                    3 => {
                        self.canopies.insert((job.x, job.z), parts.next().unwrap());
                    }
                    _ => unreachable!(),
                }
            }
            _ => return false,
        }
        // Newly streamed scenery must enter the next reflected/enclosure view.
        if job.kind != 4 {
            self.reflection_valid = false;
        }
        if job.kind <= 1 {
            self.shelter_valid = false;
        }
        true
    }
    pub fn chunk_count(&self) -> usize {
        self.chunks.len() + self.horizon.len()
    }
    pub fn triangle_count(&self) -> usize {
        self.chunks
            .values()
            .map(|c| {
                [&c.terrain, &c.props, &c.water, &c.reflected_props]
                    .into_iter()
                    .map(|m| m.as_ref().map_or(0, |m| m.count as usize / 3))
                    .sum::<usize>()
            })
            .sum::<usize>()
            + self
                .horizon
                .values()
                .flatten()
                .flatten()
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
            + self.in_flight.len()
            + self.horizon_pending.len()
            + self.canopy_pending.len()
            + if self.ground_cover_density > 0.0 {
                self.cover.pending_count()
            } else {
                0
            }
    }
    pub fn advance_time(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.precipitation
            .advance(dt, [self.weather_state.wind_x, self.weather_state.wind_z]);
        self.wind.advance(
            dt,
            [self.weather_state.wind_x, self.weather_state.wind_z],
            self.weather_state.gust,
        );
        self.water_sim.advance_time(dt);
        self.elapsed = (self.elapsed + dt).rem_euclid(86400.0);
        self.shelter_elapsed += dt;
        self.reflection_elapsed += dt;
    }
    pub fn update_people(
        &mut self,
        world: &World,
        life: &crate::citizens::Life,
        eye: Vec3,
        yaw: f32,
    ) {
        let data = life.mesh(world, eye.to_array(), yaw);
        let vertices: Vec<crate::vertex::PackedVertex> = data
            .vertices
            .iter()
            .map(crate::vertex::PackedVertex::from)
            .collect();
        let vb = bytemuck::cast_slice(&vertices);
        let ib = bytemuck::cast_slice(&data.indices);
        if !ib.is_empty() {
            if self.dynamic.as_ref().is_none_or(|m| {
                m.vertices.size() < vb.len() as u64 || m.indices.size() < ib.len() as u64
            }) {
                let buffer = |label, bytes: usize, usage| {
                    self.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some(label),
                        size: bytes.next_power_of_two().max(256) as u64,
                        usage: usage | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                };
                self.dynamic = Some(GpuMesh {
                    vertices: buffer(
                        "Reusable citizens and doors",
                        vb.len(),
                        wgpu::BufferUsages::VERTEX,
                    ),
                    indices: buffer(
                        "Reusable entity indices",
                        ib.len(),
                        wgpu::BufferUsages::INDEX,
                    ),
                    count: 0,
                    bounds: [Vec3::ZERO; 2],
                    bytes: 0,
                });
            }
            let m = self.dynamic.as_mut().unwrap();
            self.queue.write_buffer(&m.vertices, 0, vb);
            self.queue.write_buffer(&m.indices, 0, ib);
            m.count = data.indices.len() as u32;
            m.bounds = [eye - Vec3::splat(200.), eye + Vec3::splat(200.)];
        } else if let Some(m) = &mut self.dynamic {
            m.count = 0;
        }
        let doors = world.doors.borrow();
        let moving_door = doors.values().any(|d| (d.target - d.angle).abs() > 0.01);
        if moving_door {
            self.shelter_valid = false;
            self.reflection_valid = false;
        }
        if self
            .people_light_origin
            .is_some_and(|p| p.distance_squared(eye) < 1.0)
        {
            for (i, id) in self.room_doors.iter().enumerate() {
                self.rooms_c[i][0] = doors.get(id).map_or(0., |d| d.angle);
            }
            return;
        }
        drop(doors);
        self.people_light_origin = Some(eye);
        self.glass = self.upload(crate::settlement_mesh::glass_mesh(world, eye.to_array()));
        self.room_doors = [0; 12];
        self.rooms_a = [[0.; 4]; 12];
        self.rooms_b = [[0.; 4]; 12];
        self.rooms_c = [[0.; 4]; 12];
        let layouts = world.settlements.layouts_near(world, eye.x, eye.z, 100.);
        let mut buildings: Vec<_> = layouts
            .iter()
            .flat_map(|l| l.buildings.iter())
            .filter(|b| {
                b.usage != crate::settlements::Use::Tent
                    && b.usage != crate::settlements::Use::Market
                    && b.usage != crate::settlements::Use::Stable
            })
            .collect();
        buildings.sort_by(|a, b| {
            ((a.x - eye.x).hypot(a.z - eye.z)).total_cmp(&((b.x - eye.x).hypot(b.z - eye.z)))
        });
        for (i, b) in buildings.iter().take(12).enumerate() {
            self.room_doors[i] = b.id;
            self.rooms_a[i] = [b.x, b.floor, b.z, b.height];
            self.rooms_b[i] = [b.half[0], b.half[1], b.yaw.cos(), b.yaw.sin()];
            self.rooms_c[i] = [
                world.doors.borrow().get(&b.id).map_or(0., |d| d.angle),
                b.partition().unwrap_or(0.0),
                0.94,
                b.partition().is_some() as u8 as f32,
            ];
        }
        let mut lights: Vec<_> = layouts
            .iter()
            .flat_map(|l| l.buildings.iter())
            .flat_map(|b| b.lights())
            .collect();
        lights.extend(geometry::campfire_emitters(world, eye.x, eye.z, 70.));
        // Retain the containing room's fixtures before nearby lights behind
        // opaque walls. The existing eight-light GPU budget stays fixed.
        let containing = buildings.iter().find(|b| b.inside(eye.x, eye.z, 0.));
        let light_score = |p: &[f32; 4]| {
            Vec3::from_slice(p).distance_squared(eye)
                - if containing.is_some_and(|b| b.inside(p[0], p[2], 0.)) {
                    10000.
                } else {
                    0.
                }
        };
        lights.sort_by(|a, b| light_score(a).total_cmp(&light_score(b)));
        self.hearths = [[0.; 4]; 8];
        self.hearth_rooms = [[-1.; 4]; 2];
        for (i, (target, source)) in self.hearths.iter_mut().zip(lights).enumerate() {
            *target = source;
            for (j, b) in buildings.iter().take(12).enumerate() {
                if b.inside(source[0], source[2], 0.) {
                    self.hearth_rooms[i / 4][i % 4] = j as f32;
                    break;
                }
            }
        }
        // An animated leaf can reveal a window-sized patch without player travel.
        if world
            .doors
            .borrow()
            .values()
            .any(|d| (d.target - d.angle).abs() > 0.01)
        {
            self.shelter_valid = false;
            self.reflection_valid = false;
        }
    }
    pub fn render(&mut self, eye: Vec3, yaw: f32, pitch: f32, hour: f32) -> Result<(), String> {
        if let Ok(mut error) = self.gpu_error.lock() {
            if let Some(error) = error.take() {
                return Err(error);
            }
        }
        self.gpu_profile.begin();
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
        let sky = crate::celestial::state(hour);
        let sun = sky.primary;
        let brightness = sky.exposure;
        let fog_color = [0.50, 0.64, 0.76];
        let view_projection = projection * view;
        let frustum = frustum_planes(view_projection);
        // Hysteresis avoids toggling entire plant groups when hovering around
        // a distance boundary. Both the main view and mirror share the choice.
        let mut changed_tree_lod = false;
        for c in self.chunks.values_mut() {
            if let Some(mesh) = &c.props {
                let d2 = (eye.clamp(mesh.bounds[0], mesh.bounds[1]) - eye).length_squared();
                let reduced = tree_proxy_lod(c.camera_proxy, d2);
                changed_tree_lod |= reduced != c.camera_proxy;
                c.camera_proxy = reduced;
            }
        }
        if changed_tree_lod {
            self.reflection_valid = false;
        }
        let weather = self.weather_state;
        let wind = (weather.wind_x.hypot(weather.wind_z) / 11.0).clamp(0.10, 2.5);
        let wind_dir = glam::Vec2::new(weather.wind_x, weather.wind_z);
        let mut air = self.air;
        air[1] = wind;
        let shadow_matrix = self.shadow.update(
            &self.queue,
            eye,
            sun,
            self.elapsed,
            wind,
            wind_dir,
            self.wind.uniform(),
        );
        let far_matrix = self.cascades.update(
            &self.queue,
            eye,
            sun,
            self.elapsed,
            wind,
            wind_dir,
            self.wind.uniform(),
        );
        // This cached overhead view serves both local sky occlusion and shelter.
        // Unlike a screen-space effect it also covers roofs outside the view.
        let update_shelter = !self.shelter_valid
            || self.shelter_elapsed > 0.25
            || eye.distance_squared(self.shelter_origin) > 16.0;
        if update_shelter {
            self.shelter_origin = eye;
            self.shelter_matrix = self.shelter.update(
                &self.queue,
                eye,
                Vec3::Y,
                0.0,
                0.0,
                wind_dir,
                crate::wind::WindUniform::default(),
            );
            self.shelter_elapsed = 0.0;
            self.shelter_valid = true;
        }
        // Use the actual submitted water bounds, including patches crossing the
        // near plane. A nearby lake alone must not redraw an invisible mirror.
        let water_visible = self
            .horizon
            .values()
            .filter_map(|p| p[1].as_ref())
            .chain(self.chunks.values().filter_map(|c| c.water.as_ref()))
            .any(|m| bounds_visible(&frustum, m.bounds, eye));
        let reflection_active = self.reflections_enabled
            && water_visible
            && self.reflection_plane.is_some()
            && self.quality > 0;
        if !reflection_active {
            self.reflection_valid = false;
        }
        let update_reflection = reflection_active
            && (!self.reflection_valid
                || (self.gi_active() && self.gi.updating())
                || eye.distance_squared(self.reflection_eye) > 0.09
                || (yaw - self.reflection_yaw).abs() > 0.003
                || (pitch - self.reflection_pitch).abs() > 0.003
                || (hour - self.reflection_hour).abs() > 0.01
                || self.reflection_elapsed > 0.08);
        let plane_y = self.reflection_plane.unwrap_or(0.0);
        if update_reflection {
            self.reflection_origin = Vec3::new(eye.x, 2.0 * plane_y - eye.y, eye.z);
            let reflected_dir = Vec3::new(dir.x, -dir.y, dir.z);
            self.reflection_projection =
                projection * Mat4::look_to_rh(Vec3::ZERO, reflected_dir, Vec3::Y);
            self.reflection_eye = eye;
            self.reflection_yaw = yaw;
            self.reflection_pitch = pitch;
            self.reflection_hour = hour;
            self.reflection_elapsed = 0.0;
            self.reflection_valid = true;
        }
        let shadow_active = self.shadows_enabled && sky.shadow_strength > 0.001;
        let update_clouds =
            self.cloud_shadow
                .prepare(eye, sun, weather.weather, weather.surface[3]);
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
                self.scene.width() as f32,
                self.scene.height() as f32,
            ],
            shadow_matrix: shadow_matrix.to_cols_array_2d(),
            shadow_origin: eye.extend(1.).to_array(),
            shadow_params: [
                1. / self.shadow.size() as f32,
                SHADOW_RADIUS,
                shadow_active as u32 as f32,
                sky.shadow_strength,
            ],
            distant: [
                [8., 12., 16.][self.quality as usize] * CHUNK_SIZE - 300.,
                self.canopy_distance(),
                300.,
                0.,
            ],
            climate: self.climate,
            air,
            solenne: sky.sun.extend(sky.daylight).to_array(),
            aster: sky.aster.extend(crate::celestial::ASTER_RADIUS).to_array(),
            vey: sky.vey.extend(crate::celestial::VEY_RADIUS).to_array(),
            direct: sky.direct_color.extend(sky.direct_strength).to_array(),
            ambient: sky.ambient_color.extend(sky.stars).to_array(),
            weather: weather.weather,
            storm: weather.storm,
            surface: weather.surface,
            precipitation_offset: self.precipitation.uniform(),
            wind_field: self.wind.uniform(),
            reflection_matrix: (self.reflection_projection
                * Mat4::from_translation(eye - self.reflection_origin))
            .to_cols_array_2d(),
            reflection_params: [
                plane_y,
                (reflection_active && self.reflection_valid) as u32 as f32,
                0.0,
                self.quality as f32,
            ],
            hearths: self.hearths,
            cloud_shadow: self.cloud_shadow.uniform(),
            rooms_a: self.rooms_a,
            rooms_b: self.rooms_b,
            rooms_c: self.rooms_c,
            hearth_rooms: self.hearth_rooms,
            ao_params: [
                if self.lighting_mode & 1 != 0 {
                    0.75
                } else {
                    0.
                },
                0.,
                0.,
                0.,
            ],
            shelter_matrix: self.shelter_matrix.to_cols_array_2d(),
            shelter_origin: self.shelter_origin.extend(1.0).to_array(),
            shelter_params: [
                1.0 / 512.0,
                SHADOW_RADIUS,
                (self.shelter_valid && self.enclosure_enabled) as u32 as f32,
                self.shelter_valid as u32 as f32,
            ],
        };
        self.queue
            .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&globals));
        if update_reflection {
            let mut reflected = globals;
            reflected.view_projection = self.reflection_projection.to_cols_array_2d();
            reflected.camera = self.reflection_origin.extend(globals.camera[3]).to_array();
            reflected.params[2] = -pitch;
            reflected.settings[2] = self.reflection._color.width() as f32;
            reflected.settings[3] = self.reflection._color.height() as f32;
            reflected.reflection_params[2] = 1.0;
            reflected.ao_params[0] = 0.0;
            // The fallback bind group contains a separate texture, never the
            // attachment being rendered, even when the shader does not sample it.
            self.queue
                .write_buffer(&self.reflected_uniform, 0, bytemuck::bytes_of(&reflected));
        }
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
        self.gi.update(
            &self.queue,
            &mut encoder,
            crate::gi::Lighting {
                sun: sun.to_array(),
                direct: sky.direct_color.to_array(),
                ambient: sky.ambient_color.to_array(),
                sun_strength: sky.direct_strength,
                hearths: self.hearths,
            },
            self.gi_active(),
            24,
            Some(&self.gpu_profile),
        );
        self.cover.encode_meadow(
            &self.device,
            &self.queue,
            &mut encoder,
            eye,
            view_projection,
            self.scene.height() as f32 * 0.5 / 36.0_f32.to_radians().tan(),
            if self.meadow_enabled {
                self.ground_cover_density
            } else {
                0.
            },
        );
        if update_clouds {
            self.cloud_shadow.encode(&mut encoder, &self.gpu_profile);
        }
        self.water_sim.encode(
            &self.queue,
            &mut encoder,
            weather.rain,
            [weather.wind_x, weather.wind_z],
        );
        // Shadows only submit nearby terrain and substantial props. Grass receives
        // their shade without submitting tens of thousands of tiny shadow casters.
        if shadow_active {
            let planes = frustum_planes(shadow_matrix);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Nearby celestial shadows"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_profile.pass(0),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shadow.pipeline);
            pass.set_bind_group(0, &self.shadow.group, &[]);
            pass.set_bind_group(1, &self.empty_group, &[]);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            if let Some(mesh) = &self.dynamic {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
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
        if shadow_active && self.cascades.enabled() {
            let map = &self.cascades.far;
            let planes = frustum_planes(far_matrix.unwrap());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Far celestial cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &map.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_profile.pass(22),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&map.pipeline);
            pass.set_bind_group(0, &map.group, &[]);
            pass.set_bind_group(1, &self.empty_group, &[]);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            if let Some(mesh) = &self.dynamic {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            for mesh in self
                .chunks
                .values()
                .flat_map(|c| {
                    [
                        c.terrain.as_ref(),
                        c.reflected_props.as_ref().or(c.props.as_ref()),
                    ]
                })
                .flatten()
            {
                if !bounds_visible(&planes, mesh.bounds, eye) {
                    continue;
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
        }
        if update_shelter {
            let planes = frustum_planes(self.shelter_matrix);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Cached overhead enclosure and precipitation shelter"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shelter.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_profile.pass(1),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.shelter.pipeline);
            pass.set_bind_group(0, &self.shelter.group, &[]);
            pass.set_bind_group(1, &self.empty_group, &[]);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            if let Some(mesh) = &self.dynamic {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            for chunk in self.chunks.values() {
                for mesh in [&chunk.terrain, &chunk.props].into_iter().flatten() {
                    if !bounds_visible(&planes, mesh.bounds, eye) {
                        continue;
                    }
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
        }
        self.reflection_draws = 0;
        if update_reflection {
            let planes = frustum_planes(self.reflection_projection);
            let reflected_eye = self.reflection_origin;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Actual landscape mirrored in the nearest water plane"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.reflection.view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.reflection.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_profile.pass(2),
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.reflected_group, &[]);
            pass.set_bind_group(1, &self.water_sim.bind_group, &[]);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            pass.set_pipeline(&self.world_reflection_pipeline);
            if let Some(mesh) = &self.dynamic {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }

            let mut visible: Vec<_> = self
                .horizon
                .values()
                .filter_map(|p| p[0].as_ref())
                .chain(self.canopies.values().flatten())
                .chain(self.chunks.values().flat_map(|c| {
                    let props = if c.camera_proxy {
                        c.reflected_props.as_ref().or(c.props.as_ref())
                    } else {
                        c.props.as_ref()
                    };
                    [c.terrain.as_ref(), props].into_iter().flatten()
                }))
                .filter(|m| {
                    m.bounds[1].y >= plane_y && bounds_visible(&planes, m.bounds, reflected_eye)
                })
                .map(|m| {
                    (
                        (reflected_eye.clamp(m.bounds[0], m.bounds[1]) - reflected_eye)
                            .length_squared(),
                        m,
                    )
                })
                .collect();
            visible.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            for (_, mesh) in visible {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
                self.reflection_draws += 1;
            }
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
        }

        // Draw nearer occluders first. A forest must not shade every distant
        // canopy behind the same trunk before depth can reject those pixels.
        let mut visible: Vec<_> = self
            .chunks
            .values()
            .flat_map(|c| {
                // Original crown clusters keep their leaf gaps in the middle
                // LOD. Detailed c.props still supplies all sun-shadow casters.
                let props = if c.camera_proxy {
                    c.reflected_props.as_ref().or(c.props.as_ref())
                } else {
                    c.props.as_ref()
                };
                [c.terrain.as_ref(), props].into_iter().flatten()
            })
            .chain(self.horizon.values().filter_map(|p| p[0].as_ref()))
            .chain(self.canopies.values().flatten().filter(|mesh| {
                (eye.clamp(mesh.bounds[0], mesh.bounds[1]) - eye).length_squared()
                    <= self.canopy_distance().powi(2)
            }))
            .filter(|mesh| bounds_visible(&frustum, mesh.bounds, eye))
            .map(|mesh| {
                (
                    (eye.clamp(mesh.bounds[0], mesh.bounds[1]) - eye).length_squared(),
                    mesh,
                )
            })
            .collect();
        visible.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        if self.lighting_mode & 1 != 0 {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("AO depth prepass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.ao_depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: self.gpu_profile.pass(18),
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.ao_depth_pipeline);
                pass.set_bind_group(0, &self.uniform_group, &[]);
                pass.set_bind_group(1, &self.water_sim.bind_group, &[]);
                pass.set_bind_group(2, &self.empty_group, &[]);
                pass.set_bind_group(3, &self.materials.bind_group, &[]);
                if let Some(mesh) = &self.dynamic {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
                for (_, mesh) in visible
                    .iter()
                    .filter(|(distance, _)| *distance < 144.0 * 144.0)
                {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
                self.cover.draw_depth(
                    &mut pass,
                    eye,
                    view_projection,
                    self.ground_cover_density,
                    &self.materials.bind_group,
                );
                self.cover.draw_meadow_depth(&mut pass);
            }
            self.ao.update(&self.queue, projection.inverse(), 2.0, 0.75);
            self.ao.encode(
                &mut encoder,
                [
                    self.gpu_profile.compute_pass(19),
                    self.gpu_profile.compute_pass(20),
                    self.gpu_profile.compute_pass(21),
                ],
            );
            if self.gi_active() {
                self.gi_screen
                    .update(&self.queue, view_projection.inverse(), eye);
                self.gi_screen
                    .encode(&mut encoder, self.gpu_profile.compute_pass(28));
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
                        load: if self.lighting_mode & 1 != 0 {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(1.0)
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_profile.pass(3),
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_bind_group(1, &self.water_sim.bind_group, &[]);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            if self.lighting_mode & 1 != 0 {
                self.cover.draw_meadow_with_ao_depth(&mut pass);
            } else {
                self.cover.draw_meadow(&mut pass);
            }
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_pipeline(if self.lighting_mode & 1 != 0 {
                &self.world_ao_pipeline
            } else {
                &self.world_pipeline
            });
            if let Some(mesh) = &self.dynamic {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }

            for (_, mesh) in visible {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            self.cover_drawn_instances = if self.lighting_mode & 1 != 0 {
                self.cover.draw_with_ao_depth(
                    &mut pass,
                    eye,
                    view_projection,
                    self.ground_cover_density,
                    &self.materials.bind_group,
                )
            } else {
                self.cover.draw(
                    &mut pass,
                    eye,
                    view_projection,
                    self.ground_cover_density,
                    &self.materials.bind_group,
                )
            };
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        // Separate opaque and water passes avoid sampling the current attachment.
        // Depth rejects foreground samples along distorted shoreline pixels.
        if water_visible {
            let extent = self.scene.size();
            encoder.copy_texture_to_texture(
                self.scene.as_image_copy(),
                self.refraction._color.as_image_copy(),
                extent,
            );
            encoder.copy_texture_to_texture(
                self.depth.as_image_copy(),
                self.refraction._depth.as_image_copy(),
                extent,
            );
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Refractive water and precipitation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: self.gpu_profile.pass(4),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(0, &self.uniform_group, &[]);
            pass.set_bind_group(1, &self.water_sim.bind_group, &[]);
            pass.set_bind_group(2, &self.empty_group, &[]);
            pass.set_bind_group(3, &self.materials.bind_group, &[]);
            for mesh in self.horizon.values().filter_map(|p| p[1].as_ref()).chain(
                self.chunks
                    .values()
                    .filter_map(|chunk| chunk.water.as_ref()),
            ) {
                if !bounds_visible(&frustum, mesh.bounds, eye) {
                    continue;
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            if let Some(mesh) = &self.glass {
                pass.set_pipeline(&self.glass_pipeline);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.count, 0, 0..1);
            }
            if weather.rain > 0.01 || weather.snow > 0.01 {
                pass.set_pipeline(&self.precip_pipeline);
                pass.set_bind_group(0, &self.uniform_group, &[]);
                pass.draw(0..6, 0..20480);
            }
        }
        self.rays.set_cascade_state(self.cascades.state());
        self.rays.update(
            &self.queue,
            &crate::rays::RaysState {
                view_projection,
                shadow_matrix,
                eye,
                shadow_origin: eye,
                light_direction: sun,
                light_color: sky.direct_color,
                light_intensity: sky.direct_strength,
                shadow_strength: sky.shadow_strength,
                shadows_enabled: shadow_active,
                shadow_radius: SHADOW_RADIUS,
                woodland: self.climate[0],
                humidity: self.air[2],
                fog_base: self.air[0],
                cloud_cover: weather.weather[0],
                rain: weather.rain,
                fog: weather.weather[3],
                quality: self.quality,
                room: (0..12)
                    .filter_map(|i| {
                        let a = self.rooms_a[i];
                        let b = self.rooms_b[i];
                        let dx = eye.x - a[0];
                        let dz = eye.z - a[2];
                        let edge = ((dx * b[2] - dz * b[3]).abs() - b[0]).max(0.0).powi(2)
                            + ((dx * b[3] + dz * b[2]).abs() - b[1]).max(0.0).powi(2);
                        (a[3] > 0.0 && edge < 25.0 && eye.y > a[1] - 1.0 && eye.y < a[1] + a[3])
                            .then_some((i, edge))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i)
                    .map_or([[0.0; 4]; 3], |i| {
                        [self.rooms_a[i], self.rooms_b[i], self.rooms_c[i]]
                    }),
            },
        );
        self.rays
            .encode(&mut encoder, &self.scene_view, &self.gpu_profile);
        self.post.render(
            &self.queue,
            &mut encoder,
            output,
            [self.width, self.height],
            &self.gpu_profile,
        );
        self.gpu_profile.resolve(&mut encoder);
        self.queue.submit(Some(encoder.finish()));
        self.gpu_profile.readback();
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

#[cfg(test)]
mod streaming_phase_tests {
    use super::{chunk_phase, ChunkPhase};
    #[test]
    fn props_readiness_tracks_actual_terrain_lod_and_detail() {
        assert_eq!(chunk_phase(None, 0, 1), Some(ChunkPhase::Terrain));
        // Newly loaded terrain still needs its first props phase.
        assert_eq!(
            chunk_phase(Some((0, u32::MAX, u8::MAX)), 0, 1),
            Some(ChunkPhase::Props)
        );
        assert_eq!(chunk_phase(Some((0, 0, 1)), 0, 1), None);
        // Crossing a terrain LOD threshold preserves the old props briefly,
        // but must not call them ready until they are rooted to the new mesh.
        assert_eq!(
            chunk_phase(Some((0, 0, 1)), 1, 0),
            Some(ChunkPhase::Terrain)
        );
        assert_eq!(chunk_phase(Some((1, 0, 1)), 1, 0), Some(ChunkPhase::Props));
        assert_eq!(chunk_phase(Some((1, 1, 0)), 1, 0), None);
        // A quality change and turning back both requeue only what is missing.
        assert_eq!(chunk_phase(Some((0, 0, 0)), 0, 1), Some(ChunkPhase::Props));
        assert_eq!(
            chunk_phase(Some((1, 1, 0)), 0, 1),
            Some(ChunkPhase::Terrain)
        );
    }
}

fn tree_proxy_lod(reduced: bool, distance_squared: f32) -> bool {
    if reduced {
        distance_squared >= 204.0 * 204.0
    } else {
        distance_squared > 244.0 * 244.0
    }
}
#[cfg(test)]
mod tree_lod_tests {
    #[test]
    fn tree_lod_does_not_chatter_at_the_old_boundary() {
        let mut reduced = false;
        for d in [219.0_f32, 225.0, 215.0, 240.0] {
            reduced = super::tree_proxy_lod(reduced, d * d);
            assert!(!reduced);
        }
        reduced = super::tree_proxy_lod(reduced, 245.0 * 245.0);
        assert!(reduced);
        for d in [240.0_f32, 225.0, 215.0, 205.0] {
            reduced = super::tree_proxy_lod(reduced, d * d);
            assert!(reduced);
        }
        assert!(!super::tree_proxy_lod(reduced, 203.0 * 203.0));
    }
}
