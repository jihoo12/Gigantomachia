//! A fixed-size directional shadow map, snapped to light-space texels around the camera.

use super::{DEPTH_FORMAT, Gpu};
use glam::{Mat4, Vec3};

pub(super) const SHADOW_SIZE: u32 = 2048;
const HALF_EXTENT: f32 = 100.0;

pub(super) struct ShadowMap {
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl ShadowMap {
    pub fn new(gpu: &Gpu) -> Self {
        let view = gpu
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("sun-shadow-depth"),
                size: wgpu::Extent3d {
                    width: SHADOW_SIZE,
                    height: SHADOW_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun-shadow-comparison"),
            compare: Some(wgpu::CompareFunction::LessEqual),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { view, sampler }
    }
}

pub(super) fn matrix(focus: Vec3, sun: Vec3) -> Mat4 {
    let up = if sun.dot(Vec3::Y).abs() > 0.98 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    // Keep the view basis independent of camera translation so XY snapping is effective.
    let view = Mat4::look_at_rh(sun * 200.0, Vec3::ZERO, up);
    let center = view.transform_point3(focus);
    let texel = 2.0 * HALF_EXTENT / SHADOW_SIZE as f32;
    let x = (center.x / texel).round() * texel;
    let y = (center.y / texel).round() * texel;
    Mat4::orthographic_rh(
        x - HALF_EXTENT,
        x + HALF_EXTENT,
        y - HALF_EXTENT,
        y + HALF_EXTENT,
        -center.z - 180.0,
        -center.z + 180.0,
    ) * view
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shadow_focus_is_in_range_even_for_vertical_sun() {
        for sun in [Vec3::Y, Vec3::new(-0.36, 0.27, -0.893).normalize()] {
            let focus = Vec3::new(20.0, 3.0, -40.0);
            let projected = matrix(focus, sun).project_point3(focus);
            assert!(projected.is_finite());
            assert!(projected.x.abs() < 0.001 && projected.y.abs() < 0.001);
            assert!((projected.z - 0.5).abs() < 0.001);
        }
    }
}
