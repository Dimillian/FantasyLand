//! Half-resolution evaluation cache for the local directional GI field. The
//! main forward pass can sample this with depth-aware interpolation; reflected
//! cameras continue to evaluate the same probe field directly.
use crate::gi::{GiGpu, SAMPLING_SHADER};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::cell::Cell;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
struct Uniform {
    inverse_view_projection: [[f32; 4]; 4],
    camera: [f32; 4],
    resolution: [u32; 4],
}

pub struct GiScreen {
    pub output_view: wgpu::TextureView,
    pub depth_view: wgpu::TextureView,
    output: wgpu::Texture,
    depth_cache: wgpu::Texture,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::ComputePipeline,
    size: [u32; 2],
    scene_size: [u32; 2],
    published_uniform: Cell<Option<Uniform>>,
}
impl GiScreen {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        depth: &wgpu::TextureView,
        gi: &GiGpu,
    ) -> Self {
        let scene_size = [width.max(1), height.max(1)];
        let size = [scene_size[0].div_ceil(2), scene_size[1].div_ceil(2)];
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Half-resolution diffuse GI cache"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        let depth_cache = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GI half-resolution projected depth"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth_view = depth_cache.create_view(&Default::default());
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GI cache camera"),
            contents: bytemuck::bytes_of(&Uniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let buffer_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
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
            label: Some("GI screen cache layout"),
            entries: &[
                buffer_entry(0, wgpu::BufferBindingType::Uniform),
                buffer_entry(1, wgpu::BufferBindingType::Storage { read_only: true }),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                buffer_entry(3, wgpu::BufferBindingType::Uniform),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GI screen cache resources"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: gi.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: gi.probe_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&depth_view),
                },
            ],
        });
        // Keep a single pure lookup implementation across the main/reflection
        // shaders and this compute cache; only the standalone bindings differ.
        let lookup = SAMPLING_SHADER
            .replace("@group(0) @binding(10)", "@group(0) @binding(0)")
            .replace("@group(0) @binding(11)", "@group(0) @binding(1)");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GI screen cache reconstruction"),
            source: wgpu::ShaderSource::Wgsl(
                format!("{lookup}\n{}", include_str!("gi_screen.wgsl")).into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GI screen cache pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GI screen cache"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cache_indirect"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            output_view,
            depth_view,
            output,
            depth_cache,
            uniform,
            bind_group,
            pipeline,
            size,
            scene_size,
            published_uniform: Cell::new(None),
        }
    }
    /// The renderer uses a camera-relative view/projection; the absolute eye
    /// restores the world coordinates expected by the local probe volume.
    pub fn update(&self, queue: &wgpu::Queue, inverse_view_projection: Mat4, eye: Vec3) {
        let next = Uniform {
            inverse_view_projection: inverse_view_projection.to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, 0.],
            resolution: [
                self.scene_size[0],
                self.scene_size[1],
                self.size[0],
                self.size[1],
            ],
        };
        if self.published_uniform.get() == Some(next) {
            return;
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&next));
        self.published_uniform.set(Some(next));
    }
    /// Call only when GI is enabled and the current frame's opaque depth exists.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        timestamp_writes: Option<wgpu::ComputePassTimestampWrites<'_>>,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("GI half-resolution screen cache"),
            timestamp_writes,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.size[0].div_ceil(8), self.size[1].div_ceil(8), 1);
    }
    pub fn buffer_bytes(&self) -> u64 {
        self.output.width() as u64 * self.output.height() as u64 * 8
            + self.depth_cache.width() as u64 * self.depth_cache.height() as u64 * 4
            + self.uniform.size()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[test]
    fn screen_cache_pipeline_accepts_portable_depth_and_storage_resources() {
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
            let gi = GiGpu::new(&device);
            let depth = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("GI cache test depth"),
                size: wgpu::Extent3d {
                    width: 17,
                    height: 11,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = depth.create_view(&Default::default());
            let cache = GiScreen::new(&device, 17, 11, &view, &gi);
            assert_eq!(cache.size, [9, 6]);
            cache.update(&queue, Mat4::IDENTITY, Vec3::ZERO);
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear GI cache sky depth"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            cache.encode(&mut encoder, None);
            queue.submit(Some(encoder.finish()));
            device.poll(wgpu::PollType::Wait).unwrap();
        });
    }
}
