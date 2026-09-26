//! Small procedural spill pass. Geometry and animation are evaluated on the GPU.
use super::{DEPTH_FORMAT, Gpu, targets::HDR_FORMAT};

pub(super) struct WaterfallPass {
    pipeline: wgpu::RenderPipeline,
}
impl WaterfallPass {
    pub fn new(
        gpu: &Gpu,
        layout: &wgpu::BindGroupLayout,
        inputs: &wgpu::BindGroupLayout,
        fluid: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("waterfall-shader"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}
{}",
                        include_str!("../shaders/common.wgsl"),
                        include_str!("../shaders/waterfall.wgsl")
                    )
                    .into(),
                ),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("waterfall-layout"),
                bind_group_layouts: &[layout, inputs, fluid],
                push_constant_ranges: &[],
            });
        Self {
            pipeline: gpu
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("waterfall-pipeline"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: HDR_FORMAT,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::LessEqual,
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    multiview: None,
                    cache: None,
                }),
        }
    }
    pub fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        // The receiving-water disk is deliberately gone: impact response now belongs to
        // the fluid domain. Draw only the continuous free-fall surface and spray.
        pass.draw((96 * 3)..(96 * 3 + 32 * 32 * 6 + 384 * 6), 0..1);
    }
}
