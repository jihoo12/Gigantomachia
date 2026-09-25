//! Vulkan device initialization shared by windowed and headless rendering.

pub type EngineResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    })
}

/// GPU resources shared by the renderer.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl Gpu {
    /// Initialize Vulkan without a window, for development-environment checks.
    /// The windowed renderer must request an adapter compatible with its surface.
    pub async fn headless() -> EngineResult<Self> {
        let (gpu, _) = Self::request(&instance(), None).await?;
        Ok(gpu)
    }

    async fn request(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> EngineResult<(Self, wgpu::Adapter)> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface,
                force_fallback_adapter: false,
            })
            .await?;
        let adapter_info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gigantomachia-device"),
                ..Default::default()
            })
            .await?;
        Ok((
            Self {
                device,
                queue,
                adapter_info,
            },
            adapter,
        ))
    }

    pub(crate) async fn with_surface(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'_>,
    ) -> EngineResult<(Self, wgpu::SurfaceCapabilities)> {
        let (gpu, adapter) = Self::request(instance, Some(surface)).await?;
        let capabilities = surface.get_capabilities(&adapter);
        Ok((gpu, capabilities))
    }
}
