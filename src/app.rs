//! Window lifecycle and controls for the first water demo.

use std::{collections::HashSet, sync::Arc, time::Instant};

use glam::Vec3;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    camera::Camera,
    render::{EngineResult, Gpu, instance},
    water::{WaterRenderer, WaterSettings},
};

struct WindowState {
    surface: wgpu::Surface<'static>,
    window: Arc<Window>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    renderer: WaterRenderer,
    drawable: bool,
    occluded: bool,
}

impl WindowState {
    async fn new(window: Arc<Window>) -> EngineResult<Self> {
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
        surface.configure(&gpu.device, &config);
        let renderer = WaterRenderer::new(&gpu, format, config.width, config.height);
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
            drawable: size.width > 0 && size.height > 0,
            occluded: false,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.drawable = width > 0 && height > 0;
        if !self.drawable {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.gpu.device, &self.config);
        self.renderer.resize(&self.gpu, width, height);
    }

    fn draw(
        &self,
        camera: &Camera,
        settings: &WaterSettings,
        time: f32,
    ) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame.texture.create_view(&Default::default());
        self.renderer.draw(&self.gpu, &view, camera, settings, time);
        self.window.pre_present_notify();
        frame.present();
        Ok(())
    }
}

struct WaterApp {
    state: Option<WindowState>,
    camera: Camera,
    settings: WaterSettings,
    keys: HashSet<KeyCode>,
    dragging: bool,
    cursor: Option<(f64, f64)>,
    last_frame: Instant,
    wave_time: f32,
    frames_left: Option<u32>,
    error: Option<String>,
}

impl WaterApp {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        self.error = Some(error.to_string());
        event_loop.exit();
    }

    fn clear_input(&mut self) {
        self.keys.clear();
        self.dragging = false;
        self.cursor = None;
    }

    fn update_title(&self) {
        if let Some(state) = &self.state {
            state.window.set_title(&format!(
                "Gigantomachia | Water | amplitude {:.1} | speed {:.1}{} | RMB look · WASD move · Space pause",
                self.settings.amplitude, self.settings.speed,
                if self.settings.paused { " | PAUSED" } else { "" },
            ));
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        let Some(state) = self.state.as_ref() else {
            return;
        };
        if !state.drawable || state.occluded {
            return;
        }
        let pressed = |code| f32::from(self.keys.contains(&code));
        let axes = Vec3::new(
            pressed(KeyCode::KeyD) - pressed(KeyCode::KeyA),
            pressed(KeyCode::KeyE) - pressed(KeyCode::KeyQ),
            pressed(KeyCode::KeyW) - pressed(KeyCode::KeyS),
        );
        self.camera.travel(
            axes,
            dt,
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight),
        );
        if !self.settings.paused {
            self.wave_time += dt * self.settings.speed;
        }
        match state.draw(&self.camera, &self.settings, self.wave_time) {
            Ok(()) => {
                if let Some(frames) = &mut self.frames_left {
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

impl ApplicationHandler for WaterApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let result = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Gigantomachia | Water")
                    .with_inner_size(LogicalSize::new(1280.0, 720.0)),
            )
            .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
            .and_then(|window| pollster::block_on(WindowState::new(Arc::new(window))));
        match result {
            Ok(state) => {
                self.state = Some(state);
                self.update_title();
            }
            Err(error) => self.fail(event_loop, error),
        }
        self.last_frame = Instant::now();
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.state = None;
        self.clear_input();
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
                if let Some(state) = &mut self.state {
                    state.resize(size.width, size.height);
                }
                self.last_frame = Instant::now();
            }
            WindowEvent::Occluded(occluded) => {
                if let Some(state) = &mut self.state {
                    state.occluded = occluded;
                }
                self.last_frame = Instant::now();
            }
            WindowEvent::Focused(false) => self.clear_input(),
            WindowEvent::CursorLeft { .. } => {
                self.dragging = false;
                self.cursor = None;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.dragging
                    && let Some((x, y)) = self.cursor
                {
                    self.camera
                        .look((position.x - x) as f32, (position.y - y) as f32);
                }
                self.cursor = Some((position.x, position.y));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Released {
                        self.keys.remove(&code);
                        return;
                    }
                    self.keys.insert(code);
                    if event.repeat {
                        return;
                    }
                    match code {
                        KeyCode::Escape => event_loop.exit(),
                        KeyCode::Space => self.settings.paused = !self.settings.paused,
                        KeyCode::Equal | KeyCode::NumpadAdd => {
                            self.settings.amplitude = (self.settings.amplitude + 0.1).min(2.0)
                        }
                        KeyCode::Minus | KeyCode::NumpadSubtract => {
                            self.settings.amplitude = (self.settings.amplitude - 0.1).max(0.0)
                        }
                        KeyCode::BracketRight => {
                            self.settings.speed = (self.settings.speed + 0.1).min(3.0)
                        }
                        KeyCode::BracketLeft => {
                            self.settings.speed = (self.settings.speed - 0.1).max(0.0)
                        }
                        KeyCode::KeyR => {
                            self.camera = Camera::default();
                            self.settings = WaterSettings::default();
                            self.wave_time = 0.0;
                        }
                        _ => {}
                    }
                    self.update_title();
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

/// Run the water demo. A frame limit is useful for checking surface presentation.
pub fn run(frame_limit: Option<u32>) -> EngineResult<()> {
    if frame_limit == Some(0) {
        return Err("frame limit must be greater than zero".into());
    }
    let mut app = WaterApp {
        state: None,
        camera: Camera::default(),
        settings: WaterSettings::default(),
        keys: HashSet::new(),
        dragging: false,
        cursor: None,
        last_frame: Instant::now(),
        wave_time: 0.0,
        frames_left: frame_limit,
        error: None,
    };
    EventLoop::new()?.run_app(&mut app)?;
    match app.error {
        Some(error) => Err(error.into()),
        None => Ok(()),
    }
}
