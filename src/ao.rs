//! Spatial GTAO for the forward renderer. The opaque depth pass is owned by the
//! renderer; this object owns only the half-resolution integration/filter and
//! full-resolution visibility textures. No frame history or simulation clock is
//! involved, so a frozen scene remains exactly reproducible.
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use std::cell::Cell;
use wgpu::util::DeviceExt;

/// AO reaches 140 m with a 2 m local sampling radius. Keep a small margin on
/// prepass draw bounds; farther geometry cannot change its visibility result.
pub const DEPTH_DRAW_DISTANCE: f32 = 144.0;

fn texture_entry(binding: u32, view: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct AoUniform {
    inverse_projection: [[f32; 4]; 4],
    viewport: [f32; 4],
    parameters: [f32; 4],
}

pub struct AmbientOcclusion {
    pub output_view: wgpu::TextureView,
    uniform: wgpu::Buffer,
    published: Cell<Option<AoUniform>>,
    horizon: wgpu::ComputePipeline,
    denoise: wgpu::ComputePipeline,
    resolve: wgpu::ComputePipeline,
    horizon_group: wgpu::BindGroup,
    denoise_group: wgpu::BindGroup,
    resolve_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    half_width: u32,
    half_height: u32,
    // Explicit ownership keeps the texture allocation visible in diagnostics.
    _raw: wgpu::Texture,
    _filtered: wgpu::Texture,
    _output: wgpu::Texture,
}

impl AmbientOcclusion {
    pub fn new(device: &wgpu::Device, width: u32, height: u32, depth: &wgpu::TextureView) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let half_width = width.div_ceil(2);
        let half_height = height.div_ceil(2);
        let texture = |label, w, h, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let raw = texture(
            "GTAO half-resolution horizons",
            half_width,
            half_height,
            wgpu::TextureFormat::Rgba16Float,
        );
        let filtered = texture(
            "GTAO bilateral visibility",
            half_width,
            half_height,
            wgpu::TextureFormat::Rgba16Float,
        );
        let output = texture(
            "GTAO full-resolution visibility",
            width,
            height,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let raw_view = raw.create_view(&Default::default());
        let filtered_view = filtered.create_view(&Default::default());
        let output_view = output.create_view(&Default::default());
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GTAO camera and radius"),
            contents: bytemuck::bytes_of(&AoUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let sampled = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let storage = |binding, format| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };
        let horizon_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GTAO horizon resources"),
            entries: &[
                uniform_entry,
                sampled(1, wgpu::TextureSampleType::Depth),
                storage(2, wgpu::TextureFormat::Rgba16Float),
            ],
        });
        let filter_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GTAO denoise resources"),
            entries: &[
                uniform_entry,
                sampled(1, wgpu::TextureSampleType::Float { filterable: false }),
                storage(2, wgpu::TextureFormat::Rgba16Float),
            ],
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GTAO upsample resources"),
            entries: &[
                uniform_entry,
                sampled(1, wgpu::TextureSampleType::Depth),
                sampled(2, wgpu::TextureSampleType::Float { filterable: false }),
                storage(3, wgpu::TextureFormat::Rgba8Unorm),
            ],
        });
        let pipeline = |label, source: &'static str, layout: &wgpu::BindGroupLayout| {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let horizon = pipeline(
            "GTAO horizon integration",
            include_str!("ao.wgsl"),
            &horizon_layout,
        );
        let denoise = pipeline(
            "GTAO edge-aware denoise",
            include_str!("ao_filter.wgsl"),
            &filter_layout,
        );
        let resolve = pipeline(
            "GTAO depth-aware upsample",
            include_str!("ao_resolve.wgsl"),
            &resolve_layout,
        );
        let horizon_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GTAO horizon inputs"),
            layout: &horizon_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                texture_entry(1, depth),
                texture_entry(2, &raw_view),
            ],
        });
        let denoise_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GTAO denoise inputs"),
            layout: &filter_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                texture_entry(1, &raw_view),
                texture_entry(2, &filtered_view),
            ],
        });
        let resolve_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GTAO upsample inputs"),
            layout: &resolve_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                texture_entry(1, depth),
                texture_entry(2, &filtered_view),
                texture_entry(3, &output_view),
            ],
        });
        Self {
            output_view,
            uniform,
            published: Cell::new(None),
            horizon,
            denoise,
            resolve,
            horizon_group,
            denoise_group,
            resolve_group,
            width,
            height,
            half_width,
            half_height,
            _raw: raw,
            _filtered: filtered,
            _output: output,
        }
    }

    /// Recreate the object whenever the scene resolution or source depth changes.
    /// `inverse_projection` is the perspective inverse, without camera rotation.
    pub fn update(
        &self,
        queue: &wgpu::Queue,
        inverse_projection: Mat4,
        radius_m: f32,
        strength: f32,
    ) {
        let radius = if radius_m.is_finite() {
            radius_m.clamp(0.15, 6.0)
        } else {
            2.0
        };
        let strength = if strength.is_finite() {
            strength.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let next = AoUniform {
            inverse_projection: inverse_projection.to_cols_array_2d(),
            viewport: [
                self.width as f32,
                self.height as f32,
                self.half_width as f32,
                self.half_height as f32,
            ],
            parameters: [radius, strength, 140.0, 0.0],
        };
        // The AO camera is projection-only: rotation, position and animation
        // do not change this upload. Publish once per resolution/setting change.
        // None deliberately differs from a zero-valued initial uniform.
        if self.published.get() == Some(next) {
            return;
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&next));
        self.published.set(Some(next));
    }

    /// Only call when enabled. No prepass, dispatch or readback belongs to Off.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        timestamps: [Option<wgpu::ComputePassTimestampWrites<'_>>; 3],
    ) {
        for (i, timestamp_writes) in timestamps.into_iter().enumerate() {
            let (label, pipeline, group, width, height) = match i {
                0 => (
                    "GTAO horizons",
                    &self.horizon,
                    &self.horizon_group,
                    self.half_width,
                    self.half_height,
                ),
                1 => (
                    "GTAO denoise",
                    &self.denoise,
                    &self.denoise_group,
                    self.half_width,
                    self.half_height,
                ),
                _ => (
                    "GTAO upsample",
                    &self.resolve,
                    &self.resolve_group,
                    self.width,
                    self.height,
                ),
            };
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some(label),
                timestamp_writes,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
    }

    pub fn memory_bytes(&self) -> u64 {
        self.half_width as u64 * self.half_height as u64 * 16
            + self.width as u64 * self.height as u64 * 4
            + std::mem::size_of::<AoUniform>() as u64
    }

    /// Depth-only counterpart of an existing opaque draw pipeline. Reuse its
    /// vertex entry, bindings, buffers, mesh selection and draw parameters.
    pub fn depth_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        buffers: &[wgpu::VertexBufferLayout<'_>],
        vertex_entry: &str,
        fragment_entry: &str,
    ) -> wgpu::RenderPipeline {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GTAO matching opaque depth"),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vertex_entry),
                compilation_options: Default::default(),
                buffers,
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &[],
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
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ao_compute_shaders_parse_and_validate() {
        for (name, source) in [
            ("horizons", include_str!("ao.wgsl")),
            ("filter", include_str!("ao_filter.wgsl")),
            ("upsample", include_str!("ao_resolve.wgsl")),
        ] {
            let module = wgpu::naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|e| panic!("GTAO {name}: {}", e.emit_to_string(source)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::default(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("GTAO {name}: {e:?}"));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn gpu_ao_preserves_flat_ambient_and_resolves_local_depth_contact() {
        use super::*;
        const SIZE: u32 = 128;
        let instance = wgpu::Instance::new(&Default::default());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("native GPU adapter for AO validation");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("AO validation device");
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("AO validation depth"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth.create_view(&Default::default());
        let projection = Mat4::perspective_rh(72_f32.to_radians(), 1.0, 0.08, 36000.0);
        let projected_depth = |z: f32| {
            let p = projection * glam::Vec4::new(0.0, 0.0, -z, 1.0);
            p.z / p.w
        };
        let shader_source = format!(
            "@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{{
             let p=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
             return vec4<f32>(p[i],0.5,1.0);}}
             @fragment fn fs(@builtin(position) p:vec4<f32>)->@builtin(frag_depth) f32{{
             return select({},{},p.x>=64.0);}}",
            projected_depth(2.0),
            projected_depth(1.0)
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("AO validation planes"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("AO validation planes"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = AmbientOcclusion::depth_pipeline(&device, &layout, &shader, &[], "vs", "fs");
        let ao = AmbientOcclusion::new(&device, SIZE, SIZE, &depth_view);
        ao.update(&queue, projection.inverse(), 2.0, 1.0);
        let render = || {
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("AO validation planes"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&pipeline);
                pass.draw(0..3, 0..1);
            }
            ao.encode(&mut encoder, [None, None, None]);
            queue.submit(Some(encoder.finish()));
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("AO validation readback"),
                size: (SIZE * SIZE * 4) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                ao._output.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(SIZE * 4),
                        rows_per_image: Some(SIZE),
                    },
                },
                wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));
            let (tx, rx) = std::sync::mpsc::channel();
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |status| tx.send(status).unwrap());
            device.poll(wgpu::PollType::Wait).unwrap();
            rx.recv().unwrap().unwrap();
            let pixels = buffer.slice(..).get_mapped_range().to_vec();
            buffer.unmap();
            pixels
        };
        let first = render();
        assert_eq!(first, render(), "frozen AO dispatch changed visibility");
        let mean = |x0, x1| {
            let mut sum = 0_f32;
            let mut count = 0;
            for y in 24..104 {
                for x in x0..x1 {
                    sum += first[((y * SIZE + x) * 4) as usize] as f32 / 255.0;
                    count += 1;
                }
            }
            sum / count as f32
        };
        assert!(
            mean(0, 8) > 0.94,
            "flat back plane lost ambient: {}",
            mean(0, 8)
        );
        assert!(
            mean(110, 126) > 0.94,
            "unoccluded foreground lost ambient: {}",
            mean(110, 126)
        );
        assert!(
            mean(54, 62) < 0.91,
            "depth contact produced no AO: {}",
            mean(54, 62)
        );
    }
}
