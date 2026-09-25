use gigantomachia::render::Gpu;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gpu = pollster::block_on(Gpu::headless())?;
    println!("GPU: {}", gpu.adapter_info.name);
    println!("Backend: {:?}", gpu.adapter_info.backend);
    println!("Device type: {:?}", gpu.adapter_info.device_type);
    println!("Vulkan device and queue initialized successfully.");
    Ok(())
}
