//! Internal-resolution, display-space AA. Scene/depth remain single sampled.
use wgpu::util::DeviceExt;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
struct Image {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}
struct Smaa {
    edges: Image,
    weights: Image,
    edge_pipeline: wgpu::RenderPipeline,
    weight_pipeline: wgpu::RenderPipeline,
    blend_pipeline: wgpu::RenderPipeline,
    weight_layout: wgpu::BindGroupLayout,
    blend_layout: wgpu::BindGroupLayout,
    weight_group: wgpu::BindGroup,
    blend_group: wgpu::BindGroup,
    _area: wgpu::Texture,
    area: wgpu::TextureView,
    _search: wgpu::Texture,
    search: wgpu::TextureView,
}
pub struct AntiAlias {
    pub mode: u32,
    input: Image,
    output: Image,
    size: [u32; 2],
    metrics: wgpu::Buffer,
    sampler: wgpu::Sampler,
    common_layout: wgpu::BindGroupLayout,
    common: wgpu::BindGroup,
    sampler_layout: wgpu::BindGroupLayout,
    sampler_group: wgpu::BindGroup,
    fxaa: wgpu::RenderPipeline,
    smaa: Option<Smaa>,
}
fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
fn image(device: &wgpu::Device, size: [u32; 2], label: &str) -> Image {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    Image {
        _texture: texture,
        view,
    }
}
fn pipeline(
    device: &wgpu::Device,
    label: &str,
    source: &str,
    layouts: &[&wgpu::BindGroupLayout],
    vs: &str,
    fs: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: layouts,
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
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
}
fn pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    output: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    common: &wgpu::BindGroup,
    group: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: output,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, common, &[]);
    pass.set_bind_group(1, group, &[]);
    pass.draw(0..3, 0..1);
}
impl AntiAlias {
    pub fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let common_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AA color and internal pixel metrics"),
            entries: &[
                texture_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("AA linear clamp"),
            entries: &[sampler_entry(0)],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("AA linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sampler_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AA sampler"),
            layout: &sampler_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            }],
        });
        let metrics = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("AA pixel metrics"),
            contents: bytemuck::cast_slice(&[
                1.0 / size[0] as f32,
                1.0 / size[1] as f32,
                size[0] as f32,
                size[1] as f32,
            ]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let input = image(device, size, "Tone mapped AA input");
        let output = image(device, size, "AA resolved display color");
        let common = Self::common(device, &common_layout, &input.view, &metrics);
        let fxaa = pipeline(
            device,
            "FXAA restrained",
            include_str!("aa/fxaa.wgsl"),
            &[&common_layout, &sampler_layout],
            "vs_main",
            "fs_main",
        );
        Self {
            mode: 0,
            input,
            output,
            size,
            metrics,
            sampler,
            common_layout,
            common,
            sampler_layout,
            sampler_group,
            fxaa,
            smaa: None,
        }
    }
    fn common(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        input: &wgpu::TextureView,
        metrics: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("AA source"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: metrics.as_entire_binding(),
                },
            ],
        })
    }
    pub fn input(&self) -> &wgpu::TextureView {
        &self.input.view
    }
    pub fn output(&self) -> &wgpu::TextureView {
        &self.output.view
    }
    pub fn set_mode(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mode: u32) {
        self.mode = if mode <= 2 { mode } else { 1 };
        if self.mode == 2 && self.smaa.is_none() {
            self.smaa = Some(Smaa::new(
                device,
                queue,
                self.size,
                &self.common_layout,
                &self.sampler_layout,
                &self.sampler,
            ));
        }
    }
    pub fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        self.size = size;
        self.input = image(device, size, "Tone mapped AA input");
        self.output = image(device, size, "AA resolved display color");
        self.metrics = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("AA pixel metrics"),
            contents: bytemuck::cast_slice(&[
                1.0 / size[0] as f32,
                1.0 / size[1] as f32,
                size[0] as f32,
                size[1] as f32,
            ]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        self.common = Self::common(device, &self.common_layout, &self.input.view, &self.metrics);
        if let Some(smaa) = &mut self.smaa {
            smaa.resize(device, size, &self.sampler);
        }
    }
    pub fn render(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.mode == 2 {
            let smaa = self.smaa.as_ref().expect("SMAA prepared before rendering");
            pass(
                encoder,
                "SMAA luma edges",
                &smaa.edges.view,
                &smaa.edge_pipeline,
                &self.common,
                &self.sampler_group,
            );
            pass(
                encoder,
                "SMAA diagonal and corner weights",
                &smaa.weights.view,
                &smaa.weight_pipeline,
                &self.common,
                &smaa.weight_group,
            );
            pass(
                encoder,
                "SMAA neighborhood resolve",
                &self.output.view,
                &smaa.blend_pipeline,
                &self.common,
                &smaa.blend_group,
            );
        } else {
            pass(
                encoder,
                "FXAA edge resolve",
                &self.output.view,
                &self.fxaa,
                &self.common,
                &self.sampler_group,
            );
        }
    }
}
impl Smaa {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        common: &wgpu::BindGroupLayout,
        sampler_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
    ) -> Self {
        let lut = |label: &str, format: wgpu::TextureFormat, width, height, pitch, bytes: &[u8]| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(height),
                },
                texture.size(),
            );
            let view = texture.create_view(&Default::default());
            (texture, view)
        };
        let (area_tex, area) = lut(
            "SMAA area LUT",
            wgpu::TextureFormat::Rg8Unorm,
            160,
            560,
            320,
            include_bytes!("aa/area.bin"),
        );
        let (search_tex, search) = lut(
            "SMAA search LUT",
            wgpu::TextureFormat::R8Unorm,
            64,
            16,
            64,
            include_bytes!("aa/search.bin"),
        );
        let weight_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SMAA edge and lookup textures"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                texture_entry(2),
                texture_entry(3),
            ],
        });
        let blend_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SMAA blending weights"),
            entries: &[texture_entry(0), sampler_entry(1)],
        });
        let edge_pipeline = pipeline(
            device,
            "SMAA luma edges",
            include_str!("aa/smaa_edges.wgsl"),
            &[common, sampler_layout],
            "edge_detection_vertex_main",
            "luma_edge_detection_fragment_main",
        );
        let weight_pipeline = pipeline(
            device,
            "SMAA High weights",
            include_str!("aa/smaa_weights.wgsl"),
            &[common, &weight_layout],
            "blending_weight_calculation_vertex_main",
            "blending_weight_calculation_fragment_main",
        );
        let blend_pipeline = pipeline(
            device,
            "SMAA neighborhood",
            include_str!("aa/smaa_blend.wgsl"),
            &[common, &blend_layout],
            "neighborhood_blending_vertex_main",
            "neighborhood_blending_fragment_main",
        );
        let edges = image(device, size, "SMAA edges");
        let weights = image(device, size, "SMAA weights");
        let weight_group =
            Self::weight_group(device, &weight_layout, &edges.view, sampler, &search, &area);
        let blend_group = Self::blend_group(device, &blend_layout, &weights.view, sampler);
        Self {
            edges,
            weights,
            edge_pipeline,
            weight_pipeline,
            blend_pipeline,
            weight_layout,
            blend_layout,
            weight_group,
            blend_group,
            _area: area_tex,
            area,
            _search: search_tex,
            search,
        }
    }
    fn weight_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        edges: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        search: &wgpu::TextureView,
        area: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SMAA weight inputs"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(edges),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(search),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(area),
                },
            ],
        })
    }
    fn blend_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        weights: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SMAA blend inputs"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(weights),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2], sampler: &wgpu::Sampler) {
        self.edges = image(device, size, "SMAA edges");
        self.weights = image(device, size, "SMAA weights");
        self.weight_group = Self::weight_group(
            device,
            &self.weight_layout,
            &self.edges.view,
            sampler,
            &self.search,
            &self.area,
        );
        self.blend_group =
            Self::blend_group(device, &self.blend_layout, &self.weights.view, sampler);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
        let row = (texture.width() * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("AA test readback"),
            size: (row * texture.height()) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(texture.height()),
                },
            },
            texture.size(),
        );
        queue.submit(Some(encoder.finish()));
        let (send, receive) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap()
            });
        device.poll(wgpu::PollType::Wait).unwrap();
        receive.recv().unwrap().unwrap();
        let bytes = buffer.slice(..).get_mapped_range();
        bytes
            .chunks(row as usize)
            .flat_map(|r| r[..texture.width() as usize * 4].iter().copied())
            .collect()
    }
    #[test]
    fn spatial_aa_preserves_flat_color_resolves_diagonals_and_resizes() {
        // Also validates all the actual world/grass/post WGSL pipelines on Metal.
        let renderer = pollster::block_on(crate::renderer::Renderer::headless(64, 64)).unwrap();
        let (device, queue) = (&renderer.device, &renderer.queue);
        let mut aa = AntiAlias::new(device, [64, 64]);
        for size in [[64, 64], [96, 48]] {
            aa.resize(device, size);
            for mode in [1, 2, 1] {
                aa.set_mode(device, queue, mode);
                for flat in [false, true] {
                    let input: Vec<u8> = (0..size[1])
                        .flat_map(|y| {
                            (0..size[0]).flat_map(move |x| {
                                let c = if flat {
                                    93
                                } else if x > y * 2 / 3 + 7
                                    && x < size[0] - 9
                                    && y > 7
                                    && y < size[1] - 7
                                {
                                    230
                                } else {
                                    25
                                };
                                [c, c, c, 255]
                            })
                        })
                        .collect();
                    queue.write_texture(
                        aa.input._texture.as_image_copy(),
                        &input,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(size[0] * 4),
                            rows_per_image: Some(size[1]),
                        },
                        aa.input._texture.size(),
                    );
                    let mut encoder = device.create_command_encoder(&Default::default());
                    aa.render(&mut encoder);
                    queue.submit(Some(encoder.finish()));
                    let output = read(device, queue, &aa.output._texture);
                    if flat {
                        assert!(
                            output.iter().zip(&input).all(|(a, b)| a.abs_diff(*b) <= 1),
                            "mode {mode} changed a constant color"
                        );
                        if let Some(smaa) = &aa.smaa {
                            if mode == 2 {
                                assert!(
                                    read(device, queue, &smaa.edges._texture)
                                        .iter()
                                        .all(|&c| c == 0),
                                    "edge clear retained stale edges"
                                );
                                assert!(
                                    read(device, queue, &smaa.weights._texture)
                                        .iter()
                                        .all(|&c| c == 0),
                                    "weight clear retained stale edges"
                                );
                            }
                        }
                    } else {
                        let blended = output
                            .chunks_exact(4)
                            .filter(|c| c[0] > 26 && c[0] < 229)
                            .count();
                        assert!(
                            blended > 20 && blended < (size[0] * size[1] / 3) as usize,
                            "mode {mode}: implausible edge footprint {blended}"
                        );
                        assert_eq!(&output[..4], &[25, 25, 25, 255]);
                    }
                }
            }
        }
    }
}
