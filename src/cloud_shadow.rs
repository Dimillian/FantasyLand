//! World-anchored cache of the same low cloud density drawn by the sky shader.
//! Density is reused, but elevation/light projection remains per surface pixel.
use glam::{Vec2, Vec3};

pub const SIZE: u32 = 512;
const SPAN: f32 = 32768.0;

#[derive(Default)]
struct CacheState {
    center: Vec2,
    seconds: f32,
    coverage: f32,
    valid: bool,
}
impl CacheState {
    fn prepare(&mut self, eye: Vec3, light: Vec3, weather: [f32; 4], seconds: f32) -> bool {
        let base = 3300.0 - weather[1] * 1250.0 - weather[2] * 700.0;
        let projected = Vec2::new(eye.x, eye.z)
            + Vec2::new(light.x, light.z) * (base - eye.y).max(0.0) / light.y.max(0.035);
        let moved = projected.distance_squared(self.center) > (SPAN * 0.20).powi(2);
        let update = !self.valid
            || moved
            || (seconds - self.seconds).abs() >= 0.125
            || (weather[0] - self.coverage).abs() > 0.01;
        if update {
            // Recenter by whole texels. Overlapping samples remain on the same
            // world lattice, so camera movement cannot slide the cloud pattern.
            if moved || !self.valid {
                self.center = (projected / 64.0).floor() * 64.0;
            }
            self.seconds = seconds;
            self.coverage = weather[0];
            self.valid = true;
        }
        update
    }
    fn uniform(&self) -> [f32; 4] {
        [
            self.center.x,
            self.center.y,
            1.0 / SPAN,
            self.valid as u32 as f32,
        ]
    }
}

pub struct CloudShadow {
    _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    state: CacheState,
    needs_draw: bool,
}
impl CloudShadow {
    pub fn new(device: &wgpu::Device, uniform: &wgpu::Buffer, shader: &wgpu::ShaderModule) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("32 km cloud shadow density cache (256 KiB)"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        // A uniform-only group avoids binding the sampled cache as its own
        // render attachment. Unused environment textures are not bound here.
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Cloud generation uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cloud generation inputs"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cloud density cache"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cached procedural cloud density"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_sky"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_cloud_cache"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R8Unorm,
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
        Self {
            _texture: texture,
            view,
            pipeline,
            group,
            state: CacheState::default(),
            needs_draw: true,
        }
    }
    pub fn prepare(&mut self, eye: Vec3, light: Vec3, weather: [f32; 4], seconds: f32) -> bool {
        self.needs_draw |= self.state.prepare(eye, light, weather, seconds);
        self.needs_draw
    }
    pub fn uniform(&self) -> [f32; 4] {
        self.state.uniform()
    }
    pub fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.needs_draw = false;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Refresh world cloud shadows"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_cache_handles_travel_weather_and_clock_changes() {
        let mut cache = CacheState::default();
        let eye = Vec3::new(-10700.0, 500.0, 58300.0);
        let light = Vec3::new(0.7, 0.5, -0.2).normalize();
        let clear = [0.1, 0.0, 0.0, 0.0];
        assert!(cache.prepare(eye, light, clear, 10.0));
        let original = cache.center;
        assert_eq!(original % 64.0, Vec2::ZERO);
        assert!(!cache.prepare(eye + Vec3::X * 100.0, light, clear, 10.05));
        assert_eq!(cache.center, original);
        assert!(cache.prepare(eye, light, clear, 10.2));
        assert!(cache.prepare(eye, light, [0.9, 0.8, 0.0, 0.7], 10.2));
        assert!(cache.prepare(eye + Vec3::Z * 90000.0, light, clear, 10.2));
        assert_ne!(cache.center, original);
        assert!(cache.prepare(eye, light, clear, 0.0));
        assert!(cache.uniform().iter().all(|x| x.is_finite()));
    }
}
