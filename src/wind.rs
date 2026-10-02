//! A bounded, advected wind field. Weather supplies velocity and gust energy;
//! we integrate phase rather than multiplying changing velocity by world time.
//! Vegetation evaluates the same inexpensive field in every GPU pass.
use bytemuck::{Pod, Zeroable};
use glam::Vec2;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Pod, Zeroable)]
pub struct WindUniform {
    pub flow: [f32; 4],  // smoothed velocity XZ, strength, gust energy
    pub phase: [f32; 4], // three advected waves and leaf flutter
}
#[derive(Default)]
pub struct Wind {
    state: WindUniform,
}
impl Wind {
    pub fn advance(&mut self, dt: f32, velocity: [f32; 2], gust: f32) {
        if !dt.is_finite()
            || dt <= 0.
            || !velocity.iter().all(|v| v.is_finite())
            || !gust.is_finite()
        {
            return;
        }
        let dt = dt.min(0.1);
        let blend = 1. - (-dt / 1.8).exp();
        for (i, target) in velocity.into_iter().enumerate() {
            self.state.flow[i] += (target.clamp(-40., 40.) - self.state.flow[i]) * blend;
        }
        self.state.flow[3] += (gust.clamp(0., 1.) - self.state.flow[3]) * blend;
        let v = Vec2::new(self.state.flow[0], self.state.flow[1]);
        self.state.flow[2] = (v.length() / 11.).clamp(0., 2.5);
        let rates = [
            v.dot(Vec2::new(0.045, 0.027)),
            v.dot(Vec2::new(-0.021, 0.051)),
            v.dot(Vec2::new(0.013, -0.018)),
            3.2 + v.length() * 0.18,
        ];
        for (i, (p, rate)) in self.state.phase.iter_mut().zip(rates).enumerate() {
            *p = (*p + dt * rate).rem_euclid(std::f32::consts::TAU * if i == 3 { 4. } else { 1. });
        }
    }
    pub fn uniform(&self) -> WindUniform {
        self.state
    }
}
/// Canopy drag is generated once per ecological cell, never per frame.
/// Forest floor still stirs; upper crowns are given their own height response.
pub fn ground_exposure(tree_density: f32) -> f32 {
    1. - tree_density.clamp(0., 1.) * 0.72
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wind_is_bounded_pauses_and_turns_without_phase_jumps() {
        let mut wind = Wind::default();
        for _ in 0..600 {
            wind.advance(1. / 60., [24., 0.], 0.9);
        }
        let before = wind.uniform();
        wind.advance(0., [-24., 0.], 0.);
        wind.advance(f32::NAN, [0.; 2], 0.);
        assert_eq!(before, wind.uniform());
        wind.advance(1. / 60., [-24., 0.], 0.1);
        let after = wind.uniform();
        assert!((after.flow[0] - before.flow[0]).abs() < 0.5);
        for i in 0..4 {
            let delta = (after.phase[i] - before.phase[i] + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            assert!(delta.abs() < 0.15);
        }
        for _ in 0..1000 {
            wind.advance(0.1, [100.; 2], 10.);
        }
        assert!(wind.uniform().flow[2] <= 2.5);
        assert!(ground_exposure(0.9) < ground_exposure(0.1));
    }
    #[test]
    fn calm_and_storm_have_distinct_energy_and_repeatable_replay() {
        let mut a = Wind::default();
        let mut b = Wind::default();
        let mut storm = Wind::default();
        for _ in 0..1200 {
            a.advance(1. / 60., [2., 1.], 0.1);
            b.advance(1. / 60., [2., 1.], 0.1);
            storm.advance(1. / 60., [24., 12.], 0.9);
        }
        assert_eq!(a.uniform(), b.uniform());
        assert!(storm.uniform().flow[2] > a.uniform().flow[2] * 8.);
    }
    #[test]
    fn shared_wind_shader_validates() {
        let source = include_str!("wind.wgsl");
        let module = wgpu::naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn gpu_roots_shelter_strength_and_traveling_gusts() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&Default::default());
            let adapter = instance.request_adapter(&Default::default()).await.unwrap();
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            let source = concat!(
                include_str!("wind.wgsl"),
                r#"
@group(0) @binding(0) var<storage,read_write> result:array<vec4<f32>>;
@compute @workgroup_size(1) fn check(@builtin(global_invocation_id) id:vec3<u32>) {
    let i=id.x; let p=vec3<f32>(120000.0,30.0,-85000.0);
    var field=WindField(vec4<f32>(22.0,10.0,2.2,0.9),vec4<f32>(0.4,1.2,0.8,3.0));
    var material=6.4; var response= -1.0;
    if i==0u {material=6.0;}
    if i==1u {material=1.4;response=0.0;}
    if i==2u {material=3.0;response=0.0;}
    if i==3u {material=2.0;response=0.0;}
    if i==4u {field.flow.z=0.2;}
    if i==6u {response= -0.3;}
    if i==7u {material=3.0;response=0.9;}
    if i==8u {material=3.0;response=0.3;}
    if i==9u {field.phase+=vec4<f32>(1.0);}
    result[i]=vec4<f32>(wind_displacement(p,material,response,field),0.0);
}
"#
            );
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Wind deformation regression"),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("check"),
                compilation_options: Default::default(),
                cache: None,
            });
            let output = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 160,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let read = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 160,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output.as_entire_binding(),
                }],
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(10, 1, 1);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &read, 0, 160);
            queue.submit(Some(encoder.finish()));
            let (tx, rx) = std::sync::mpsc::channel();
            read.slice(..)
                .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            device.poll(wgpu::PollType::Wait).unwrap();
            rx.recv().unwrap().unwrap();
            let mapped = read.slice(..).get_mapped_range();
            let values: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
            for v in &values[..4] {
                assert!(
                    v.iter().all(|x| x.abs() < 0.00001),
                    "roots/static props must stay fixed: {v:?}"
                );
            }
            let length = |i: usize| values[i][0].hypot(values[i][2]);
            assert!(length(5) > length(4) * 8.);
            assert!(length(6) < length(5) * 0.4);
            assert!(length(7) > length(8) * 2.5);
            assert!(
                (values[5][0] - values[9][0]).abs() + (values[5][2] - values[9][2]).abs() > 0.01
            );
            assert!(values
                .iter()
                .flatten()
                .all(|x| x.is_finite() && x.abs() < 3.));
        });
    }
}
