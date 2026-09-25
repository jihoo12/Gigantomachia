//! Shared example controls and PNG/CLI conveniences, deliberately outside the engine.

use gigantomachia::{
    app::{self, AppAction, AppConfig, Application},
    asset::AnimatedFbx,
    input::{Input, KeyCode as Key, MouseButton},
    render::{EngineResult, Gpu, OffscreenTarget, Renderer},
    scene::Scene,
    terrain::Island,
    water::WaterStyle,
};
use glam::{Mat4, Vec2, Vec3};
use std::{fs::File, io::BufWriter, path::Path};

pub struct Demo {
    pub scene: Scene,
    initial: Scene,
    name: &'static str,
    ground: Option<Island>,
    speed: f32,
    paused: bool,
    animation: Option<(AnimatedFbx, Mat4)>,
    elapsed: f64,
}

impl Demo {
    pub fn new(name: &'static str, scene: Scene, ground: Option<Island>) -> Self {
        Self {
            initial: scene.clone(),
            scene,
            name,
            ground,
            speed: 1.0,
            paused: false,
            animation: None,
            elapsed: 0.0,
        }
    }
    #[allow(dead_code)] // Only the FBX viewer attaches an animation.
    pub fn with_animation(mut self, asset: AnimatedFbx, placement: Mat4) -> EngineResult<Self> {
        self.animation = Some((asset, placement));
        self.sample_animation(0.0)?;
        self.initial = self.scene.clone();
        Ok(self)
    }

    fn sample_animation(&mut self, seconds: f64) -> EngineResult<()> {
        if let Some((asset, placement)) = &self.animation {
            let instances = asset.sample(0, seconds, true, *placement)?;
            for (destination, instance) in self.scene.meshes.iter_mut().zip(instances) {
                *destination = instance;
            }
        }
        self.elapsed = seconds;
        Ok(())
    }
}

impl Application for Demo {
    fn scene(&self) -> &Scene {
        &self.scene
    }

    fn update(&mut self, input: &Input, dt: f32) -> AppAction {
        if input.pressed(Key::Escape) {
            return AppAction::Exit;
        }
        if input.pressed(Key::KeyR) {
            self.scene = self.initial.clone();
            self.elapsed = 0.0;
            self.speed = 1.0;
            self.paused = false;
            return AppAction::Continue;
        }
        if input.mouse_held(MouseButton::Right) {
            let delta = input.mouse_delta();
            self.scene.camera.look(delta.x, delta.y);
        }
        let pressed = |key| f32::from(input.held(key));
        self.scene.camera.travel(
            Vec3::new(
                pressed(Key::KeyD) - pressed(Key::KeyA),
                pressed(Key::KeyE) - pressed(Key::KeyQ),
                pressed(Key::KeyW) - pressed(Key::KeyS),
            ),
            dt,
            input.held(Key::ShiftLeft) || input.held(Key::ShiftRight),
        );
        // Example-specific clearance, not a physics/collision system.
        if self.scene.water.is_some_and(|water| water.bounds.is_none()) || self.ground.is_some() {
            let position = &mut self.scene.camera.position;
            let floor = self
                .ground
                .map_or(0.0, |island| {
                    island.height_at(Vec2::new(position.x, position.z))
                })
                .max(0.0);
            position.y = position.y.clamp(floor + 3.5, 80.0_f32.max(floor + 3.5));
        }
        if input.pressed(Key::Digit1) {
            self.scene.sun.shadows = !self.scene.sun.shadows;
        }
        if input.pressed(Key::Space) {
            self.paused = !self.paused;
        }
        if input.pressed(Key::BracketRight) {
            self.speed = (self.speed + 0.1).min(3.0);
        }
        if input.pressed(Key::BracketLeft) {
            self.speed = (self.speed - 0.1).max(0.0);
        }
        if let Some(water) = &mut self.scene.water {
            if input.pressed(Key::Digit4) {
                water.style = if water.style == WaterStyle::Realistic {
                    WaterStyle::Stylized
                } else {
                    WaterStyle::Realistic
                };
            }
            if input.pressed(Key::Digit2) {
                water.refraction = !water.refraction;
            }
            if input.pressed(Key::Digit3) {
                water.foam_strength = if water.foam_strength > 0.0 { 0.0 } else { 1.0 };
            }
            if input.pressed(Key::Equal) || input.pressed(Key::NumpadAdd) {
                water.amplitude = (water.amplitude + 0.1).min(2.0);
            }
            if input.pressed(Key::Minus) || input.pressed(Key::NumpadSubtract) {
                water.amplitude = (water.amplitude - 0.1).max(0.0);
            }
            if !self.paused {
                water.time += dt * self.speed;
            }
        }
        if !self.paused
            && let Err(error) = self.sample_animation(self.elapsed + f64::from(dt * self.speed))
        {
            eprintln!("Animation playback failed: {error}");
            return AppAction::Exit;
        }
        AppAction::Continue
    }

    fn title(&self) -> String {
        if let Some((asset, _)) = &self.animation {
            return format!(
                "Gigantomachia | {} | {} | {:.2}s | speed {:.1}{} | Space: pause | [/]: speed | R: reset",
                self.name,
                asset.clips()[0].name,
                self.elapsed,
                self.speed,
                if self.paused { " | PAUSED" } else { "" }
            );
        }
        format!(
            "Gigantomachia | {} | amplitude {:.1} | speed {:.1}{} | shadow {} · refraction {} · foam {} | water {}",
            self.name,
            self.scene.water.map_or(0.0, |water| water.amplitude),
            self.speed,
            if self.paused { " | PAUSED" } else { "" },
            if self.scene.sun.shadows { "on" } else { "off" },
            if self.scene.water.is_some_and(|w| w.refraction) {
                "on"
            } else {
                "off"
            },
            if self.scene.water.is_some_and(|w| w.foam_strength > 0.0) {
                "on"
            } else {
                "off"
            },
            self.scene.water.map_or("none", |water| match water.style {
                WaterStyle::Stylized => "stylized",
                WaterStyle::Realistic => "realistic",
            })
        )
    }
}

fn snapshot(scene: &Scene, path: &Path) -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let (width, height) = (1280, 720);
    let target = OffscreenTarget::new(&gpu, width, height)?;
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, width, height)?;
    renderer.render(&gpu, &target.view(), scene);
    let pixels = target.read_rgba8(&gpu)?;
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&pixels)?;
    writer.finish()?;
    println!("Saved {} using {}", path.display(), gpu.adapter_info.name);
    Ok(())
}

#[allow(dead_code)] // The FBX viewer parses its model path before calling run_with_args.
pub fn run(demo: Demo) -> EngineResult<()> {
    run_with_args(demo, std::env::args().skip(1).collect())
}

pub fn run_with_args(mut demo: Demo, mut args: Vec<String>) -> EngineResult<()> {
    args.retain(|arg| {
        match arg.as_str() {
            "--realistic-water" => {
                if let Some(water) = &mut demo.scene.water {
                    water.style = WaterStyle::Realistic;
                }
            }
            "--no-shadows" => demo.scene.sun.shadows = false,
            "--no-refraction" => {
                if let Some(water) = &mut demo.scene.water {
                    water.refraction = false;
                }
            }
            "--no-foam" => {
                if let Some(water) = &mut demo.scene.water {
                    water.foam_strength = 0.0;
                }
            }
            _ => return true,
        }
        false
    });
    demo.initial = demo.scene.clone();
    match args.as_slice() {
        [] => app::run(demo, AppConfig::default()),
        [flag] if flag == "--help" => {
            println!(
                "{} demo\nOptions: --frames COUNT | --headless output.png [seconds]\nOptional: --realistic-water --no-shadows --no-refraction --no-foam\n\nWASD: move | Q/E: down/up | Shift: faster | RMB drag: look\nSpace: pause | -/+: amplitude | [/]: speed | R: reset | Esc: exit\n1: shadows | 2: refraction | 3: shore foam | 4: water style",
                demo.name
            );
            Ok(())
        }
        [flag, count] if flag == "--frames" => app::run(
            demo,
            AppConfig {
                frame_limit: Some(count.parse()?),
                ..Default::default()
            },
        ),
        [flag, path] if flag == "--headless" => {
            if let Some(water) = &mut demo.scene.water {
                water.time = 1.25;
            }
            demo.sample_animation(1.25)?;
            snapshot(&demo.scene, Path::new(path))
        }
        [flag, path, time] if flag == "--headless" => {
            let time: f32 = time.parse()?;
            if !time.is_finite() || time < 0.0 {
                return Err("snapshot time must be finite and nonnegative".into());
            }
            if let Some(water) = &mut demo.scene.water {
                water.time = time;
            }
            demo.sample_animation(f64::from(time))?;
            snapshot(&demo.scene, Path::new(path))
        }
        _ => Err("invalid arguments; use --help for usage".into()),
    }
}
