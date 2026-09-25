//! Size-dependent scene attachments and sampling bindings. Rebuilt together on resize.

use super::{DEPTH_FORMAT, Gpu};

pub(super) const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(super) struct SceneTargets {
    pub opaque_color: wgpu::Texture,
    pub opaque_color_view: wgpu::TextureView,
    pub opaque_depth: wgpu::Texture,
    pub opaque_depth_view: wgpu::TextureView,
    pub composite_color: wgpu::Texture,
    pub composite_view: wgpu::TextureView,
    pub water_depth: wgpu::Texture,
    pub water_depth_view: wgpu::TextureView,
    pub reflection_view: wgpu::TextureView,
    pub reflection_depth_view: wgpu::TextureView,
    pub water_inputs: wgpu::BindGroup,
    pub post_inputs: wgpu::BindGroup,
}

impl SceneTargets {
    pub fn new(
        gpu: &Gpu,
        width: u32,
        height: u32,
        water_layout: &wgpu::BindGroupLayout,
        post_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture = |label, format, usage| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        use wgpu::TextureUsages as U;
        let opaque_color = texture(
            "opaque-hdr",
            HDR_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC,
        );
        let opaque_depth = texture(
            "opaque-depth",
            DEPTH_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC,
        );
        let composite_color = texture(
            "composite-hdr",
            HDR_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_DST,
        );
        let water_depth = texture(
            "water-depth",
            DEPTH_FORMAT,
            U::RENDER_ATTACHMENT | U::COPY_DST,
        );
        let opaque_color_view = opaque_color.create_view(&Default::default());
        let opaque_depth_view = opaque_depth.create_view(&Default::default());
        let composite_view = composite_color.create_view(&Default::default());
        let water_depth_view = water_depth.create_view(&Default::default());
        let reflection_view = texture(
            "reflection-hdr",
            HDR_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        )
        .create_view(&Default::default());
        let reflection_depth_view = texture("reflection-depth", DEPTH_FORMAT, U::RENDER_ATTACHMENT)
            .create_view(&Default::default());
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("refraction-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let water_inputs = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water-inputs"),
            layout: water_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&opaque_color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&opaque_depth_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&reflection_view),
                },
            ],
        });
        let post_inputs = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tone-map-inputs"),
            layout: post_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&composite_view),
            }],
        });
        Self {
            opaque_color,
            opaque_color_view,
            opaque_depth,
            opaque_depth_view,
            composite_color,
            composite_view,
            water_depth,
            water_depth_view,
            reflection_view,
            reflection_depth_view,
            water_inputs,
            post_inputs,
        }
    }
}

pub(super) fn water_layout(gpu: &Gpu) -> wgpu::BindGroupLayout {
    gpu.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("refraction-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        })
}
