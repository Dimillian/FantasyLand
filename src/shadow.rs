//! Nearby directional shadows. Light-space texel snapping prevents walking shimmer.
use crate::vertex::PackedVertex;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec2, Vec3};
use std::cell::Cell;
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
    size: u32,
    anchor: Cell<Option<ShadowAnchor>>,
}
impl ShadowMap {
    pub fn new(
        device: &wgpu::Device,
        materials: &wgpu::BindGroupLayout,
        empty: &wgpu::BindGroupLayout,
    ) -> Self {
        Self::with_size(device, SHADOW_SIZE, materials, empty)
    }
    pub fn with_size(
        device: &wgpu::Device,
        size: u32,
        materials: &wgpu::BindGroupLayout,
        empty: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Nearby sunlight depth"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
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
            bind_group_layouts: &[&layout, empty, empty, materials],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sun shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let attributes = crate::vertex::ATTRIBUTES;
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Nearby sunlight"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<PackedVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_shadow"),
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
            size,
            anchor: Cell::new(None),
        }
    }
    pub fn update(
        &self,
        queue: &wgpu::Queue,
        eye: Vec3,
        sun: Vec3,
        time: f32,
        wind: f32,
        wind_dir: glam::Vec2,
    ) -> Mat4 {
        let view = shadow_view(sun);
        let texel = SHADOW_RADIUS * 2.0 / self.size as f32;
        let mut anchor = self.anchor.get().unwrap_or_else(|| ShadowAnchor::new(eye));
        anchor.rebase(eye, view, texel);
        let matrix = shadow_matrix_anchored(eye, view, self.size, anchor);
        self.anchor.set(Some(anchor));
        queue.write_buffer(
            &self.uniform,
            0,
            bytemuck::bytes_of(&ShadowUniform {
                matrix: matrix.to_cols_array_2d(),
                origin: eye.extend(1.).to_array(),
                time: [time, wind, wind_dir.x, wind_dir.y],
            }),
        );
        matrix
    }
}
// Snapping an absolute world position in a rotating light basis makes the
// grid phase depend on distance from world origin. At 60km a normal sun step
// can change that phase by half a texel every frame while the player is still.
// Keep the lattice local and retain its fractional phase when rebasing.
#[derive(Clone, Copy)]
struct ShadowAnchor {
    position: Vec3,
    phase: Vec2,
}
impl ShadowAnchor {
    fn new(eye: Vec3) -> Self {
        Self {
            position: eye,
            phase: Vec2::ZERO,
        }
    }
    fn rebase(&mut self, eye: Vec3, view: Mat4, texel: f32) {
        if eye.distance_squared(self.position) <= 512.0 * 512.0 {
            return;
        }
        let next = (eye / 256.0).round() * 256.0;
        let displacement = view.transform_vector3(next - self.position).truncate();
        let phase = self.phase + displacement;
        // Removing whole texels preserves the exact current raster lattice,
        // including its phase, and keeps all later arithmetic in a small range.
        self.phase = phase - (phase / texel).round() * texel;
        self.position = next;
    }
}
fn shadow_view(sun: Vec3) -> Mat4 {
    let up = if sun.dot(Vec3::Y).abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    Mat4::look_at_rh(sun * 1000.0, Vec3::ZERO, up)
}
fn shadow_matrix_anchored(eye: Vec3, view: Mat4, size: u32, anchor: ShadowAnchor) -> Mat4 {
    let local = view.transform_vector3(eye - anchor.position).truncate() + anchor.phase;
    let texel = SHADOW_RADIUS * 2.0 / size as f32;
    let offset = (local / texel).round() * texel - local;
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
    fn projected(eye: Vec3, sun: Vec3, anchor: ShadowAnchor, target: Vec3) -> Vec3 {
        shadow_matrix_anchored(eye, shadow_view(sun), SHADOW_SIZE, anchor)
            .transform_point3(target - eye)
    }
    #[test]
    fn shadow_projection_keeps_fixed_sun_walking_snapped() {
        let sun = Vec3::new(0.7, 0.6, -0.25).normalize();
        let eye = Vec3::new(150000., 250., -70000.);
        let anchor = ShadowAnchor::new(eye);
        let target = eye + Vec3::new(10., 0., -20.);
        let a = projected(eye, sun, anchor, target);
        for i in 1..100 {
            let next = eye + Vec3::new(i as f32 * 0.1, 0., i as f32 * 0.07);
            let b = projected(next, sun, anchor, target);
            assert!(a.is_finite() && b.is_finite());
            for difference in [
                (a.x - b.x) * SHADOW_SIZE as f32 * 0.5,
                (a.y - b.y) * SHADOW_SIZE as f32 * 0.5,
            ] {
                assert!((difference - difference.round()).abs() < 0.03);
            }
        }
    }
    #[test]
    fn rotating_sun_does_not_jump_at_remote_stationary_cameras() {
        for eye in [
            Vec3::new(-10879., 140., 58547.),
            Vec3::new(150000., 250., -70000.),
            Vec3::new(-165000., 1000., -150000.),
        ] {
            let anchor = ShadowAnchor::new(eye);
            let target = eye + Vec3::new(10., 0., -20.);
            let mut old = None;
            for i in 0..240 {
                let hour = 7.5 + i as f32 / (60.0 * 120.0);
                let sun = crate::celestial::state(hour).primary;
                let p = projected(eye, sun, anchor, target);
                if let Some(previous) = old {
                    let shift: Vec3 = (p - previous) * (SHADOW_SIZE as f32 * 0.5);
                    assert!(
                        shift.x.abs() < 0.1 && shift.y.abs() < 0.1,
                        "remote stationary shadow lattice jumped: {shift:?}"
                    );
                }
                old = Some(p);
            }
        }
    }
    #[test]
    fn regional_rebase_preserves_current_texel_phase() {
        let start = Vec3::new(150000.125, 250., -70000.375);
        let eye = start + Vec3::new(560., 20., 200.);
        let target = eye + Vec3::new(10., 0., -20.);
        let sun = crate::celestial::state(7.5).primary;
        let mut anchor = ShadowAnchor::new(start);
        let before = projected(eye, sun, anchor, target);
        anchor.rebase(
            eye,
            shadow_view(sun),
            SHADOW_RADIUS * 2.0 / SHADOW_SIZE as f32,
        );
        let after = projected(eye, sun, anchor, target);
        assert_ne!(anchor.position, start);
        let shift = (after - before) * (SHADOW_SIZE as f32 * 0.5);
        assert!(
            shift.x.abs() < 0.03 && shift.y.abs() < 0.03,
            "rebase shifted lattice: {shift:?}"
        );
        assert!(eye.distance(anchor.position) < 256.0);
    }
}
