//! Engine-level 3D fluid scene objects.
//!
//! FluidWorld describes water independently from rendering. Solvers consume emitters and
//! colliders; a falling stream is therefore an outcome of gravity and geometry, not an object.

use crate::render::EngineResult;
use glam::{Quat, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub center: Vec3,
    pub half_extent: Vec3,
}
impl Aabb {
    pub fn new(center: Vec3, half_extent: Vec3) -> EngineResult<Self> {
        if !center.is_finite() || !half_extent.is_finite() || half_extent.min_element() <= 0.0 {
            return Err("fluid AABB requires a finite center and positive finite half extents".into());
        }
        Ok(Self { center, half_extent })
    }
    pub fn min(self) -> Vec3 { self.center - self.half_extent }
    pub fn max(self) -> Vec3 { self.center + self.half_extent }
}

#[derive(Clone, Copy, Debug)]
pub struct OrientedBox {
    pub center: Vec3,
    pub half_extent: Vec3,
    pub rotation: Quat,
}

#[derive(Clone, Copy, Debug)]
pub enum FluidCollider {
    Box(OrientedBox),
}
impl FluidCollider {
    pub fn cuboid(center: Vec3, half_extent: Vec3) -> EngineResult<Self> {
        Self::oriented_cuboid(center, half_extent, Quat::IDENTITY)
    }
    pub fn oriented_cuboid(center: Vec3, half_extent: Vec3, rotation: Quat) -> EngineResult<Self> {
        Aabb::new(center, half_extent)?;
        if !rotation.is_finite() || rotation.length_squared() <= f32::EPSILON {
            return Err("fluid oriented box requires a finite nonzero rotation".into());
        }
        Ok(Self::Box(OrientedBox { center, half_extent, rotation: rotation.normalize() }))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FluidEmitter {
    pub volume: Aabb,
    pub velocity: Vec3,
    pub rate: f32,
}
impl FluidEmitter {
    pub fn new(volume: Aabb, velocity: Vec3, rate: f32) -> EngineResult<Self> {
        if !velocity.is_finite() || !rate.is_finite() || rate < 0.0 {
            return Err("fluid emitter requires finite velocity and a nonnegative finite rate".into());
        }
        Ok(Self { volume, velocity, rate })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FluidWorld {
    pub colliders: Vec<FluidCollider>,
    pub emitters: Vec<FluidEmitter>,
}
impl FluidWorld {
    pub fn add_collider(&mut self, collider: FluidCollider) { self.colliders.push(collider); }
    pub fn add_emitter(&mut self, emitter: FluidEmitter) { self.emitters.push(emitter); }
}
