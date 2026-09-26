//! Renderable scene data. May share GPU fluid handles; no window or demo controls are stored here.

use crate::{camera::Camera, mesh::Mesh, render::EngineResult, terrain::OceanSurface};
use glam::{Mat4, Vec3};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub camera: Camera,
    pub meshes: Vec<MeshInstance>,
    /// Authored ocean/terrain surface; not a simulated fluid volume.
    pub ocean: Option<OceanSurface>,
    /// Reconstructed surfaces from independent Fluid objects.
    pub fluids: Vec<crate::fluid::FluidSurface>,
    /// GPU-resident simulations, created on the renderer device.
    pub gpu_fluids: Vec<Arc<crate::fluid::GpuFluid>>,
    pub sun: Sun,
}

/// Direction from a surface toward the sun. Ambient sky light remains unshadowed.
#[derive(Clone, Copy, Debug)]
pub struct Sun {
    pub direction: Vec3,
    pub shadows: bool,
}

impl Default for Sun {
    fn default() -> Self {
        Self {
            direction: Vec3::new(-0.36, 0.27, -0.893),
            shadows: true,
        }
    }
}

impl Sun {
    pub(crate) fn direction(&self) -> Vec3 {
        if self.direction.is_finite()
            && self.direction.length_squared() > 1e-8
            && self.direction.length_squared().is_finite()
        {
            self.direction.normalize()
        } else {
            Self::default().direction.normalize()
        }
    }
}

#[derive(Clone, Debug)]
pub struct MeshInstance {
    pub mesh: Arc<Mesh>,
    transform: Mat4,
}

impl MeshInstance {
    pub fn new(mesh: Arc<Mesh>) -> Self {
        Self {
            mesh,
            transform: Mat4::IDENTITY,
        }
    }
    pub fn transform(&self) -> Mat4 {
        self.transform
    }

    /// Require an invertible affine transform with positive winding.
    /// Reflected/negative-determinant transforms need a different culling pipeline.
    pub fn set_transform(&mut self, transform: Mat4) -> EngineResult<()> {
        if !transform.is_finite()
            || !transform.inverse().is_finite()
            || transform.determinant() <= 1e-8
            || transform.x_axis.w != 0.0
            || transform.y_axis.w != 0.0
            || transform.z_axis.w != 0.0
            || transform.w_axis.w != 1.0
        {
            return Err(
                "mesh transform must be finite, affine, invertible, and preserve winding".into(),
            );
        }
        self.transform = transform;
        Ok(())
    }
}
