//! Reusable forward renderer. Owns frame resources, draw order, and GPU mesh caching.

mod gpu;
mod mesh;
mod offscreen;
mod shadow;
mod targets;
mod water;
mod waterfall;

use crate::{scene::Scene, water::WaterStyle};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
pub use gpu::{EngineResult, Gpu, instance};
use mesh::MeshPass;
pub use offscreen::OffscreenTarget;
use shadow::ShadowMap;
use targets::{HDR_FORMAT, SceneTargets};
use water::{GRID_CELLS, GRID_EXTENT, WaterPass};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniforms {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    light_view_projection: [[f32; 4]; 4],
    camera_time: [f32; 4],
    water: [f32; 4],
    sun: [f32; 4],
    effects: [f32; 4],
    absorption: [f32; 4],
    surface: [f32; 4],
    water_bounds: [f32; 4],
    waterfall_origin: [f32; 4],
    waterfall_shape: [f32; 4],
    reflection_view_projection: [[f32; 4]; 4],
    reflection: [f32; 4],
    secondary_water: [f32; 4],
    secondary_bounds: [f32; 4],
}

pub struct Renderer {
    sky: wgpu::RenderPipeline,
    water: WaterPass,
    waterfall: waterfall::WaterfallPass,
    meshes: MeshPass,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    reflection_uniforms: wgpu::Buffer,
    reflection_binding: wgpu::BindGroup,
    shadow: ShadowMap,
    shadow_binding: wgpu::BindGroup,
    targets: SceneTargets,
    water_layout: wgpu::BindGroupLayout,
    post_layout: wgpu::BindGroupLayout,
    post: wgpu::RenderPipeline,
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
    depth: Option<bool>,
) -> wgpu::RenderPipeline {
    pipeline_with_winding(
        gpu,
        format,
        label,
        source,
        layouts,
        buffers,
        depth,
        wgpu::FrontFace::Ccw,
    )
}

#[allow(clippy::too_many_arguments)]
fn pipeline_with_winding(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    label: &str,
    source: &str,
    layouts: &[&wgpu::BindGroupLayout],
    buffers: &[wgpu::VertexBufferLayout<'_>],
    depth: Option<bool>,
    front_face: wgpu::FrontFace,
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
                front_face,
                cull_mode: if depth == Some(true) {
                    Some(wgpu::Face::Back)
                } else {
                    None
                },
                ..Default::default()
            },
            depth_stencil: depth.map(|write| wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: write,
                depth_compare: if write {
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
        let shadow_layout = uniform_layout(gpu, size);
        let (uniforms, shadow_binding) = uniform_binding(gpu, &shadow_layout, size);
        let shadow = ShadowMap::new(gpu);
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("scene-frame-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(size),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                ],
            });
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-frame-bindings"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow.sampler),
                },
            ],
        });
        let reflection_uniforms = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reflection-frame-uniforms"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let reflection_binding = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene-frame-bindings"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: reflection_uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow.sampler),
                },
            ],
        });
        let water_layout = targets::water_layout(gpu);
        let post_layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("post-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
        let source = format!(
            "{}\n{}",
            include_str!("../shaders/common.wgsl"),
            include_str!("../shaders/sky.wgsl")
        );
        Ok(Self {
            sky: pipeline(
                gpu,
                HDR_FORMAT,
                "sky",
                &source,
                &[&layout],
                &[],
                Some(false),
            ),
            waterfall: waterfall::WaterfallPass::new(gpu, &layout, &water_layout),
            water: WaterPass::new(gpu, HDR_FORMAT, &layout, &water_layout),
            meshes: MeshPass::new(gpu, HDR_FORMAT, &layout, &shadow_layout),
            post: pipeline(
                gpu,
                format,
                "tone-map",
                include_str!("../shaders/post.wgsl"),
                &[&post_layout],
                &[],
                None,
            ),
            targets: SceneTargets::new(gpu, width, height, &water_layout, &post_layout),
            uniforms,
            bind_group,
            reflection_uniforms,
            reflection_binding,
            shadow,
            shadow_binding,
            water_layout,
            post_layout,
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
        self.targets = SceneTargets::new(gpu, width, height, &self.water_layout, &self.post_layout);
        Ok(())
    }

    /// Render shadow depth, opaque HDR color/depth, refractive water, then tone-map once.
    /// Sampled opaque depth is distinct from the water attachment to avoid read/write feedback.
    pub fn render(&mut self, gpu: &Gpu, target: &wgpu::TextureView, scene: &Scene) {
        self.meshes.prepare(gpu, &scene.meshes);
        let camera = &scene.camera;
        let water = scene.water.unwrap_or_default();
        let secondary_water = scene.secondary_water;
        let detailed = water.style == WaterStyle::Realistic;
        let secondary_detailed = secondary_water.is_some_and(|w| w.style == WaterStyle::Realistic);
        self.water.prepare(gpu, (scene.water.is_some() || secondary_water.is_some()) && (detailed || secondary_detailed));
        let sun = scene.sun.direction();
        let matrix = camera.view_projection(self.width as f32 / self.height as f32);
        let reflection_enabled =
            scene.water.is_some() && water.reflections && camera.position.y > water.level;
        let mirror = Mat4::from_translation(Vec3::Y * (2.0 * water.level))
            * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        let reflection_matrix = matrix * mirror;
        let spacing = GRID_EXTENT / GRID_CELLS as f32;
        let uniforms = FrameUniforms {
            reflection_view_projection: reflection_matrix.to_cols_array_2d(),
            reflection: [f32::from(reflection_enabled), water.level, 0.0, 0.0],
            secondary_water: secondary_water.map_or([0.0; 4], |w| [
                w.amplitude.clamp(0.0, 2.0), w.level, 1.0, 0.0
            ]),
            secondary_bounds: secondary_water.and_then(|w| w.bounds).map_or([0.0; 4], |bounds| [
                bounds.center.x, bounds.center.y, bounds.half_extent.x, bounds.half_extent.y
            ]),
            view_projection: matrix.to_cols_array_2d(),
            inverse_view_projection: matrix.inverse().to_cols_array_2d(),
            light_view_projection: shadow::matrix(
                Vec3::new(camera.position.x, water.level, camera.position.z),
                sun,
            )
            .to_cols_array_2d(),
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
            sun: [sun.x, sun.y, sun.z, f32::from(scene.sun.shadows)],
            effects: [
                f32::from(water.refraction),
                water.refraction_strength.clamp(0.0, 1.0),
                water.foam_strength.clamp(0.0, 1.0),
                water.foam_width.clamp(0.05, 10.0),
            ],
            waterfall_origin: water
                .waterfall
                .map_or([0.0; 4], |f| [f.origin.x, f.origin.y, f.origin.z, f.width]),
            waterfall_shape: water
                .waterfall
                .map_or([0.0; 4], |f| [f.direction.x, f.direction.y, f.drop, 1.0]),
            water_bounds: water.bounds.map_or([0.0; 4], |bounds| {
                [
                    bounds.center.x,
                    bounds.center.y,
                    bounds.half_extent.x,
                    bounds.half_extent.y,
                ]
            }),
            surface: [
                f32::from(detailed),
                if water.ripple_strength.is_finite() {
                    water.ripple_strength.clamp(0.0, 2.0)
                } else {
                    1.0
                },
                if water.roughness.is_finite() {
                    water.roughness.clamp(0.08, 0.6)
                } else {
                    0.22
                },
                0.0,
            ],
            absorption: [
                water.absorption[0].clamp(0.0, 10.0),
                water.absorption[1].clamp(0.0, 10.0),
                water.absorption[2].clamp(0.0, 10.0),
                0.0,
            ],
        };
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        if reflection_enabled {
            let mut reflected = uniforms;
            reflected.view_projection = reflection_matrix.to_cols_array_2d();
            reflected.inverse_view_projection = reflection_matrix.inverse().to_cols_array_2d();
            reflected.camera_time[1] = 2.0 * water.level - camera.position.y;
            reflected.reflection[3] = 1.0;
            gpu.queue
                .write_buffer(&self.reflection_uniforms, 0, bytemuck::bytes_of(&reflected));
        }
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene-frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun-shadow-pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // This binding contains only the uniform buffer, not the shadow texture being written.
            pass.set_bind_group(0, &self.shadow_binding, &[]);
            if scene.sun.shadows {
                self.meshes.encode_shadow(&mut pass, &scene.meshes);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque-hdr-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.opaque_color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.opaque_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
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
        }
        if reflection_enabled {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("planar-reflection-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.reflection_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.reflection_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.reflection_binding, &[]);
            self.meshes.encode_reflection(&mut pass, &scene.meshes);
        }
        let extent = wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        };
        encoder.copy_texture_to_texture(
            self.targets.opaque_color.as_image_copy(),
            self.targets.composite_color.as_image_copy(),
            extent,
        );
        if scene.water.is_some() || scene.secondary_water.is_some() {
            encoder.copy_texture_to_texture(
                self.targets.opaque_depth.as_image_copy(),
                self.targets.water_depth.as_image_copy(),
                extent,
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("water-composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.targets.composite_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.targets.water_depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.targets.water_inputs, &[]);
            self.water.encode(&mut pass, detailed || secondary_detailed, 1 + u32::from(secondary_water.is_some()));
            if water.waterfall.is_some() {
                self.waterfall.encode(&mut pass);
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tone-map-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
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
            pass.set_pipeline(&self.post);
            pass.set_bind_group(0, &self.targets.post_inputs, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
    }

    /// Number of unique mesh assets currently resident, useful for resource lifetime checks.
    pub fn resident_meshes(&self) -> usize {
        self.meshes.resident_meshes()
    }
}
