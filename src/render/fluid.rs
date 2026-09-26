//! Streams reconstructed fluid surfaces into reusable GPU buffers, independently of ocean geometry.
use super::{Gpu, pipeline};
use crate::{fluid::FluidSurface, mesh::Vertex};
struct SurfaceBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    vertex_bytes: u64,
    index_bytes: u64,
    count: u32,
}
pub(super) struct FluidPass {
    pipeline: wgpu::RenderPipeline,
    surfaces: Vec<SurfaceBuffers>,
}
impl FluidPass {
    pub fn new(gpu: &Gpu, frame: &wgpu::BindGroupLayout, inputs: &wgpu::BindGroupLayout) -> Self {
        let attributes = wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x3];
        Self {
            pipeline: pipeline(
                gpu,
                super::targets::HDR_FORMAT,
                "fluid-surface",
                &format!(
                    "{}\n{}",
                    include_str!("../shaders/common.wgsl"),
                    include_str!("../shaders/fluid.wgsl")
                ),
                &[frame, inputs],
                &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &attributes,
                }],
                Some(true),
            ),
            surfaces: Vec::new(),
        }
    }
    pub fn prepare(&mut self, gpu: &Gpu, surfaces: &[FluidSurface]) {
        self.surfaces.truncate(surfaces.len());
        for (i, surface) in surfaces.iter().enumerate() {
            let vb = bytemuck::cast_slice::<_, u8>(&surface.vertices);
            let ib = bytemuck::cast_slice::<_, u8>(&surface.indices);
            if i == self.surfaces.len()
                || self.surfaces[i].vertex_bytes < (vb.len() as u64)
                || self.surfaces[i].index_bytes < (ib.len() as u64)
            {
                let vertex_bytes = (vb.len() as u64).max(4).next_power_of_two();
                let index_bytes = (ib.len() as u64).max(4).next_power_of_two();
                let buffer = |size, usage| {
                    gpu.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("dynamic-fluid"),
                        size,
                        usage,
                        mapped_at_creation: false,
                    })
                };
                let entry = SurfaceBuffers {
                    vertices: buffer(
                        vertex_bytes,
                        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    ),
                    indices: buffer(
                        index_bytes,
                        wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    ),
                    vertex_bytes,
                    index_bytes,
                    count: 0,
                };
                if i == self.surfaces.len() {
                    self.surfaces.push(entry);
                } else {
                    self.surfaces[i] = entry;
                }
            }
            let entry = &mut self.surfaces[i];
            entry.count = surface.indices.len() as u32;
            if !vb.is_empty() {
                gpu.queue.write_buffer(&entry.vertices, 0, vb);
            }
            if !ib.is_empty() {
                gpu.queue.write_buffer(&entry.indices, 0, ib);
            }
        }
    }
    pub fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        for surface in &self.surfaces {
            if surface.count == 0 {
                continue;
            }
            pass.set_vertex_buffer(0, surface.vertices.slice(..));
            pass.set_index_buffer(surface.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..surface.count, 0, 0..1);
        }
    }
}
