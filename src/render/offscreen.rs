//! Offscreen RGBA target and GPU readback, with no image-file dependency.

use super::{EngineResult, Gpu, validate_extent};
use std::sync::mpsc;

pub struct OffscreenTarget {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
}

impl OffscreenTarget {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> EngineResult<Self> {
        validate_extent(gpu, width, height)?;
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen-color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Ok(Self {
            texture,
            width,
            height,
        })
    }
    pub fn view(&self) -> wgpu::TextureView {
        self.texture.create_view(&Default::default())
    }
    pub fn read_rgba8(&self, gpu: &Gpu) -> EngineResult<Vec<u8>> {
        read_frame(gpu, &self.texture, self.width, self.height)
    }
}

fn read_frame(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> EngineResult<Vec<u8>> {
    let row_bytes = width * 4;
    let padded_row =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("water-snapshot-readback"),
        size: u64::from(padded_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
    receiver.recv()??;
    let mapped = buffer.slice(..).get_mapped_range();
    let mut pixels = Vec::with_capacity((row_bytes * height) as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        pixels.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}
