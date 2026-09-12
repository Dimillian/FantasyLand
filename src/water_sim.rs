//! A bounded, camera-following shallow-water surface. Each cell exchanges real
//! volume with its neighbours; this is a small-wave solver, not a flood model.
//! Generated rivers/lakes remain the rest surface and far-water representation.
use crate::world::World;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

pub const WATER_GRID: usize = 96;
pub const WATER_CELL: f32 = 1.;
pub const WATER_STEP: f32 = 1. / 60.;
const MAX_STEPS: usize = 6;
const SHIFT_CELLS: i32 = 16;
const CELLS: usize = WATER_GRID * WATER_GRID;
const BUFFER_BYTES: u64 = (CELLS * 16) as u64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WaterParams {
    grid: [f32; 4],   // world x/z of first cell centre, cell size, count
    timing: [f32; 4], // fixed dt, simulation seconds, rain 0..1, active flag
    wind: [f32; 4],
    wake: [f32; 4],     // world position x/z, player velocity x/z
    activity: [f32; 4], // wake strength, remap shift x/z, hard-reset flag
}

pub struct WaterSim {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    state: wgpu::Buffer,
    scratch: wgpu::Buffer,
    bed: wgpu::Buffer,
    sample_uniform: wgpu::Buffer,
    step_uniform: wgpu::Buffer,
    velocity_groups: Vec<wgpu::BindGroup>,
    surface_groups: Vec<wgpu::BindGroup>,
    velocity_pipeline: wgpu::ComputePipeline,
    surface_pipeline: wgpu::ComputePipeline,
    remap_pipeline: wgpu::ComputePipeline,
    bathymetry: Vec<[f32; 4]>,
    origin: Option<[i32; 2]>,
    gpu_origin: Option<[i32; 2]>,
    eye: Vec3,
    previous_eye: Option<Vec3>,
    player_in_water: bool,
    pending: f32,
    motion_elapsed: f32,
    ticks: u64,
    wet_cells: usize,
    wind_offset: [f32; 2],
    world_seed: Option<u32>,
}

fn buffer_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    uniform: bool,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: if uniform {
                wgpu::BufferBindingType::Uniform
            } else {
                wgpu::BufferBindingType::Storage { read_only }
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl WaterSim {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let state = buffer(
            "Local shallow-water surface",
            BUFFER_BYTES,
            wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let scratch = buffer(
            "Local shallow-water flux scratch",
            BUFFER_BYTES,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let bed = buffer(
            "Generated water bathymetry",
            BUFFER_BYTES,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let sample_uniform = buffer(
            "Local water sampling transform",
            80,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let step_uniform = buffer(
            "Fixed water step parameters",
            (256 * MAX_STEPS) as u64,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Local water vertex and fragment sampling"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT, true, true),
                buffer_entry(1, wgpu::ShaderStages::VERTEX_FRAGMENT, false, true),
                buffer_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT, false, true),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Evolving water surface"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: sample_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: state.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bed.as_entire_binding(),
                },
            ],
        });
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shallow-water finite-volume compute"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::COMPUTE, true, true),
                buffer_entry(1, wgpu::ShaderStages::COMPUTE, false, true),
                buffer_entry(2, wgpu::ShaderStages::COMPUTE, false, false),
                buffer_entry(3, wgpu::ShaderStages::COMPUTE, false, true),
            ],
        });
        let groups = |input: &wgpu::Buffer, output: &wgpu::Buffer| {
            (0..MAX_STEPS)
                .map(|step| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Fixed-step shallow-water buffers"),
                        layout: &compute_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: &step_uniform,
                                    offset: (step * 256) as u64,
                                    size: std::num::NonZeroU64::new(80),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: input.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: output.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: bed.as_entire_binding(),
                            },
                        ],
                    })
                })
                .collect::<Vec<_>>()
        };
        let velocity_groups = groups(&state, &scratch);
        let surface_groups = groups(&scratch, &state);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Neighbour-exchanging shallow-water solver"),
            source: wgpu::ShaderSource::Wgsl(include_str!("water_sim.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shallow-water step layout"),
            bind_group_layouts: &[&compute_layout],
            push_constant_ranges: &[],
        });
        let pipeline = |entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            velocity_pipeline: pipeline("velocity_step"),
            surface_pipeline: pipeline("surface_step"),
            remap_pipeline: pipeline("remap_step"),
            layout,
            bind_group,
            state,
            scratch,
            bed,
            sample_uniform,
            step_uniform,
            velocity_groups,
            surface_groups,
            bathymetry: vec![[0.; 4]; CELLS],
            origin: None,
            gpu_origin: None,
            eye: Vec3::ZERO,
            previous_eye: None,
            player_in_water: false,
            pending: 0.,
            motion_elapsed: 0.,
            ticks: 0,
            wet_cells: 0,
            wind_offset: [0.; 2],
            world_seed: None,
        }
    }

    /// Bathymetry is cached in world-space metre cells. Moving the patch samples
    /// only exposed strips; the matching GPU remap retains all overlapping waves.
    pub fn update_world(&mut self, queue: &wgpu::Queue, world: &World, eye: Vec3) {
        if self.world_seed != Some(world.seed) {
            self.origin = None;
            self.gpu_origin = None;
            self.previous_eye = None;
            self.world_seed = Some(world.seed);
        }
        self.eye = eye;
        let s = world.sample(eye.x, eye.z);
        let feet = eye.y - 1.72;
        self.player_in_water = s.water_height > s.height + 0.04
            && feet < s.water_height + 0.24
            && feet > s.height - 0.3;
        let next = [
            ((eye.x / WATER_CELL).floor() as i32).div_euclid(SHIFT_CELLS) * SHIFT_CELLS
                - WATER_GRID as i32 / 2,
            ((eye.z / WATER_CELL).floor() as i32).div_euclid(SHIFT_CELLS) * SHIFT_CELLS
                - WATER_GRID as i32 / 2,
        ];
        if self.origin == Some(next) {
            return;
        }
        let shift = self.origin.map(|old| [next[0] - old[0], next[1] - old[1]]);
        let mut bed = vec![[0.; 4]; CELLS];
        let mut wet = 0;
        for z in 0..WATER_GRID {
            for x in 0..WATER_GRID {
                let old = shift.and_then(|d| {
                    let ox = x as i32 + d[0];
                    let oz = z as i32 + d[1];
                    (ox >= 0 && oz >= 0 && ox < WATER_GRID as i32 && oz < WATER_GRID as i32)
                        .then_some((oz * WATER_GRID as i32 + ox) as usize)
                });
                let value = if let Some(i) = old {
                    self.bathymetry[i]
                } else {
                    let wx = (next[0] as f32 + x as f32) * WATER_CELL;
                    let wz = (next[1] as f32 + z as f32) * WATER_CELL;
                    let sample = world.sample(wx, wz);
                    let depth = (sample.water_height - sample.height).max(0.);
                    let current = if depth > 0.04 {
                        world.water_flow(wx, wz)
                    } else {
                        [0.; 2]
                    };
                    // Only local surface waves are simulated: cap propagation
                    // depth for short waves and a fixed, conservative CFL bound.
                    [
                        if depth > 0.04 {
                            depth.clamp(0.05, 4.)
                        } else {
                            0.
                        },
                        sample.water_height,
                        current[0],
                        current[1],
                    ]
                };
                wet += usize::from(value[0] > 0.);
                bed[z * WATER_GRID + x] = value;
            }
        }
        self.origin = Some(next);
        self.bathymetry = bed;
        self.wet_cells = wet;
        queue.write_buffer(&self.bed, 0, bytemuck::cast_slice(&self.bathymetry));
    }

    /// The renderer calls this only when game animation advances. Captures and
    /// repeated render passes never advance the fluid. Excess catch-up is dropped.
    pub fn advance_time(&mut self, dt: f32) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(WATER_STEP * MAX_STEPS as f32);
        self.pending = (self.pending + dt).min(WATER_STEP * MAX_STEPS as f32);
        self.motion_elapsed = (self.motion_elapsed + dt).min(0.25);
    }

    /// Encode before any pass that samples `bind_group`. Rain is 0..1; wind is
    /// the same horizontal velocity vector as sky/foliage, in metres per second.
    pub fn encode(
        &mut self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        rain: f32,
        wind: [f32; 2],
    ) -> u32 {
        let Some(origin) = self.origin else {
            return 0;
        };
        let steps = ((self.pending + 1.0e-7) / WATER_STEP).floor() as usize;
        let steps = steps.min(MAX_STEPS);
        self.pending = (self.pending - WATER_STEP * steps as f32).max(0.);
        let shift = self
            .gpu_origin
            .map(|old| [origin[0] - old[0], origin[1] - old[1]])
            .unwrap_or([0, 0]);
        let remap = self.gpu_origin != self.origin;
        let reset = self.gpu_origin.is_none()
            || shift[0].abs() >= WATER_GRID as i32
            || shift[1].abs() >= WATER_GRID as i32;
        let velocity = self
            .previous_eye
            .map(|old| (self.eye - old) / self.motion_elapsed.max(1.0e-3))
            .unwrap_or(Vec3::ZERO);
        let speed = Vec3::new(velocity.x, 0., velocity.z).length();
        let wake_strength = if self.player_in_water && speed < 14. {
            (speed - 0.15).clamp(0., 5.)
        } else {
            0.
        };
        if steps > 0 {
            self.previous_eye = Some(self.eye);
            self.motion_elapsed = 0.;
        }
        let base = WaterParams {
            grid: [
                origin[0] as f32 * WATER_CELL,
                origin[1] as f32 * WATER_CELL,
                WATER_CELL,
                WATER_GRID as f32,
            ],
            timing: [
                WATER_STEP,
                self.ticks as f32 * WATER_STEP,
                rain.clamp(0., 1.),
                if self.wet_cells > 0 { 1. } else { 0. },
            ],
            wind: [wind[0], wind[1], self.wind_offset[0], self.wind_offset[1]],
            wake: [
                self.eye.x,
                self.eye.z,
                velocity.x.clamp(-14., 14.),
                velocity.z.clamp(-14., 14.),
            ],
            activity: [
                wake_strength,
                shift[0] as f32,
                shift[1] as f32,
                if reset { 1. } else { 0. },
            ],
        };
        queue.write_buffer(&self.sample_uniform, 0, bytemuck::bytes_of(&base));
        let mut bytes = vec![0u8; MAX_STEPS * 256];
        for i in 0..MAX_STEPS {
            let mut params = base;
            params.timing[1] = (self.ticks + i as u64) as f32 * WATER_STEP;
            params.wind[2] += wind[0] * WATER_STEP * i as f32 * 0.38;
            params.wind[3] += wind[1] * WATER_STEP * i as f32 * 0.38;
            bytes[i * 256..i * 256 + 80].copy_from_slice(bytemuck::bytes_of(&params));
        }
        queue.write_buffer(&self.step_uniform, 0, &bytes);
        if remap {
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Preserve overlapping water cells"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.remap_pipeline);
                pass.set_bind_group(0, &self.velocity_groups[0], &[]);
                pass.dispatch_workgroups(WATER_GRID as u32 / 8, WATER_GRID as u32 / 8, 1);
            }
            encoder.copy_buffer_to_buffer(&self.scratch, 0, &self.state, 0, BUFFER_BYTES);
            self.gpu_origin = self.origin;
        }
        self.wind_offset[0] += wind[0] * WATER_STEP * steps as f32 * 0.38;
        self.wind_offset[1] += wind[1] * WATER_STEP * steps as f32 * 0.38;
        if self.wet_cells == 0 {
            self.ticks += steps as u64;
            return 0;
        }
        for i in 0..steps {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Fixed 60 Hz shallow-water exchange"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.velocity_pipeline);
            pass.set_bind_group(0, &self.velocity_groups[i], &[]);
            pass.dispatch_workgroups(WATER_GRID as u32 / 8, WATER_GRID as u32 / 8, 1);
            pass.set_pipeline(&self.surface_pipeline);
            pass.set_bind_group(0, &self.surface_groups[i], &[]);
            pass.dispatch_workgroups(WATER_GRID as u32 / 8, WATER_GRID as u32 / 8, 1);
        }
        self.ticks += steps as u64;
        steps as u32
    }

    pub fn wet_cells(&self) -> usize {
        self.wet_cells
    }
    pub fn simulated_seconds(&self) -> f32 {
        self.ticks as f32 * WATER_STEP
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, serde::Serialize)]
pub struct WaterProbeReport {
    pub propagation_radius_m: f32,
    pub mass_error_m3: f32,
    pub maximum_height_m: f32,
    pub obstacle_leak_m: f32,
    pub rain_response_m: f32,
    pub wake_response_m: f32,
    pub remap_error_m: f32,
    pub frozen_error_m: f32,
}

#[cfg(not(target_arch = "wasm32"))]
impl WaterSim {
    fn read_state(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<[f32; 4]>, String> {
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Water solver readback"),
            size: BUFFER_BYTES,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.state, 0, &read, 0, BUFFER_BYTES);
        queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(wgpu::PollType::Wait)
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = read.slice(..).get_mapped_range();
        let output = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        read.unmap();
        Ok(output)
    }

    /// Uses the production compute pipelines, not an imitation CPU solver.
    /// A basin pulse must propagate, conserve interior volume and respect a dry
    /// barrier. Then rain, long-running stability, sliding and frozen renders
    /// are checked by reading actual GPU state.
    pub fn verify_gpu(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<WaterProbeReport, String> {
        let mut sim = Self::new(device);
        sim.origin = Some([0, 0]);
        sim.gpu_origin = Some([0, 0]);
        sim.wet_cells = CELLS;
        // Deep closed basin with a complete dry wall splitting east and west.
        for z in 0..WATER_GRID {
            for x in 0..WATER_GRID {
                sim.bathymetry[z * WATER_GRID + x] = [if x == 58 { 0. } else { 2. }, 10., 0., 0.];
            }
        }
        queue.write_buffer(&sim.bed, 0, bytemuck::cast_slice(&sim.bathymetry));
        let mut initial = vec![[0f32; 4]; CELLS];
        initial[48 * WATER_GRID + 50][0] = 0.1;
        queue.write_buffer(&sim.state, 0, bytemuck::cast_slice(&initial));
        let run = |sim: &mut Self, frames: usize, rain: f32, wind: [f32; 2]| {
            for _ in 0..frames {
                sim.advance_time(WATER_STEP * 6.);
                let mut encoder = device.create_command_encoder(&Default::default());
                sim.encode(queue, &mut encoder, rain, wind);
                queue.submit(Some(encoder.finish()));
            }
        };
        run(&mut sim, 20, 0., [0.; 2]);
        let spread = sim.read_state(device, queue)?;
        let mass = spread.iter().map(|s| s[0]).sum::<f32>();
        let mass_error = (mass - 0.1).abs();
        let mut radius = 0f32;
        let mut leak = 0f32;
        for z in 0..WATER_GRID {
            for x in 0..WATER_GRID {
                let eta = spread[z * WATER_GRID + x][0];
                if eta.abs() > 0.00003 {
                    radius =
                        radius.max(((x as f32 - 50.).powi(2) + (z as f32 - 48.).powi(2)).sqrt());
                }
                if x >= 58 {
                    leak = leak.max(eta.abs());
                }
            }
        }
        if radius < 6. || mass_error > 0.00003 || leak > 0.0000001 {
            return Err(format!(
                "Fluid exchange failed: radius={radius}, mass error={mass_error}, wall leak={leak}"
            ));
        }
        let before_freeze = sim.read_state(device, queue)?;
        for _ in 0..4 {
            let mut encoder = device.create_command_encoder(&Default::default());
            if sim.encode(queue, &mut encoder, 0., [0.; 2]) != 0 {
                return Err("Frozen render advanced water".into());
            }
            queue.submit(Some(encoder.finish()));
        }
        let frozen = sim.read_state(device, queue)?;
        let frozen_error = frozen
            .iter()
            .zip(&before_freeze)
            .flat_map(|(a, b)| a.iter().zip(b).map(|(a, b)| (a - b).abs()))
            .fold(0f32, f32::max);
        if frozen_error != 0. {
            return Err(format!("Frozen solver changed by {frozen_error}"));
        }
        queue.write_buffer(&sim.state, 0, bytemuck::cast_slice(&vec![[0f32; 4]; CELLS]));
        run(&mut sim, 100, 1., [0.; 2]);
        let rain = sim.read_state(device, queue)?;
        let rain_response = rain.iter().map(|s| s[0].abs()).sum::<f32>() / CELLS as f32;
        queue.write_buffer(&sim.state, 0, bytemuck::cast_slice(&vec![[0f32; 4]; CELLS]));
        sim.eye = Vec3::new(36., 11.72, 42.);
        sim.previous_eye = Some(sim.eye);
        sim.player_in_water = true;
        for _ in 0..30 {
            sim.eye.x += 0.2;
            run(&mut sim, 1, 0., [0.; 2]);
        }
        let wake = sim.read_state(device, queue)?;
        let wake_response = wake.iter().map(|s| s[0].abs()).fold(0f32, f32::max);
        sim.player_in_water = false;
        run(&mut sim, 600, 1., [14., -8.]);
        let storm = sim.read_state(device, queue)?;
        let maximum = storm.iter().map(|s| s[0].abs()).fold(0f32, f32::max);
        if storm.iter().flatten().any(|v| !v.is_finite())
            || maximum >= 0.239
            || rain_response < 0.00001
            || wake_response < 0.001
        {
            return Err(format!("Weather/stability failed: max={maximum}, rain response={rain_response}, wake response={wake_response}"));
        }
        // Move to a uniform ocean grid: remap must carry exact same world-cell
        // values into their new array indices, including velocity and foam.
        sim.bathymetry.fill([2., 10., 0., 0.]);
        queue.write_buffer(&sim.bed, 0, bytemuck::cast_slice(&sim.bathymetry));
        sim.origin = Some([16, 0]);
        let mut encoder = device.create_command_encoder(&Default::default());
        sim.encode(queue, &mut encoder, 0., [0.; 2]);
        queue.submit(Some(encoder.finish()));
        let shifted = sim.read_state(device, queue)?;
        let mut remap_error = 0f32;
        for z in 0..WATER_GRID {
            for x in 0..WATER_GRID - 16 {
                for channel in 0..4 {
                    remap_error = remap_error.max(
                        (shifted[z * WATER_GRID + x][channel]
                            - storm[z * WATER_GRID + x + 16][channel])
                            .abs(),
                    );
                }
            }
        }
        if remap_error != 0. {
            return Err(format!("Sliding water lost overlap: {remap_error}"));
        }
        Ok(WaterProbeReport {
            propagation_radius_m: radius,
            mass_error_m3: mass_error,
            maximum_height_m: maximum,
            obstacle_leak_m: leak,
            rain_response_m: rain_response,
            wake_response_m: wake_response,
            remap_error_m: remap_error,
            frozen_error_m: frozen_error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_step_cfl_covers_deep_cells_and_fastest_river_current() {
        // The gravity-wave and current Courant sum must stay safely below one.
        let courant = ((9.81f32 * 4.).sqrt() + 4.5) * WATER_STEP / WATER_CELL;
        assert!(
            courant < 0.20,
            "unstable local-wave discretisation: {courant}"
        );
    }
    #[test]
    fn shore_wave_limit_cannot_empty_a_wet_cell() {
        for depth in [0.05f32, 0.1, 0.3, 1., 4.] {
            let allowed = 0.24f32.min(depth * 0.24);
            assert!(depth - allowed >= depth * 0.75);
        }
    }
    #[test]
    fn sample_and_step_uniform_layout_match_wgsl() {
        assert_eq!(std::mem::size_of::<WaterParams>(), 5 * 16);
        assert!(WATER_GRID.is_multiple_of(8));
    }
}
