//! Deterministic island heightfield generation; the renderer only sees a regular Mesh.

use crate::{
    mesh::{Mesh, Vertex, grid_indices},
    render::EngineResult,
};
use glam::{Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Island {
    pub center: Vec2,
    pub radius: f32,
    pub peak_height: f32,
    pub cells: u32,
    pub seed: u32,
}

impl Default for Island {
    fn default() -> Self {
        Self {
            center: Vec2::new(0.0, -12.0),
            radius: 30.0,
            peak_height: 18.0,
            cells: 160,
            seed: 7,
        }
    }
}

fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let t = ((value - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn noise(point: Vec2, seed: u32) -> f32 {
    let cell = point.floor();
    let f = point - cell;
    let t = f * f * (Vec2::splat(3.0) - 2.0 * f);
    let hash = |x: i32, z: i32| {
        let mut n = (x as u32).wrapping_mul(374761393)
            ^ (z as u32).wrapping_mul(668265263)
            ^ seed.wrapping_mul(1442695041);
        n = (n ^ (n >> 13)).wrapping_mul(1274126177);
        (n ^ (n >> 16)) as f32 / u32::MAX as f32
    };
    let x = cell.x as i32;
    let z = cell.y as i32;
    let a = hash(x, z) * (1.0 - t.x) + hash(x + 1, z) * t.x;
    let b = hash(x, z + 1) * (1.0 - t.x) + hash(x + 1, z + 1) * t.x;
    a * (1.0 - t.y) + b * t.y
}

impl Island {
    pub fn validate(&self) -> EngineResult<()> {
        if !self.center.is_finite()
            || !self.radius.is_finite()
            || !(2.0..=1000.0).contains(&self.radius)
            || !self.peak_height.is_finite()
            || !(1.0..=200.0).contains(&self.peak_height)
            || !(8..=512).contains(&self.cells)
        {
            return Err("island requires a finite center, radius 2..1000, peak height 1..200, and 8..512 cells".into());
        }
        Ok(())
    }

    /// Sample the analytic heightfield after validating the configuration.
    pub fn height_at(&self, position: Vec2) -> f32 {
        let p = (position - self.center) / self.radius;
        let theta = p.y.atan2(p.x);
        let phase = self.seed as f32 * 0.37;
        let shoreline =
            1.0 + 0.09 * (3.0 * theta + phase).sin() + 0.06 * (5.0 * theta - phase).cos();
        let radius = (p * Vec2::new(1.0, 1.12)).length() / shoreline;
        // Deepen the offshore shelf before the finite mesh edge, so transmission fades naturally.
        let beach =
            -4.0 + 6.0 * (1.0 - smooth(0.77, 1.10, radius)) - 65.0 * smooth(1.02, 1.55, radius);
        let interior = (1.0 - smooth(0.02, 0.82, radius)).powf(1.3);
        let broad = noise(p * 4.0, self.seed);
        let detail = noise(p * 13.0, self.seed.wrapping_add(1));
        let ridge = 0.62 + broad * 0.65 + (detail - 0.5) * 0.22;
        beach + self.peak_height * interior * ridge
    }

    pub fn mesh(&self) -> EngineResult<Mesh> {
        self.validate()?;
        let extent = self.radius * 3.6;
        let step = extent / self.cells as f32;
        let epsilon = step * 0.5;
        let mut vertices = Vec::with_capacity(((self.cells + 1).pow(2)) as usize);
        for z in 0..=self.cells {
            for x in 0..=self.cells {
                let p = self.center
                    + Vec2::new(
                        x as f32 * step - extent * 0.5,
                        z as f32 * step - extent * 0.5,
                    );
                let height = self.height_at(p);
                let dx =
                    self.height_at(p + Vec2::X * epsilon) - self.height_at(p - Vec2::X * epsilon);
                let dz =
                    self.height_at(p + Vec2::Y * epsilon) - self.height_at(p - Vec2::Y * epsilon);
                let normal = Vec3::new(-dx, 2.0 * epsilon, -dz).normalize();
                let sand = Vec3::new(0.66, 0.49, 0.27) * (0.65 + 0.35 * smooth(0.0, 1.8, height));
                let grass = Vec3::new(0.11, 0.28, 0.045);
                let rock = Vec3::new(0.29, 0.28, 0.25);
                let grass_weight = smooth(2.0, 4.5, height);
                let rock_weight = (1.0 - smooth(0.65, 0.9, normal.y))
                    .max(smooth(self.peak_height * 0.72, self.peak_height * 1.05, height) * 0.8);
                let color = sand.lerp(grass, grass_weight).lerp(rock, rock_weight)
                    * (0.85 + 0.25 * noise(p * 0.6, self.seed));
                vertices.push(Vertex {
                    position: [p.x, height, p.y],
                    normal: normal.to_array(),
                    color: color.to_array(),
                });
            }
        }
        Mesh::new(vertices, grid_indices(self.cells))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn island_has_land_submerged_edges_and_consistent_winding() {
        let island = Island {
            cells: 32,
            ..Default::default()
        };
        let mesh = island.mesh().unwrap();
        assert!(island.height_at(island.center) > 10.0);
        assert!(island.height_at(island.center + Vec2::X * island.radius * 1.4) < -3.0);
        assert_eq!(mesh.vertices().len(), 33 * 33);
        for v in mesh.vertices() {
            assert!((Vec3::from_array(v.normal).length() - 1.0).abs() < 1e-5);
        }
        for t in mesh.indices().as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| Vec3::from_array(mesh.vertices()[i as usize].position));
            assert!((b - a).cross(c - a).y > 0.0);
        }
        assert_eq!(mesh.vertices(), island.mesh().unwrap().vertices());
        assert_ne!(
            mesh.vertices(),
            Island { seed: 11, ..island }.mesh().unwrap().vertices()
        );
    }
    #[test]
    fn invalid_island_settings_are_rejected() {
        assert!(
            Island {
                radius: 0.0,
                ..Default::default()
            }
            .mesh()
            .is_err()
        );
        assert!(
            Island {
                cells: u32::MAX,
                ..Default::default()
            }
            .mesh()
            .is_err()
        );
    }
}
