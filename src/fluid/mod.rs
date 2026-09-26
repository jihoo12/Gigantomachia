//! Position-based fluid simulation: CPU reference and GPU-resident compute backends.
mod gpu;
mod surface;
use crate::render::EngineResult;
use glam::{IVec3, Vec3};
pub use gpu::{GpuFluid, GpuFluidSnapshot};
use std::collections::HashMap;
pub use surface::FluidSurface;

pub const FIXED_DT: f32 = 1.0 / 120.0;

#[derive(Clone, Copy, Debug)]
pub struct BoxCollider {
    min: Vec3,
    max: Vec3,
}
impl BoxCollider {
    pub fn new(min: Vec3, max: Vec3) -> EngineResult<Self> {
        if !min.is_finite() || !max.is_finite() || (max - min).min_element() <= 0.0 {
            return Err("collider bounds must be finite with min < max on all axes".into());
        }
        Ok(Self { min, max })
    }
    pub fn min(self) -> Vec3 {
        self.min
    }
    pub fn max(self) -> Vec3 {
        self.max
    }
    fn project(self, previous: Vec3, mut position: Vec3, radius: f32) -> Vec3 {
        let min = self.min - Vec3::splat(radius);
        let max = self.max + Vec3::splat(radius);
        // Swept point against a radius-expanded box prevents crossing thin walls.
        let delta = position - previous;
        let mut enter = 0.0_f32;
        let mut exit = 1.0_f32;
        let mut face = None;
        for axis in 0..3 {
            if delta[axis].abs() < 1e-8 {
                if previous[axis] < min[axis] || previous[axis] > max[axis] {
                    return position;
                }
            } else {
                let a = (min[axis] - previous[axis]) / delta[axis];
                let b = (max[axis] - previous[axis]) / delta[axis];
                let near = a.min(b);
                if near >= enter {
                    enter = near;
                    face = Some((
                        axis,
                        if delta[axis] > 0.0 {
                            min[axis] - 1e-5
                        } else {
                            max[axis] + 1e-5
                        },
                    ));
                }
                exit = exit.min(a.max(b));
                if enter > exit {
                    return position;
                }
            }
        }
        if let Some((axis, boundary)) = face {
            position[axis] = boundary;
        } else if position.cmpge(min).all() && position.cmple(max).all() {
            let mut best = (f32::INFINITY, 0, 0.0);
            for axis in 0..3 {
                for boundary in [min[axis] - 1e-5, max[axis] + 1e-5] {
                    let distance = (position[axis] - boundary).abs();
                    if distance < best.0 {
                        best = (distance, axis, boundary);
                    }
                }
            }
            position[best.1] = best.2;
        }
        position
    }
}

#[derive(Clone, Debug)]
pub struct Fluid {
    positions: Vec<Vec3>,
    velocities: Vec<Vec3>,
    spacing: f32,
    rest_density: f32,
    time: f64,
}
impl Fluid {
    /// Create a finite block at rest, with equal-mass particles. No emitters or automatic recycling.
    pub fn block(min: Vec3, counts: [u32; 3], spacing: f32) -> EngineResult<Self> {
        let count = counts
            .iter()
            .try_fold(1usize, |n, &v| n.checked_mul(v as usize))
            .unwrap_or(usize::MAX);
        if !min.is_finite() || !(0.03..=1.0).contains(&spacing) || count == 0 || count > 8192 {
            return Err(
                "fluid block needs finite coordinates, spacing 0.03..=1 m, and 1..=8192 particles"
                    .into(),
            );
        }
        let mut positions = Vec::with_capacity(count);
        for z in 0..counts[2] {
            for y in 0..counts[1] {
                for x in 0..counts[0] {
                    positions.push(min + Vec3::new(x as f32, y as f32, z as f32) * spacing);
                }
            }
        }
        if positions
            .iter()
            .any(|p| !p.is_finite() || p.abs().max_element() > 10000.0)
        {
            return Err("fluid block must lie within 10 km of the origin".into());
        }
        // Calibrate rest density to this kernel's interior cubic lattice, avoiding initial compression.
        let mut rest_density = 0.0;
        for z in -2..=2 {
            for y in -2..=2 {
                for x in -2..=2 {
                    rest_density +=
                        poly6(Vec3::new(x as f32, y as f32, z as f32).length_squared() / 4.0);
                }
            }
        }
        Ok(Self {
            velocities: vec![Vec3::ZERO; count],
            positions,
            spacing,
            rest_density,
            time: 0.0,
        })
    }
    pub fn positions(&self) -> &[Vec3] {
        &self.positions
    }
    pub fn velocities(&self) -> &[Vec3] {
        &self.velocities
    }
    pub fn particle_spacing(&self) -> f32 {
        self.spacing
    }
    pub fn particle_count(&self) -> usize {
        self.positions.len()
    }
    pub fn time(&self) -> f64 {
        self.time
    }
    /// Nominal particle volume is constant; reconstructed surface volume is approximate.
    pub fn volume(&self) -> f32 {
        self.positions.len() as f32 * self.spacing.powi(3)
    }
    /// Exactly one 1/120 s step. Game code controls the accumulator and collision geometry.
    pub fn step(&mut self, colliders: &[BoxCollider]) {
        let h = self.spacing * 2.0;
        let radius = self.spacing * 0.42;
        let maximum_speed = self.spacing * 0.45 / FIXED_DT;
        let old = &self.positions;
        let mut predicted = old
            .iter()
            .zip(&self.velocities)
            .map(|(&p, &v)| {
                let velocity =
                    (v + Vec3::new(0.0, -9.81, 0.0) * FIXED_DT).clamp_length_max(maximum_speed);
                let mut next = p + velocity * FIXED_DT;
                for &c in colliders {
                    next = c.project(p, next, radius);
                }
                next
            })
            .collect::<Vec<_>>();
        let count = old.len();
        let mut lambdas = vec![0.0; count];
        let mut corrections = vec![Vec3::ZERO; count];
        for _ in 0..5 {
            let grid = neighbors(&predicted, h);
            for i in 0..count {
                let mut density = 0.0;
                let mut gradient = Vec3::ZERO;
                let mut sum = 0.0;
                visit(&grid, predicted[i], h, |j| {
                    let d = predicted[i] - predicted[j];
                    density += poly6(d.length_squared() / (h * h));
                    if i != j {
                        let g = gradient_kernel(d, h) / self.rest_density;
                        gradient += g;
                        sum += g.length_squared();
                    }
                });
                // Compression-only constraints avoid artificial attraction at free surfaces.
                let constraint = (density / self.rest_density - 1.0).max(0.0);
                lambdas[i] = -constraint / (sum + gradient.length_squared() + 0.01 / (h * h));
            }
            for i in 0..count {
                let mut correction = Vec3::ZERO;
                visit(&grid, predicted[i], h, |j| {
                    if i != j {
                        correction += (lambdas[i] + lambdas[j])
                            * gradient_kernel(predicted[i] - predicted[j], h)
                            / self.rest_density;
                    }
                });
                corrections[i] = correction.clamp_length_max(self.spacing * 0.2);
            }
            for i in 0..count {
                predicted[i] += corrections[i];
                for &c in colliders {
                    predicted[i] = c.project(old[i], predicted[i], radius);
                }
            }
        }
        let velocities = predicted
            .iter()
            .zip(old)
            .map(|(&p, &o)| (p - o) / FIXED_DT)
            .collect::<Vec<_>>();
        let grid = neighbors(&predicted, h);
        for i in 0..count {
            let mut average = Vec3::ZERO;
            let mut weight = 0.0;
            visit(&grid, predicted[i], h, |j| {
                let w = poly6((predicted[i] - predicted[j]).length_squared() / (h * h));
                average += velocities[j] * w;
                weight += w;
            });
            self.velocities[i] = velocities[i]
                .lerp(average / weight.max(1e-6), 0.04)
                .clamp_length_max(maximum_speed);
        }
        self.positions = predicted;
        self.time += f64::from(FIXED_DT);
    }
}

fn poly6(q2: f32) -> f32 {
    (1.0 - q2).max(0.0).powi(3)
}
fn gradient_kernel(delta: Vec3, h: f32) -> Vec3 {
    let q2 = delta.length_squared() / (h * h);
    if q2 >= 1.0 {
        Vec3::ZERO
    } else {
        delta * (-6.0 * (1.0 - q2).powi(2) / (h * h))
    }
}
fn neighbors(positions: &[Vec3], h: f32) -> HashMap<IVec3, Vec<usize>> {
    let mut grid = HashMap::<IVec3, Vec<usize>>::new();
    for (i, &p) in positions.iter().enumerate() {
        grid.entry((p / h).floor().as_ivec3()).or_default().push(i);
    }
    grid
}
fn visit(grid: &HashMap<IVec3, Vec<usize>>, p: Vec3, h: f32, mut f: impl FnMut(usize)) {
    let cell = (p / h).floor().as_ivec3();
    for z in -1..=1 {
        for y in -1..=1 {
            for x in -1..=1 {
                if let Some(indices) = grid.get(&(cell + IVec3::new(x, y, z))) {
                    for &i in indices {
                        f(i);
                    }
                }
            }
        }
    }
}
