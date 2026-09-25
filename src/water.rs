//! A displaced water grid and procedural sky, usable with a surface or an offscreen target.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::{camera::Camera, render::Gpu};

const GRID_CELLS: u32 = 256;
const GRID_EXTENT: f32 = 256.0;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Clone, Copy, Debug)]
pub struct WaterSettings {
    /// Displacement multiplier, clamped to 0..2 on upload.
    pub amplitude: f32,
    /// Wave clock multiplier, clamped to 0..3 by the demo.
    pub speed: f32,
    pub paused: bool,
}

impl Default for WaterSettings {
    fn default() -> Self {
        Self {
            amplitude: 1.0,
            speed: 1.0,
            paused: false,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    camera_time: [f32; 4],
    // amplitude, grid origin x/z, reserved
    water: [f32; 4],
}

pub struct WaterRenderer {
    sky_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    depth: wgpu::TextureView,
    width: u32,
    height: u32,
}

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
    let mut indices = Vec::with_capacity((cells * cells * 6) as usize);
    for z in 0..cells {
        for x in 0..cells {
            let a = z * (cells + 1) + x;
            let b = a + cells + 1;
            // CCW when viewed from above (+Y).
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    (vertices, indices)
}

fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("water-depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl WaterRenderer {
    /// The target must use an sRGB format. Width and height must be nonzero.
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        assert!(format.is_srgb(), "water rendering requires an sRGB target");
        assert!(width > 0 && height > 0);
        let device = &gpu.device;
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water-uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water-uniform-bind-group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water-pipeline-layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ocean-wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/water.wgsl").into()),
        });
        let vertex_attributes = wgpu::vertex_attr_array![0 => Float32x2];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attributes,
        };
        let make_pipeline =
            |label, vertex, fragment, buffers: &[wgpu::VertexBufferLayout<'_>], water: bool| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(vertex),
                        buffers,
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(fragment),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        cull_mode: if water { Some(wgpu::Face::Back) } else { None },
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: water,
                        depth_compare: if water {
                            wgpu::CompareFunction::Less
                        } else {
                            wgpu::CompareFunction::Always
                        },
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    multiview: None,
                    cache: None,
                })
            };
        let sky_pipeline = make_pipeline("sky", "sky_vertex", "sky_fragment", &[], false);
        let water_pipeline = make_pipeline(
            "water",
            "water_vertex",
            "water_fragment",
            &[vertex_layout],
            true,
        );
        let (vertex_data, index_data) = grid(GRID_CELLS, GRID_EXTENT);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water-grid-vertices"),
            contents: bytemuck::cast_slice(&vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water-grid-indices"),
            contents: bytemuck::cast_slice(&index_data),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            sky_pipeline,
            water_pipeline,
            uniforms,
            bind_group,
            vertices,
            indices,
            index_count: index_data.len() as u32,
            depth: depth_view(device, width, height),
            width,
            height,
        }
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.depth = depth_view(&gpu.device, width, height);
    }

    /// Render one frame. Time is supplied by the caller for deterministic snapshots.
    pub fn draw(
        &self,
        gpu: &Gpu,
        target: &wgpu::TextureView,
        camera: &Camera,
        settings: &WaterSettings,
        time: f32,
    ) {
        let matrix = camera.view_projection(self.width as f32 / self.height as f32);
        let spacing = GRID_EXTENT / GRID_CELLS as f32;
        let uniforms = Uniforms {
            view_projection: matrix.to_cols_array_2d(),
            inverse_view_projection: matrix.inverse().to_cols_array_2d(),
            camera_time: [
                camera.position.x,
                camera.position.y,
                camera.position.z,
                time,
            ],
            water: [
                settings.amplitude.clamp(0.0, 2.0),
                (camera.position.x / spacing).floor() * spacing,
                (camera.position.z / spacing).floor() * spacing,
                0.0,
            ],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("water-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky-and-water"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_pipeline(&self.sky_pipeline);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.water_pipeline);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
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
