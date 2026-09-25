//! Opaque vertex-colored mesh pass with shared geometry and per-instance transforms.

use super::{Gpu, pipeline, uniform_binding, uniform_layout};
use crate::{mesh::Vertex, scene::MeshInstance};
use bytemuck::{Pod, Zeroable};
use std::collections::{HashMap, HashSet};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniforms {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

pub(super) struct MeshPass {
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    reflection_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    geometry: HashMap<u64, GpuMesh>,
    objects: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
}

impl MeshPass {
    pub fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        frame_layout: &wgpu::BindGroupLayout,
        shadow_frame_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = uniform_layout(gpu, std::mem::size_of::<ObjectUniforms>() as u64);
        let source = format!(
            "{}\n{}",
            include_str!("../shaders/common.wgsl"),
            include_str!("../shaders/mesh.wgsl")
        );
        let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &attributes,
        };
        let shadow_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("shadow-shader"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}",
                        include_str!("../shaders/common.wgsl"),
                        include_str!("../shaders/shadow.wgsl")
                    )
                    .into(),
                ),
            });
        let shadow_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shadow-pipeline-layout"),
                bind_group_layouts: &[shadow_frame_layout, &layout],
                push_constant_ranges: &[],
            });
        let shadow_pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sun-shadow-pipeline"),
                layout: Some(&shadow_layout),
                vertex: wgpu::VertexState {
                    module: &shadow_shader,
                    entry_point: Some("vs_main"),
                    buffers: std::slice::from_ref(&vertex_layout),
                    compilation_options: Default::default(),
                },
                fragment: None,
                // Single-sided heightfields must cast shadows; use back-face culling, not front-face culling.
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: super::DEPTH_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            });
        Self {
            reflection_pipeline: super::pipeline_with_winding(
                gpu,
                format,
                "reflection-mesh",
                &source,
                &[frame_layout, &layout],
                std::slice::from_ref(&vertex_layout),
                Some(true),
                wgpu::FrontFace::Cw,
            ),
            shadow_pipeline,
            pipeline: pipeline(
                gpu,
                format,
                "opaque-mesh",
                &source,
                &[frame_layout, &layout],
                &[vertex_layout],
                Some(true),
            ),
            layout,
            geometry: HashMap::new(),
            objects: Vec::new(),
        }
    }

    pub fn prepare(&mut self, gpu: &Gpu, instances: &[MeshInstance]) {
        let active: HashSet<_> = instances
            .iter()
            .map(|instance| instance.mesh.id())
            .collect();
        self.geometry.retain(|id, _| active.contains(id));
        self.objects.truncate(instances.len());
        for (i, instance) in instances.iter().enumerate() {
            self.geometry
                .entry(instance.mesh.id())
                .or_insert_with(|| GpuMesh {
                    vertices: gpu
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("mesh-vertices"),
                            contents: bytemuck::cast_slice(instance.mesh.vertices()),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                    indices: gpu
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("mesh-indices"),
                            contents: bytemuck::cast_slice(instance.mesh.indices()),
                            usage: wgpu::BufferUsages::INDEX,
                        }),
                    count: instance.mesh.indices().len() as u32,
                });
            if self.objects.len() <= i {
                self.objects.push(uniform_binding(
                    gpu,
                    &self.layout,
                    std::mem::size_of::<ObjectUniforms>() as u64,
                ));
            }
            let model = instance.transform();
            gpu.queue.write_buffer(
                &self.objects[i].0,
                0,
                bytemuck::bytes_of(&ObjectUniforms {
                    model: model.to_cols_array_2d(),
                    normal: model.inverse().transpose().to_cols_array_2d(),
                }),
            );
        }
    }

    pub fn encode(&self, pass: &mut wgpu::RenderPass<'_>, instances: &[MeshInstance]) {
        self.encode_with(pass, instances, &self.pipeline);
    }

    pub fn encode_reflection(&self, pass: &mut wgpu::RenderPass<'_>, instances: &[MeshInstance]) {
        self.encode_with(pass, instances, &self.reflection_pipeline);
    }

    pub fn encode_shadow(&self, pass: &mut wgpu::RenderPass<'_>, instances: &[MeshInstance]) {
        self.encode_with(pass, instances, &self.shadow_pipeline);
    }

    fn encode_with(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        instances: &[MeshInstance],
        pipeline: &wgpu::RenderPipeline,
    ) {
        if instances.is_empty() {
            return;
        }
        pass.set_pipeline(pipeline);
        for (i, instance) in instances.iter().enumerate() {
            let mesh = &self.geometry[&instance.mesh.id()];
            pass.set_bind_group(1, &self.objects[i].1, &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
        }
    }

    pub fn resident_meshes(&self) -> usize {
        self.geometry.len()
    }
}
