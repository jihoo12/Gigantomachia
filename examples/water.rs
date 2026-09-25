//! Interactive water demo, with deterministic offscreen capture for GPU validation.

use std::{fs::File, io::BufWriter, path::Path, sync::mpsc};

use gigantomachia::{
    app,
    camera::Camera,
    render::{EngineResult, Gpu},
    water::{WaterRenderer, WaterSettings},
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

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

fn target(gpu: &Gpu, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("water-snapshot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn snapshot(path: &Path, time: f32) -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let (width, height) = (1280, 720);
    let renderer = WaterRenderer::new(&gpu, FORMAT, width, height);
    let texture = target(&gpu, width, height);
    renderer.draw(
        &gpu,
        &texture.create_view(&Default::default()),
        &Camera::default(),
        &WaterSettings::default(),
        time,
    );
    let pixels = read_frame(&gpu, &texture, width, height)?;
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(&pixels)?;
    println!(
        "Saved {} at t={time:.2}s using {}",
        path.display(),
        gpu.adapter_info.name
    );
    Ok(())
}

fn main() -> EngineResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => app::run(None),
        [flag] if flag == "--help" => {
            println!(
                "Water demo\n  cargo run --example water\n  cargo run --example water -- --frames 10\n  cargo run --example water -- --headless output.png [seconds]\n\nWASD: move | Q/E: down/up | Shift: faster | RMB drag: look\nSpace: pause | -/+: amplitude | [/]: speed | R: reset | Esc: exit"
            );
            Ok(())
        }
        [flag, count] if flag == "--frames" => app::run(Some(count.parse()?)),
        [flag, path] if flag == "--headless" => snapshot(Path::new(path), 1.25),
        [flag, path, time] if flag == "--headless" => {
            let time: f32 = time.parse()?;
            if !time.is_finite() || time < 0.0 {
                return Err("snapshot time must be finite and nonnegative".into());
            }
            snapshot(Path::new(path), time)
        }
        _ => Err("invalid arguments; use --help for usage".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a Vulkan adapter; run inside nix develop"]
    fn water_renders_animates_and_resizes_without_validation_errors() -> EngineResult<()> {
        let gpu = pollster::block_on(Gpu::headless())?;
        gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = WaterRenderer::new(&gpu, FORMAT, 320, 180);
        let camera = Camera::default();
        let settings = WaterSettings::default();
        let texture = target(&gpu, 320, 180);
        let view = texture.create_view(&Default::default());
        renderer.draw(&gpu, &view, &camera, &settings, 0.0);
        let first = read_frame(&gpu, &texture, 320, 180)?;
        renderer.draw(&gpu, &view, &camera, &settings, 1.25);
        let second = read_frame(&gpu, &texture, 320, 180)?;
        let changed = first
            .as_chunks::<4>()
            .0
            .iter()
            .zip(second.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 320 * 180 / 10, "waves should visibly animate");
        assert!(
            second
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255)
        );
        assert!(
            second
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[2] > pixel[0].saturating_add(20)),
            "water/sky should have a blue component"
        );

        let flat = WaterSettings {
            amplitude: 0.0,
            ..settings
        };
        renderer.draw(&gpu, &view, &camera, &flat, 0.0);
        let still_a = read_frame(&gpu, &texture, 320, 180)?;
        renderer.draw(&gpu, &view, &camera, &flat, 2.0);
        assert_eq!(
            still_a,
            read_frame(&gpu, &texture, 320, 180)?,
            "zero amplitude must disable waves and ripples"
        );
        assert_ne!(first, still_a, "displacement control must affect the image");

        // Non-aligned row width exercises readback padding; a portrait target exercises aspect/resize.
        renderer.resize(&gpu, 0, 0);
        renderer.resize(&gpu, 173, 257);
        let resized = target(&gpu, 173, 257);
        renderer.draw(
            &gpu,
            &resized.create_view(&Default::default()),
            &camera,
            &settings,
            0.5,
        );
        assert_eq!(read_frame(&gpu, &resized, 173, 257)?.len(), 173 * 257 * 4);
        if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
            return Err(error.into());
        }
        Ok(())
    }
}
