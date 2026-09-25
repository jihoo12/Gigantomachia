//! A small above-water fly camera. All angles are in radians.

use glam::{Mat4, Vec3};

#[derive(Clone, Debug)]
pub struct Camera {
    pub position: Vec3,
    yaw: f32,
    pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 5.5, 18.0),
            yaw: 0.0,
            pitch: -0.20,
        }
    }
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx * 0.003).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch - dy * 0.003).clamp(-1.45, 1.45);
    }

    /// Move along horizontal right/forward and world up; normalize diagonal input.
    pub fn travel(&mut self, axes: Vec3, dt: f32, fast: bool) {
        let forward = Vec3::new(self.yaw.sin(), 0.0, -self.yaw.cos());
        let right = forward.cross(Vec3::Y);
        self.position += (right * axes.x + Vec3::Y * axes.y + forward * axes.z).normalize_or_zero()
            * dt.clamp(0.0, 0.1)
            * if fast { 30.0 } else { 8.0 };
        // The current shading model is only defined above the largest supported crest.
        self.position.y = self.position.y.clamp(3.5, 80.0);
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(55.0_f32.to_radians(), aspect, 0.1, 700.0)
            * Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_uses_zero_to_one_depth() {
        let camera = Camera::default();
        let matrix = camera.view_projection(16.0 / 9.0);
        let near = matrix.project_point3(camera.position + camera.forward() * 0.1);
        let far = matrix.project_point3(camera.position + camera.forward() * 700.0);
        assert!(near.z.abs() < 0.0001);
        assert!((far.z - 1.0).abs() < 0.0001);
    }

    #[test]
    fn diagonal_movement_is_not_faster() {
        let mut straight = Camera::default();
        let mut diagonal = straight.clone();
        let origin = straight.position;
        straight.travel(Vec3::Z, 0.1, false);
        diagonal.travel(Vec3::new(1.0, 0.0, 1.0), 0.1, false);
        assert!(
            ((straight.position - origin).length() - (diagonal.position - origin).length()).abs()
                < 0.00001
        );
    }

    #[test]
    fn camera_stays_above_water_and_avoids_vertical_singularity() {
        let mut camera = Camera::default();
        camera.look(0.0, -100000.0);
        for _ in 0..100 {
            camera.travel(-Vec3::Y, 1.0, true);
        }
        assert_eq!(camera.position.y, 3.5);
        assert!(camera.view_projection(1.0).is_finite());
    }
}
