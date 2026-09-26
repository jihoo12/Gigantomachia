//! GPU-resident position-based fluid and marching-tetrahedra surface reconstruction.
use super::{BoxCollider, Fluid};
use crate::render::{EngineResult, Gpu};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

const BUCKETS: u32 = 8192;
const MAX_VERTICES: u32 = 600_000;
const MAX_COLLIDERS: usize = 64;
const STAGES: [&str; 13] = [
    "predict",
    "clear_grid",
    "build_grid",
    "density",
    "correct",
    "apply_correction",
    "velocity",
    "viscosity",
    "commit_velocity",
    "sample_field",
    "extract_surface",
    "finish_surface",
    "surface_diagnostics",
];
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    counts: [u32; 4],   // particles, colliders, hash buckets, vertex capacity
    shape: [u32; 4],    // samples xyz, unused
    origin: [f32; 4],   // xyz, sample spacing
    material: [f32; 4], // particle spacing, rest density, unused
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Particle {
    position: [f32; 4],
    previous: [f32; 4],
    velocity: [f32; 4],
    scratch: [f32; 4], // correction or smoothed velocity; lambda in w
}
/// Created on the same device used for rendering. Cloning an Arc shares the simulation.
/// No CPU readback occurs in step(), reconstruct(), or rendering.
#[derive(Debug)]
pub struct GpuFluid {
    particles: wgpu::Buffer,
    params: wgpu::Buffer,
    colliders: wgpu::Buffer,
    binding: wgpu::BindGroup,
    pipelines: Vec<wgpu::ComputePipeline>,
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indirect: wgpu::Buffer,
    config: Params,
}
impl std::fmt::Debug for Params {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Params")
            .field("particles", &self.counts[0])
            .field("grid", &self.shape)
            .finish()
    }
}
/// Explicit blocking diagnostics for tests/tools, never needed by the interactive renderer.
pub struct GpuFluidSnapshot {
    pub positions: Vec<Vec3>,
    pub vertex_count: u32,
    pub surface_overflow: bool,
    /// Particles whose reconstruction support crosses the configured surface domain.
    pub outside_surface_domain: u32,
}
impl GpuFluid {
    /// The fixed reconstruction domain bounds rendering only, not physical motion.
    pub fn new(gpu: &Gpu, seed: &Fluid, min: Vec3, max: Vec3) -> EngineResult<Self> {
        let cell = seed.spacing * 0.65;
        let extent = max - min;
        if !min.is_finite()
            || !max.is_finite()
            || !extent.is_finite()
            || extent.min_element() <= 0.0
        {
            return Err("GPU fluid surface bounds must be finite with min < max".into());
        }
        let dimensions = (extent / cell).ceil() + Vec3::ONE;
        if !dimensions.is_finite() || dimensions.max_element() > 600_000.0 {
            return Err("GPU fluid surface exceeds 600,000 samples".into());
        }
        let shape = dimensions.as_uvec3();
        let samples = u64::from(shape.x) * u64::from(shape.y) * u64::from(shape.z);
        if samples > 600_000 {
            return Err("GPU fluid surface exceeds 600,000 samples".into());
        }
        let config = Params {
            counts: [seed.particle_count() as u32, 0, BUCKETS, MAX_VERTICES],
            shape: [shape.x, shape.y, shape.z, 0],
            origin: [min.x, min.y, min.z, cell],
            material: [seed.spacing, seed.rest_density, 0.0, 0.0],
        };
        let storage = |label, size, extra| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | extra,
                mapped_at_creation: false,
            })
        };
        let initial = seed
            .positions
            .iter()
            .zip(&seed.velocities)
            .map(|(&p, &v)| Particle {
                position: p.extend(0.0).to_array(),
                previous: p.extend(0.0).to_array(),
                velocity: v.extend(0.0).to_array(),
                scratch: [0.0; 4],
            })
            .collect::<Vec<_>>();
        let particles = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fluid-particles"),
                contents: bytemuck::cast_slice(&initial),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        let params = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fluid-parameters"),
                contents: bytemuck::bytes_of(&config),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let heads = storage(
            "fluid-hash-heads",
            u64::from(BUCKETS) * 4,
            wgpu::BufferUsages::empty(),
        );
        let links = storage(
            "fluid-hash-links",
            seed.particle_count() as u64 * 4,
            wgpu::BufferUsages::empty(),
        );
        let colliders = storage(
            "fluid-colliders",
            MAX_COLLIDERS as u64 * 32,
            wgpu::BufferUsages::COPY_DST,
        );
        let field = storage(
            "fluid-density-field",
            samples * 16,
            wgpu::BufferUsages::empty(),
        );
        let vertices = storage(
            "fluid-generated-vertices",
            u64::from(MAX_VERTICES) * 36,
            wgpu::BufferUsages::VERTEX,
        );
        let indirect = storage(
            "fluid-draw-arguments",
            32,
            wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
        );
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("fluid-compute-layout"),
                entries: &(0..8)
                    .map(|binding| wgpu::BindGroupLayoutEntry {
                        binding,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: if binding == 0 {
                                wgpu::BufferBindingType::Uniform
                            } else {
                                wgpu::BufferBindingType::Storage {
                                    read_only: binding == 4,
                                }
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    })
                    .collect::<Vec<_>>(),
            });
        let binding = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fluid-compute-binding"),
            layout: &layout,
            entries: &[
                &params, &particles, &heads, &links, &colliders, &field, &vertices, &indirect,
            ]
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect::<Vec<_>>(),
        });
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("fluid-compute"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../shaders/fluid_compute.wgsl").into(),
                ),
            });
        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("fluid-compute"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
        let pipelines = STAGES
            .iter()
            .map(|entry| {
                gpu.device
                    .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(entry),
                        layout: Some(&pipeline_layout),
                        module: &shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        cache: None,
                    })
            })
            .collect();
        let result = Self {
            particles,
            params,
            colliders,
            binding,
            pipelines,
            vertices,
            indirect,
            config,
        };
        result.reconstruct(gpu);
        Ok(result)
    }
    fn dispatch(&self, encoder: &mut wgpu::CommandEncoder, stage: usize, count: u32) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(STAGES[stage]),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipelines[stage]);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
    }
    fn grid(&self, encoder: &mut wgpu::CommandEncoder) {
        self.dispatch(encoder, 1, BUCKETS);
        self.dispatch(encoder, 2, self.config.counts[0]);
    }
    /// Enqueue up to eight fixed 1/120 s steps with the supplied collider snapshot.
    /// Separate submissions preserve collider changes between batches without CPU waits.
    pub fn step(&self, gpu: &Gpu, colliders: &[BoxCollider], steps: u32) -> EngineResult<()> {
        if steps > 8 || colliders.len() > MAX_COLLIDERS {
            return Err("GPU fluid batches allow at most 8 steps and 64 colliders".into());
        }
        if steps == 0 {
            return Ok(());
        }
        let mut config = self.config;
        config.counts[1] = colliders.len() as u32;
        gpu.queue
            .write_buffer(&self.params, 0, bytemuck::bytes_of(&config));
        let boxes = colliders
            .iter()
            .map(|c| {
                [
                    c.min().extend(0.0).to_array(),
                    c.max().extend(0.0).to_array(),
                ]
            })
            .collect::<Vec<_>>();
        if !boxes.is_empty() {
            gpu.queue
                .write_buffer(&self.colliders, 0, bytemuck::cast_slice(&boxes));
        }
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        let count = self.config.counts[0];
        for _ in 0..steps {
            self.dispatch(&mut encoder, 0, count);
            for _ in 0..5 {
                self.grid(&mut encoder);
                for stage in [3, 4, 5] {
                    self.dispatch(&mut encoder, stage, count);
                }
            }
            self.grid(&mut encoder);
            for stage in [6, 7, 8] {
                self.dispatch(&mut encoder, stage, count);
            }
        }
        gpu.queue.submit([encoder.finish()]);
        Ok(())
    }
    /// Rebuild the surface once per changed frame, then draw directly from GPU buffers.
    pub fn reconstruct(&self, gpu: &Gpu) {
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        encoder.clear_buffer(&self.indirect, 0, None);
        self.grid(&mut encoder);
        let [x, y, z, _] = self.config.shape;
        self.dispatch(&mut encoder, 9, x * y * z);
        self.dispatch(&mut encoder, 10, (x - 1) * (y - 1) * (z - 1));
        self.dispatch(&mut encoder, 11, 1);
        self.dispatch(&mut encoder, 12, self.config.counts[0]);
        gpu.queue.submit([encoder.finish()]);
    }
    /// Wait for queued work and copy diagnostics to the CPU. Do not call in a frame loop.
    pub fn readback(&self, gpu: &Gpu) -> EngineResult<GpuFluidSnapshot> {
        let size = self.particles.size();
        let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fluid-debug-readback"),
            size: size + 32,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&self.particles, 0, &read, 0, size);
        encoder.copy_buffer_to_buffer(&self.indirect, 0, &read, size, 32);
        gpu.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv()??;
        let data = read.slice(..).get_mapped_range();
        let particles: &[Particle] = bytemuck::cast_slice(&data[..size as usize]);
        let args: &[u32] = bytemuck::cast_slice(&data[size as usize..]);
        let result = GpuFluidSnapshot {
            positions: particles
                .iter()
                .map(|p| Vec3::from_array(p.position[..3].try_into().unwrap()))
                .collect(),
            vertex_count: args[0],
            surface_overflow: args[4] != 0,
            outside_surface_domain: args[5],
        };
        drop(data);
        read.unmap();
        Ok(result)
    }
}
