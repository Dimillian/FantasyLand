//! Nearby directional shadows. Light-space texel snapping prevents walking shimmer.
use crate::geometry::Vertex;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub const SHADOW_SIZE: u32 = 2048;
pub const SHADOW_RADIUS: f32 = 320.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShadowUniform {
    matrix: [[f32; 4]; 4],
    origin: [f32; 4],
    time: [f32; 4],
}
pub struct ShadowMap {
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub pipeline: wgpu::RenderPipeline,
    pub group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
}
impl ShadowMap {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Nearby sunlight depth"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Filtered sunlight comparison"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Sun shadow camera"),
            contents: bytemuck::bytes_of(&ShadowUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sun shadow uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sun shadow camera"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sun shadow pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sun shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let attributes =
            wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x3,3=>Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Nearby sunlight"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                }],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        Self {
            view,
            sampler,
            pipeline,
            group,
            uniform,
        }
    }
    pub fn update(&self, queue: &wgpu::Queue, eye: Vec3, sun: Vec3, time: f32) -> Mat4 {
        let matrix = shadow_matrix(eye, sun);
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&ShadowUniform {
                matrix: matrix.to_cols_array_2d(),
                origin: eye.extend(1.).to_array(),
                time: [time, 0., 0., 0.],
            }),
        );
        matrix
    }
}
fn shadow_matrix(eye: Vec3, sun: Vec3) -> Mat4 {
    let up = if sun.dot(Vec3::Y).abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let view = Mat4::look_at_rh(sun * 1000.0, Vec3::ZERO, up);
    let absolute = view.transform_vector3(eye);
    let texel = SHADOW_RADIUS * 2.0 / SHADOW_SIZE as f32;
    let offset = (absolute / texel).round() * texel - absolute;
    Mat4::orthographic_rh(
        -SHADOW_RADIUS + offset.x,
        SHADOW_RADIUS + offset.x,
        -SHADOW_RADIUS + offset.y,
        SHADOW_RADIUS + offset.y,
        1.0,
        2000.0,
    ) * view
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_projection_is_finite_and_snaps_world_samples_to_texels() {
        let sun = Vec3::new(0.7, 0.6, -0.25).normalize();
        let eye = Vec3::new(150000., 250., -70000.);
        let next = eye + Vec3::new(0.1, 0., 0.1);
        let target = eye + Vec3::new(10., 0., -20.);
        let a = shadow_matrix(eye, sun).transform_point3(target - eye);
        let b = shadow_matrix(next, sun).transform_point3(target - next);
        assert!(a.is_finite() && b.is_finite());
        for difference in [
            (a.x - b.x) * SHADOW_SIZE as f32 * 0.5,
            (a.y - b.y) * SHADOW_SIZE as f32 * 0.5,
        ] {
            assert!((difference - difference.round()).abs() < 0.03);
        }
    }
}
