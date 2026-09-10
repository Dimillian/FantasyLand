//! Screen filters applied to the world only; browser HUD text remains crisp.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Settings {
    controls: [f32; 4], // mode, strength, ASCII cell scale, ASCII palette
    output: [f32; 4],   // presentation width/height
}
struct Target {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    source: wgpu::BindGroup,
}
struct Level {
    a: Target,
    b: Target,
}

pub struct PostProcess {
    source_layout: wgpu::BindGroupLayout,
    ascii_layout: wgpu::BindGroupLayout,
    ascii_pipeline: wgpu::RenderPipeline,
    ascii_source: wgpu::BindGroup,
    ascii_texture: wgpu::Texture,
    ascii_view: wgpu::TextureView,
    ascii_scale: u32,
    ascii_palette: u32,
    presentation_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    extract: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    presentation: wgpu::RenderPipeline,
    scene_source: wgpu::BindGroup,
    presentation_group: wgpu::BindGroup,
    levels: Vec<Level>,
    mode: u32,
    strength: f32,
}
impl PostProcess {
    pub fn new(
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        size: [u32; 2],
        output_size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bloom source"),
            entries: &[texture(0), sampler_entry(1)],
        });
        let uniform_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let ascii_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ASCII cell source"),
            entries: &[texture(0), sampler_entry(1), uniform_entry(2)],
        });
        let presentation_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Filter composition"),
                entries: &[
                    texture(0),
                    texture(1),
                    texture(2),
                    texture(3),
                    sampler_entry(4),
                    texture(6),
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Soft light reconstruction"),
            min_filter: wgpu::FilterMode::Linear,
            mag_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Filter settings"),
            contents: bytemuck::bytes_of(&Settings::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bloom_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom pyramid"),
            source: wgpu::ShaderSource::Wgsl(include_str!("bloom.wgsl").into()),
        });
        let ascii_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ASCII cell encoder"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ascii.wgsl").into()),
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom, CRT and ASCII presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let pipeline =
            |label, shader: &wgpu::ShaderModule, layout: &wgpu::BindGroupLayout, entry, format| {
                let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(label),
                    bind_group_layouts: &[layout],
                    push_constant_ranges: &[],
                });
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some(entry),
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
                })
            };
        let extract = pipeline(
            "Soft knee highlights",
            &bloom_shader,
            &source_layout,
            "fs_extract",
            wgpu::TextureFormat::Rgba16Float,
        );
        let downsample = pipeline(
            "Bloom downsample",
            &bloom_shader,
            &source_layout,
            "fs_downsample",
            wgpu::TextureFormat::Rgba16Float,
        );
        let horizontal = pipeline(
            "Bloom horizontal blur",
            &bloom_shader,
            &source_layout,
            "fs_horizontal",
            wgpu::TextureFormat::Rgba16Float,
        );
        let vertical = pipeline(
            "Bloom vertical blur",
            &bloom_shader,
            &source_layout,
            "fs_vertical",
            wgpu::TextureFormat::Rgba16Float,
        );
        let presentation = pipeline(
            "World filter",
            &shader,
            &presentation_layout,
            "fs_main",
            format,
        );
        let ascii_pipeline = pipeline(
            "Encode terminal cells",
            &ascii_shader,
            &ascii_layout,
            "fs_main",
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let ascii_source = Self::ascii_source(device, &ascii_layout, scene, &sampler, &uniform);
        let (ascii_texture, ascii_view) = Self::ascii_target(device, output_size, 2);
        let scene_source = Self::source(device, &source_layout, scene, &sampler);
        let levels = Self::levels(device, &source_layout, &sampler, size);
        let presentation_group = Self::composition(
            device,
            &presentation_layout,
            scene,
            &levels,
            &sampler,
            &uniform,
            &ascii_view,
        );
        Self {
            source_layout,
            ascii_layout,
            ascii_pipeline,
            ascii_source,
            ascii_texture,
            ascii_view,
            ascii_scale: 2,
            ascii_palette: 0,
            presentation_layout,
            sampler,
            uniform,
            extract,
            downsample,
            horizontal,
            vertical,
            presentation,
            scene_source,
            presentation_group,
            levels,
            mode: 1,
            strength: 1.0,
        }
    }
    fn source(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Filter input"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
    fn levels(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        size: [u32; 2],
    ) -> Vec<Level> {
        (1..=3)
            .map(|level| {
                let target = || {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Bloom linear light"),
                        size: wgpu::Extent3d {
                            width: (size[0] >> level).max(1),
                            height: (size[1] >> level).max(1),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba16Float,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    let source = Self::source(device, layout, &view, sampler);
                    Target {
                        _texture: texture,
                        view,
                        source,
                    }
                };
                Level {
                    a: target(),
                    b: target(),
                }
            })
            .collect()
    }
    fn composition(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        scene: &wgpu::TextureView,
        levels: &[Level],
        sampler: &wgpu::Sampler,
        uniform: &wgpu::Buffer,
        ascii_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("World and bloom levels"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&levels[0].a.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&levels[1].a.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&levels[2].a.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(ascii_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }
    fn ascii_target(
        device: &wgpu::Device,
        output_size: [u32; 2],
        scale: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Terminal glyph and foreground grid"),
            size: wgpu::Extent3d {
                width: output_size[0].div_ceil(6 * scale).max(1),
                height: output_size[1].div_ceil(9 * scale).max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        (texture, view)
    }
    fn ascii_source(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        scene: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ASCII analysis input"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }
    pub fn set_ascii(&mut self, scale: u32, palette: u32) -> bool {
        let changed = self.ascii_scale != scale.clamp(1, 3);
        self.ascii_scale = scale.clamp(1, 3);
        self.ascii_palette = if palette <= 2 { palette } else { 0 };
        changed
    }
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        scene: &wgpu::TextureView,
        size: [u32; 2],
        output_size: [u32; 2],
    ) {
        (self.ascii_texture, self.ascii_view) =
            Self::ascii_target(device, output_size, self.ascii_scale);
        self.ascii_source = Self::ascii_source(
            device,
            &self.ascii_layout,
            scene,
            &self.sampler,
            &self.uniform,
        );
        self.levels = Self::levels(device, &self.source_layout, &self.sampler, size);
        self.scene_source = Self::source(device, &self.source_layout, scene, &self.sampler);
        self.presentation_group = Self::composition(
            device,
            &self.presentation_layout,
            scene,
            &self.levels,
            &self.sampler,
            &self.uniform,
            &self.ascii_view,
        );
    }
    pub fn set_filter(&mut self, mode: u32, strength: f32) {
        self.mode = if mode <= 3 { mode } else { 1 };
        self.strength = if strength.is_finite() {
            strength.clamp(0.0, 1.5)
        } else {
            1.0
        };
    }
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&Settings {
                controls: [
                    self.mode as f32,
                    self.strength,
                    self.ascii_scale as f32,
                    self.ascii_palette as f32,
                ],
                output: [size[0] as f32, size[1] as f32, 0.0, 0.0],
            }),
        );
        if self.mode == 3 {
            Self::pass(
                encoder,
                &self.ascii_view,
                &self.ascii_pipeline,
                &self.ascii_source,
            );
        }
        if matches!(self.mode, 1 | 2) && self.strength > 0.0 {
            for (i, level) in self.levels.iter().enumerate() {
                let input = if i == 0 {
                    &self.scene_source
                } else {
                    &self.levels[i - 1].a.source
                };
                Self::pass(
                    encoder,
                    &level.a.view,
                    if i == 0 {
                        &self.extract
                    } else {
                        &self.downsample
                    },
                    input,
                );
                Self::pass(encoder, &level.b.view, &self.horizontal, &level.a.source);
                Self::pass(encoder, &level.a.view, &self.vertical, &level.b.source);
            }
        }
        Self::pass(
            encoder,
            output,
            &self.presentation,
            &self.presentation_group,
        );
    }
    fn pass(
        encoder: &mut wgpu::CommandEncoder,
        output: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("World post processing"),
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}
