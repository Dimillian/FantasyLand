//! Shadow-resolved participating media, evaluated at quarter pixel count.
//! The composite reads depth only and blends into HDR before bloom. It never
//! samples the scene color attachment, so it needs neither a color copy nor an
//! additional global bind group. Stable spatial dithering avoids temporal boil.
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

/// All coordinates use the same camera-relative projection as the world pass.
pub struct RaysState {
    pub view_projection: Mat4,
    pub shadow_matrix: Mat4,
    pub eye: Vec3,
    pub shadow_origin: Vec3,
    pub light_direction: Vec3,
    pub light_color: Vec3,
    pub light_intensity: f32,
    pub shadow_strength: f32,
    pub shadows_enabled: bool,
    pub shadow_radius: f32,
    pub woodland: f32,
    pub humidity: f32,
    pub fog_base: f32,
    pub cloud_cover: f32,
    pub rain: f32,
    pub fog: f32,
    pub quality: u32,
    pub room: [[f32; 4]; 3],
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_cascade_atmosphere_modules_validate() {
        for pass in [
            include_str!("rays_march.wgsl"),
            include_str!("rays_composite.wgsl"),
        ] {
            let source = [
                include_str!("rays_common.wgsl"),
                pass,
                include_str!("rays_integrate.wgsl"),
                include_str!("room_probe.wgsl"),
            ]
            .join("\n");
            let module = wgpu::naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
        assert_eq!(std::mem::size_of::<super::Uniform>() % 16, 0);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniform {
    inverse_view_projection: [[f32; 4]; 4],
    shadow_matrix: [[f32; 4]; 4],
    camera: [f32; 4], // absolute XYZ, local fog base
    shadow_origin: [f32; 4],
    light_direction: [f32; 4], // direction toward light, strength
    light_color: [f32; 4],
    atmosphere: [f32; 4], // extinction per metre, range, height falloff, phase g
    shadow: [f32; 4],     // radius, enabled, strength, step count
    room: [[f32; 4]; 3],
    resolution: [f32; 4], // scene width/height, ray width/height
    far_shadow_matrix: [[f32; 4]; 4],
    cascade_params: [f32; 4],
}

struct Targets {
    _scattering: wgpu::Texture,
    scattering: wgpu::TextureView,
    _distance: wgpu::Texture,
    distance: wgpu::TextureView,
    size: [u32; 2],
}

pub struct Rays {
    march_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    march_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    march_group: wgpu::BindGroup,
    composite_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    targets: Targets,
    size: [u32; 2],
    active: bool,
    far_shadow: Option<wgpu::TextureView>,
    cascade_state: Option<crate::shadow::CascadeState>,
}

impl Rays {
    pub fn new(
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        shadow: &wgpu::TextureView,
        comparison: &wgpu::Sampler,
        size: [u32; 2],
    ) -> Self {
        let uniform_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let texture = |binding, depth| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: if depth {
                    wgpu::TextureSampleType::Depth
                } else {
                    // Only textureLoad is used. No optional filtering features.
                    wgpu::TextureSampleType::Float { filterable: false }
                },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let march_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Volumetric shadow integration inputs"),
            entries: &[
                uniform_entry,
                texture(1, true),
                texture(2, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                texture(4, true),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Depth bilateral atmospheric reconstruction"),
            entries: &[
                uniform_entry,
                texture(1, true),
                texture(2, false),
                texture(3, false),
                texture(4, true),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                texture(6, true),
            ],
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Volumetric atmosphere camera"),
            contents: bytemuck::bytes_of(&Uniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow raymarched sun and moon shafts"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("rays_common.wgsl"),
                    include_str!("rays_march.wgsl"),
                    include_str!("rays_integrate.wgsl"),
                    "\n",
                    include_str!("room_probe.wgsl")
                )
                .into(),
            ),
        });
        // Separate modules allow both passes to use group 0 with different
        // resource types; this keeps all renderer/world bindings independent.
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Depth bilateral shaft composite"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("rays_common.wgsl"),
                    include_str!("rays_composite.wgsl"),
                    include_str!("rays_integrate.wgsl"),
                    "\n",
                    include_str!("room_probe.wgsl")
                )
                .into(),
            ),
        });
        let make = |label,
                    module: &wgpu::ShaderModule,
                    layout: &wgpu::BindGroupLayout,
                    entry,
                    targets: &[Option<wgpu::ColorTargetState>]| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[layout],
                push_constant_ranges: &[],
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets,
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let march_pipeline = make(
            "Quarter-area participating media",
            &shader,
            &march_layout,
            "fs_rays",
            &[
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                }),
                Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::R16Float,
                    blend: None,
                    write_mask: wgpu::ColorWrites::RED,
                }),
            ],
        );
        let composite_pipeline = make(
            "Depth-aware scattering into scene before bloom",
            &composite_shader,
            &composite_layout,
            "fs_composite",
            &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba16Float,
                blend: Some(wgpu::BlendState {
                    // RGB = inscattering + scene * transmittance.
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::SrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        );
        let targets = Self::targets(device, size);
        let march_group = Self::march_group(
            device,
            &march_layout,
            &uniform,
            depth,
            shadow,
            shadow,
            comparison,
        );
        let composite_group = Self::composite_group(
            device,
            &composite_layout,
            &uniform,
            depth,
            &targets,
            shadow,
            shadow,
            comparison,
        );
        Self {
            march_layout,
            composite_layout,
            march_pipeline,
            composite_pipeline,
            march_group,
            composite_group,
            uniform,
            targets,
            size,
            active: false,
            far_shadow: None,
            cascade_state: None,
        }
    }

    fn targets(device: &wgpu::Device, size: [u32; 2]) -> Targets {
        let size = [size[0].div_ceil(2).max(1), size[1].div_ceil(2).max(1)];
        let texture = |label, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let scattering = texture(
            "Half-width atmospheric radiance and transmittance",
            wgpu::TextureFormat::Rgba16Float,
        );
        let distance = texture(
            "Conservative atmospheric integration distance",
            wgpu::TextureFormat::R16Float,
        );
        Targets {
            scattering: scattering.create_view(&Default::default()),
            _scattering: scattering,
            distance: distance.create_view(&Default::default()),
            _distance: distance,
            size,
        }
    }

    fn march_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        depth: &wgpu::TextureView,
        shadow: &wgpu::TextureView,
        far_shadow: &wgpu::TextureView,
        comparison: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atmospheric march resources"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(shadow),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(comparison),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(far_shadow),
                },
            ],
        })
    }

    fn composite_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        depth: &wgpu::TextureView,
        targets: &Targets,
        shadow: &wgpu::TextureView,
        far_shadow: &wgpu::TextureView,
        comparison: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atmospheric edge reconstruction resources"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&targets.scattering),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&targets.distance),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(shadow),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(comparison),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(far_shadow),
                },
            ],
        })
    }

    /// Call whenever the scene depth texture changes (resize or internal scale).
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        shadow: &wgpu::TextureView,
        comparison: &wgpu::Sampler,
        size: [u32; 2],
    ) {
        self.size = size;
        self.targets = Self::targets(device, size);
        self.march_group = Self::march_group(
            device,
            &self.march_layout,
            &self.uniform,
            depth,
            shadow,
            self.far_shadow.as_ref().unwrap_or(shadow),
            comparison,
        );
        self.composite_group = Self::composite_group(
            device,
            &self.composite_layout,
            &self.uniform,
            depth,
            &self.targets,
            shadow,
            self.far_shadow.as_ref().unwrap_or(shadow),
            comparison,
        );
    }

    /// Rebind only when either sunlight texture changes; scene targets survive.
    pub fn set_cascade_shadow(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        primary: &wgpu::TextureView,
        far: &wgpu::TextureView,
        comparison: &wgpu::Sampler,
    ) {
        self.far_shadow = Some(far.clone());
        self.march_group = Self::march_group(
            device,
            &self.march_layout,
            &self.uniform,
            depth,
            primary,
            far,
            comparison,
        );
        self.composite_group = Self::composite_group(
            device,
            &self.composite_layout,
            &self.uniform,
            depth,
            &self.targets,
            primary,
            far,
            comparison,
        );
    }
    pub fn set_cascade_state(&mut self, state: Option<crate::shadow::CascadeState>) {
        self.cascade_state = state;
    }

    pub fn update(&mut self, queue: &wgpu::Queue, state: &RaysState) {
        // Clouded and rainy weather suppresses direct shafts. This leaves the
        // existing large-scale weather fog in charge of overcast/storm scenes.
        let cloud = state.cloud_cover.clamp(0.0, 1.0);
        let cloud_transmission = 1.0 - cloud.powi(3) * 0.91;
        let strength = state.light_intensity.max(0.0)
            * cloud_transmission
            * (1.0 - state.rain.clamp(0.0, 1.0) * 0.72);
        self.active = state.shadows_enabled && state.shadow_strength > 0.001 && strength > 0.001;
        let density = 0.00032
            + state.woodland.clamp(0.0, 1.0) * 0.0012
            + state.humidity.clamp(0.0, 1.0) * 0.00040
            + state.fog.clamp(0.0, 1.0) * 0.0017;
        let uniform = Uniform {
            inverse_view_projection: state.view_projection.inverse().to_cols_array_2d(),
            shadow_matrix: state.shadow_matrix.to_cols_array_2d(),
            camera: state.eye.extend(state.fog_base).to_array(),
            shadow_origin: state.shadow_origin.extend(1.0).to_array(),
            light_direction: state
                .light_direction
                .normalize_or_zero()
                .extend(strength)
                .to_array(),
            light_color: state.light_color.extend(0.48).to_array(),
            atmosphere: [
                density,
                (state.shadow_radius * 0.60).clamp(24.0, 192.0),
                0.028,
                0.58,
            ],
            shadow: [
                state.shadow_radius,
                state.shadows_enabled as u32 as f32,
                state.shadow_strength,
                [12.0, 16.0, 20.0][state.quality.min(2) as usize],
            ],
            room: state.room,
            resolution: [
                self.size[0] as f32,
                self.size[1] as f32,
                self.targets.size[0] as f32,
                self.targets.size[1] as f32,
            ],
            far_shadow_matrix: self
                .cascade_state
                .map_or(Mat4::IDENTITY, |s| s.far_matrix)
                .to_cols_array_2d(),
            cascade_params: self.cascade_state.map_or([0.0; 4], |s| s.params),
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
    }

    /// After opaque, water and precipitation, before PostProcess::render. Water
    /// writes scene depth, so integration ends at its actual moving surface.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        profile: &crate::gpu_profile::GpuProfile,
    ) {
        if !self.active {
            return;
        }
        {
            let attachment = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Shadow-resolved volumetric atmosphere"),
                color_attachments: &[
                    attachment(&self.targets.scattering),
                    attachment(&self.targets.distance),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: profile.pass(5),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.march_pipeline);
            pass.set_bind_group(0, &self.march_group, &[]);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bilateral HDR atmosphere composition"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: profile.pass(6),
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &self.composite_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
