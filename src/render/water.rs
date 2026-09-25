//! Water geometry and draw encoding. Frame submission belongs to Renderer.

use super::{Gpu, pipeline};
use wgpu::util::DeviceExt;

pub(super) const GRID_CELLS: u32 = 256;
pub(super) const GRID_EXTENT: f32 = 256.0;

fn grid(cells: u32, extent: f32) -> (Vec<[f32; 2]>, Vec<u32>) {
    assert!(cells > 0);
    let mut vertices = Vec::with_capacity(((cells + 1) * (cells + 1)) as usize);
    for z in 0..=cells {
        for x in 0..=cells {
            vertices.push([
                (x as f32 / cells as f32 - 0.5) * extent,
                (z as f32 / cells as f32 - 0.5) * extent,
            ]);
        }
    }
    (vertices, crate::mesh::grid_indices(cells))
}

pub(super) struct WaterPass {
    pipeline: wgpu::RenderPipeline,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

impl WaterPass {
    pub fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        layout: &wgpu::BindGroupLayout,
        inputs: &wgpu::BindGroupLayout,
    ) -> Self {
        let attributes = wgpu::vertex_attr_array![0 => Float32x2];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        };
        let source = format!(
            "{}\n{}",
            include_str!("../shaders/common.wgsl"),
            include_str!("../shaders/water.wgsl")
        );
        let pipeline = pipeline(
            gpu,
            format,
            "water",
            &source,
            &[layout, inputs],
            &[vertex_layout],
            Some(true),
        );
        let (vertices, indices) = grid(GRID_CELLS, GRID_EXTENT);
        let vertex_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water-grid"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("water-indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        Self {
            pipeline,
            vertices: vertex_buffer,
            indices: index_buffer,
            index_count: indices.len() as u32,
        }
    }

    pub fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_bounded_and_faces_up() {
        let (vertices, indices) = grid(8, 16.0);
        assert_eq!(vertices.len(), 81);
        assert_eq!(indices.len(), 384);
        for point in &vertices {
            assert!(point.iter().all(|value| value.abs() <= 8.0));
        }
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| {
                let [x, z] = vertices[i as usize];
                glam::Vec3::new(x, 0.0, z)
            });
            assert!((b - a).cross(c - a).y > 0.0);
        }
    }
}
