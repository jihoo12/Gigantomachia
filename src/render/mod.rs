//! Reusable forward renderer. Owns frame resources, draw order, and GPU mesh caching.

mod gpu;
mod mesh;
mod offscreen;
mod water;

use crate::scene::Scene;
use bytemuck::{Pod, Zeroable};
pub use gpu::{EngineResult, Gpu, instance};
use mesh::MeshPass;
pub use offscreen::OffscreenTarget;
use water::{GRID_CELLS, GRID_EXTENT, WaterPass};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniforms {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    camera_time: [f32; 4],
    water: [f32; 4],
}

pub struct Renderer {
    sky: wgpu::RenderPipeline,
    water: WaterPass,
    meshes: MeshPass,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth: wgpu::TextureView,
    width: u32,
    height: u32,
}

pub(super) fn validate_extent(gpu: &Gpu, width: u32, height: u32) -> EngineResult<()> {
    let max = gpu.device.limits().max_texture_dimension_2d;
    if width == 0 || height == 0 || width > max || height > max {
        return Err(format!("render dimensions must be in 1..={max}").into());
    }
    Ok(())
}

fn depth_view(gpu: &Gpu, width: u32, height: u32) -> wgpu::TextureView {
    gpu.device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("scene-depth"),
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

fn uniform_layout(gpu: &Gpu, size: u64) -> wgpu::BindGroupLayout {
    gpu.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size),
                },
                count: None,
            }],
        })
}

fn uniform_binding(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    size: u64,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uniform-buffer"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let binding = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("uniform-binding"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, binding)
}

fn pipeline(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    label: &str,
    source: &str,
    layouts: &[&wgpu::BindGroupLayout],
    buffers: &[wgpu::VertexBufferLayout<'_>],
    depth: bool,
) -> wgpu::RenderPipeline {
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
    let layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: layouts,
            push_constant_ranges: &[],
        });
    gpu.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: if depth { Some(wgpu::Face::Back) } else { None },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: depth,
                depth_compare: if depth {
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
}

impl Renderer {
    /// Create resources for an sRGB RGBA/BGRA target. The target view passed to render must match.
    pub fn new(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> EngineResult<Self> {
        validate_extent(gpu, width, height)?;
        if !matches!(
            format,
            wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            return Err("renderer requires an RGBA8/BGRA8 sRGB target".into());
        }
        let size = std::mem::size_of::<FrameUniforms>() as u64;
        let layout = uniform_layout(gpu, size);
        let (uniforms, bind_group) = uniform_binding(gpu, &layout, size);
        let source = format!(
            "{}\n{}",
            include_str!("../shaders/common.wgsl"),
            include_str!("../shaders/sky.wgsl")
        );
        Ok(Self {
            sky: pipeline(gpu, format, "sky", &source, &[&layout], &[], false),
            water: WaterPass::new(gpu, format, &layout),
            meshes: MeshPass::new(gpu, format, &layout),
            uniforms,
            bind_group,
            depth: depth_view(gpu, width, height),
            width,
            height,
        })
    }

    /// Ignore minimized dimensions; callers should suspend rendering until the target is nonzero.
    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) -> EngineResult<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        validate_extent(gpu, width, height)?;
        self.width = width;
        self.height = height;
        self.depth = depth_view(gpu, width, height);
        Ok(())
    }

    /// Upload newly encountered immutable meshes, then draw sky, opaque meshes, and optional water.
    /// All geometry shares one depth attachment, so water cannot paint over visible land.
    pub fn render(&mut self, gpu: &Gpu, target: &wgpu::TextureView, scene: &Scene) {
        self.meshes.prepare(gpu, &scene.meshes);
        let camera = &scene.camera;
        let water = scene.water.unwrap_or_default();
        let matrix = camera.view_projection(self.width as f32 / self.height as f32);
        let spacing = GRID_EXTENT / GRID_CELLS as f32;
        let uniforms = FrameUniforms {
            view_projection: matrix.to_cols_array_2d(),
            inverse_view_projection: matrix.inverse().to_cols_array_2d(),
            camera_time: [
                camera.position.x,
                camera.position.y,
                camera.position.z,
                water.time,
            ],
            water: [
                water.amplitude.clamp(0.0, 2.0),
                (camera.position.x / spacing).floor() * spacing,
                (camera.position.z / spacing).floor() * spacing,
                water.level,
            ],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene-forward"),
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
            pass.set_pipeline(&self.sky);
            pass.draw(0..3, 0..1);
            self.meshes.encode(&mut pass, &scene.meshes);
            if scene.water.is_some() {
                self.water.encode(&mut pass);
            }
        }
        gpu.queue.submit([encoder.finish()]);
    }

    /// Number of unique mesh assets currently resident, useful for resource lifetime checks.
    pub fn resident_meshes(&self) -> usize {
        self.meshes.resident_meshes()
    }
}
