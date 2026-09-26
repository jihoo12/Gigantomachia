//! Finite 3D fluid draining from a container under gravity. No ocean or scripted waterfall.
mod support;
use gigantomachia::{
    app::{self, AppAction, AppConfig, Application},
    camera::Camera,
    fluid::{BoxCollider, FIXED_DT, Fluid},
    input::{Input, KeyCode as Key},
    mesh::{Mesh, Vertex},
    render::EngineResult,
    scene::{MeshInstance, Scene, Sun},
};
use glam::Vec3;
use std::{path::Path, sync::Arc};

fn cuboid(center: Vec3, half: Vec3, color: [f32; 3]) -> EngineResult<MeshInstance> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (normal, u, v) in [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, Vec3::Y, Vec3::X),
    ] {
        let first = vertices.len() as u32;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.push(Vertex {
                position: (center + (normal + u * x + v * y) * half).to_array(),
                normal: normal.to_array(),
                color,
            });
        }
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    Ok(MeshInstance::new(Arc::new(Mesh::new(vertices, indices)?)))
}

struct FluidDemo {
    demo: support::Demo,
    fluid: Fluid,
    solids: Vec<BoxCollider>,
    gate: BoxCollider,
    gate_mesh: MeshInstance,
    gate_open: bool,
    automatic: bool,
    hold_closed: bool,
    paused: bool,
    accumulator: f32,
    speed: f32,
}
impl FluidDemo {
    fn new(hold_closed: bool) -> EngineResult<Self> {
        let bounds = [
            (Vec3::new(-2.2, -0.3, -1.6), Vec3::new(2.2, 0.0, 4.2)),
            (Vec3::new(-1.12, 1.45, -1.12), Vec3::new(1.12, 1.65, 1.12)),
            (Vec3::new(-1.12, 1.65, -1.12), Vec3::new(-1.0, 2.5, 1.12)),
            (Vec3::new(1.0, 1.65, -1.12), Vec3::new(1.12, 2.5, 1.12)),
            (Vec3::new(-1.12, 1.65, -1.12), Vec3::new(1.12, 2.5, -1.0)),
            // A receiving basin keeps the finite amount of water in the demonstration area.
            (Vec3::new(-2.2, 0.0, -1.6), Vec3::new(-2.0, 0.8, 4.2)),
            (Vec3::new(2.0, 0.0, -1.6), Vec3::new(2.2, 0.8, 4.2)),
            (Vec3::new(-2.2, 0.0, 4.0), Vec3::new(2.2, 0.8, 4.2)),
            (Vec3::new(-2.2, 0.0, -1.6), Vec3::new(2.2, 0.8, -1.4)),
            // A real collider in the runoff path splits and deflects the flow.
            (Vec3::new(-0.25, 0.0, 2.0), Vec3::new(0.25, 0.5, 2.5)),
        ];
        let mut solids = Vec::new();
        let mut meshes = Vec::new();
        for (i, (min, max)) in bounds.into_iter().enumerate() {
            solids.push(BoxCollider::new(min, max)?);
            let color = if i == 0 {
                [0.18, 0.21, 0.23]
            } else if i == 9 {
                [0.65, 0.07, 0.025]
            } else {
                [0.38, 0.20, 0.075]
            };
            meshes.push(cuboid((min + max) * 0.5, (max - min) * 0.5, color)?);
        }
        let gate = BoxCollider::new(Vec3::new(-1.12, 1.65, 1.0), Vec3::new(1.12, 2.5, 1.12))?;
        let gate_mesh = cuboid(
            (gate.min() + gate.max()) * 0.5,
            (gate.max() - gate.min()) * 0.5,
            [0.46, 0.25, 0.09],
        )?;
        meshes.push(gate_mesh.clone());
        let fluid = Fluid::block(Vec3::new(-0.84, 1.72, -0.84), [15, 6, 15], 0.12)?;
        let scene = Scene {
            camera: Camera::looking_at(Vec3::new(4.2, 6.5, 7.0), Vec3::new(0.0, 1.1, 1.0))?,
            meshes,
            fluids: vec![fluid.surface()?],
            sun: Sun {
                direction: Vec3::new(-0.6, 0.8, -0.7),
                ..Default::default()
            },
            ..Default::default()
        };
        Ok(Self {
            demo: support::Demo::new("3D fluid", scene, None),
            fluid,
            solids,
            gate,
            gate_mesh,
            gate_open: false,
            automatic: !hold_closed,
            hold_closed,
            paused: false,
            accumulator: 0.0,
            speed: 1.0,
        })
    }
    fn set_gate(&mut self, open: bool) {
        if open == self.gate_open {
            return;
        }
        if open {
            self.demo.scene.meshes.pop();
        } else {
            self.demo.scene.meshes.push(self.gate_mesh.clone());
        }
        self.gate_open = open;
    }
    fn step(&mut self) {
        if self.automatic && self.fluid.time() >= 1.0 {
            self.set_gate(true);
            self.automatic = false;
        }
        if self.gate_open {
            self.fluid.step(&self.solids);
        } else {
            self.solids.push(self.gate);
            self.fluid.step(&self.solids);
            self.solids.pop();
        }
    }
    fn refresh(&mut self) -> EngineResult<()> {
        self.demo.scene.fluids = vec![self.fluid.surface()?];
        Ok(())
    }
}
impl Application for FluidDemo {
    fn scene(&self) -> &Scene {
        &self.demo.scene
    }
    fn update(&mut self, input: &Input, dt: f32) -> AppAction {
        if input.pressed(Key::KeyR) {
            match Self::new(self.hold_closed) {
                Ok(reset) => *self = reset,
                Err(e) => {
                    eprintln!("{e}");
                    return AppAction::Exit;
                }
            }
            return AppAction::Continue;
        }
        if matches!(self.demo.update(input, dt), AppAction::Exit) {
            return AppAction::Exit;
        }
        if input.pressed(Key::Space) {
            self.paused = !self.paused;
        }
        if input.pressed(Key::KeyO) {
            self.automatic = false;
            self.set_gate(!self.gate_open);
        }
        if input.pressed(Key::BracketLeft) {
            self.speed = (self.speed - 0.25).max(0.25);
        }
        if input.pressed(Key::BracketRight) {
            self.speed = (self.speed + 0.25).min(2.0);
        }
        if !self.paused {
            // Limit catch-up work: overload slows simulated time instead of enlarging the physics step.
            self.accumulator = (self.accumulator + dt * self.speed).min(FIXED_DT * 8.0);
            let mut stepped = false;
            while self.accumulator >= FIXED_DT {
                self.step();
                self.accumulator -= FIXED_DT;
                stepped = true;
            }
            if stepped && let Err(error) = self.refresh() {
                eprintln!("Fluid surface: {error}");
                return AppAction::Exit;
            }
        }
        AppAction::Continue
    }
    fn title(&self) -> String {
        format!(
            "3D Fluid | {} particles | {:.2}s | gate {} | {} | O: gate · Space: pause · R: reset",
            self.fluid.particle_count(),
            self.fluid.time(),
            if self.gate_open { "open" } else { "closed" },
            if self.paused { "PAUSED" } else { "running" }
        )
    }
}
fn main() -> EngineResult<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let closed = args.iter().any(|a| a == "--closed");
    args.retain(|a| a != "--closed");
    if args.iter().any(|a| a == "--help") {
        println!(
            "3D fluid board: [--closed] [--headless output.png seconds | --frames count]\nThe gate opens after one simulated second unless --closed is set.\nO: open/close gate | Space: pause | R: reset | [/]: speed | WASD/QE + RMB: camera"
        );
        return Ok(());
    }
    let mut demo = FluidDemo::new(closed)?;
    match args.as_slice() {
        [] => app::run(demo, AppConfig::default()),
        [flag, count] if flag == "--frames" => app::run(
            demo,
            AppConfig {
                frame_limit: Some(count.parse()?),
                ..Default::default()
            },
        ),
        [flag, path, seconds] if flag == "--headless" => {
            let seconds: f64 = seconds.parse()?;
            if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
                return Err("capture time must be in 0..=60 seconds".into());
            }
            for _ in 0..(seconds / f64::from(FIXED_DT)).round() as usize {
                demo.step();
            }
            demo.refresh()?;
            println!(
                "Simulated {:.3}s, {} particles, nominal volume {:.3} m3, {} particles below the board",
                demo.fluid.time(),
                demo.fluid.particle_count(),
                demo.fluid.volume(),
                demo.fluid.positions().iter().filter(|p| p.y < 1.4).count()
            );
            support::snapshot(&demo.demo.scene, Path::new(path))
        }
        _ => Err("invalid arguments; use --help".into()),
    }
}
