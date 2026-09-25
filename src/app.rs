//! Generic application host. Demo-specific controls and scene construction live in examples.

use crate::{
    input::Input,
    render::{EngineResult, Gpu, Renderer, instance},
    scene::Scene,
};
use glam::Vec2;
use std::{sync::Arc, time::Instant};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

pub enum AppAction {
    Continue,
    Exit,
}

/// Game code supplies CPU-side scene data and behavior; it never manages a surface or render pass.
pub trait Application {
    fn scene(&self) -> &Scene;
    fn update(&mut self, input: &Input, dt: f32) -> AppAction;
    fn title(&self) -> String {
        "Gigantomachia".into()
    }
}

pub struct AppConfig {
    pub width: u32,
    pub height: u32,
    /// Exit after this many successful presentations, for smoke tests.
    pub frame_limit: Option<u32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            frame_limit: None,
        }
    }
}

struct WindowState {
    surface: wgpu::Surface<'static>,
    window: Arc<Window>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    drawable: bool,
    occluded: bool,
    title: String,
}

impl WindowState {
    async fn new(window: Arc<Window>, title: String) -> EngineResult<Self> {
        let instance = instance();
        let surface = instance.create_surface(window.clone())?;
        let (gpu, capabilities) = Gpu::with_surface(&instance, &surface).await?;
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .ok_or("the surface does not support an sRGB format")?;
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        let renderer = Renderer::new(&gpu, format, config.width, config.height)?;
        surface.configure(&gpu.device, &config);
        eprintln!(
            "GPU: {} ({:?})",
            gpu.adapter_info.name, gpu.adapter_info.backend
        );
        Ok(Self {
            surface,
            window,
            gpu,
            config,
            renderer,
            title,
            drawable: size.width > 0 && size.height > 0,
            occluded: false,
        })
    }

    fn resize(&mut self, width: u32, height: u32) -> EngineResult<()> {
        self.drawable = width > 0 && height > 0;
        if !self.drawable {
            return Ok(());
        }
        self.renderer.resize(&self.gpu, width, height)?;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.gpu.device, &self.config);
        Ok(())
    }

    fn draw(&mut self, scene: &Scene) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        self.renderer.render(
            &self.gpu,
            &frame.texture.create_view(&Default::default()),
            scene,
        );
        self.window.pre_present_notify();
        frame.present();
        Ok(())
    }
}

struct Host<A> {
    application: A,
    options: AppConfig,
    state: Option<WindowState>,
    input: Input,
    last_frame: Instant,
    error: Option<String>,
}

impl<A: Application> Host<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if !state.drawable || state.occluded {
            return;
        }
        let action = self.application.update(&self.input, dt);
        self.input.end_frame();
        if matches!(action, AppAction::Exit) {
            event_loop.exit();
            return;
        }
        let title = self.application.title();
        if title != state.title {
            state.window.set_title(&title);
            state.title = title;
        }
        match state.draw(self.application.scene()) {
            Ok(()) => {
                if let Some(frames) = &mut self.options.frame_limit {
                    *frames -= 1;
                    if *frames == 0 {
                        event_loop.exit();
                    }
                }
            }
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state.surface.configure(&state.gpu.device, &state.config);
            }
            Err(wgpu::SurfaceError::Timeout) => {}
            Err(error) => self.fail(event_loop, error),
        }
    }
}

impl<A: Application> ApplicationHandler for Host<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let title = self.application.title();
        let result = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(&title)
                    .with_inner_size(LogicalSize::new(self.options.width, self.options.height)),
            )
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
            .and_then(|window| pollster::block_on(WindowState::new(Arc::new(window), title)));
        match result {
            Ok(state) => self.state = Some(state),
            Err(error) => self.fail(event_loop, error),
        }
        self.last_frame = Instant::now();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.state = None;
        self.input.clear();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self
            .state
            .as_ref()
            .is_none_or(|state| state.window.id() != id)
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state
                    && let Err(error) = state.resize(size.width, size.height)
                {
                    self.fail(event_loop, error);
                }
                self.last_frame = Instant::now();
            }
            WindowEvent::Occluded(occluded) => {
                if let Some(state) = &mut self.state {
                    state.occluded = occluded;
                }
                self.last_frame = Instant::now();
            }
            WindowEvent::Focused(false) => self.input.clear(),
            WindowEvent::CursorLeft { .. } => self.input.cursor_left(),
            WindowEvent::MouseInput { state, button, .. } => self
                .input
                .mouse_button(button, state == ElementState::Pressed),
            WindowEvent::CursorMoved { position, .. } => self
                .input
                .cursor(Vec2::new(position.x as f32, position.y as f32)),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.input.key(code, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state
            && state.drawable
            && !state.occluded
        {
            state.window.request_redraw();
        }
    }
}

pub fn run(application: impl Application, options: AppConfig) -> EngineResult<()> {
    if options.width == 0 || options.height == 0 || options.frame_limit == Some(0) {
        return Err("window dimensions and frame limit must be greater than zero".into());
    }
    let mut host = Host {
        application,
        options,
        state: None,
        input: Input::default(),
        last_frame: Instant::now(),
        error: None,
    };
    EventLoop::new()?.run_app(&mut host)?;
    match host.error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}
