//! Optional water surface parameters. Playback controls belong to the application.

use crate::render::EngineResult;
use glam::Vec2;

/// Fixed, axis-aligned rectangle in world XZ. The renderer stretches its grid to this area.
#[derive(Clone, Copy, Debug)]
pub struct WaterBounds {
    pub(crate) center: Vec2,
    pub(crate) half_extent: Vec2,
}

impl WaterBounds {
    pub fn new(center: Vec2, half_extent: Vec2) -> EngineResult<Self> {
        if !center.is_finite()
            || !half_extent.is_finite()
            || half_extent.min_element() <= 0.0
            || !(center + half_extent).is_finite()
            || !(center - half_extent).is_finite()
            || !(half_extent * 2.0).is_finite()
        {
            return Err(
                "water bounds require a finite center and positive finite half extents".into(),
            );
        }
        Ok(Self {
            center,
            half_extent,
        })
    }
    pub fn center(self) -> Vec2 {
        self.center
    }
    pub fn half_extent(self) -> Vec2 {
        self.half_extent
    }
}

/// Surface shading and geometry quality. Both modes use the same wave motion and effects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaterStyle {
    #[default]
    Stylized,
    Realistic,
}

#[derive(Clone, Copy, Debug)]
pub struct Water {
    pub style: WaterStyle,
    /// None follows the camera as an ocean; Some fixes a rectangular surface in world space.
    pub bounds: Option<WaterBounds>,
    pub waterfall: Option<Waterfall>,
    /// Realistic-mode short-wave slope multiplier, clamped to 0..2.
    pub ripple_strength: f32,
    /// Realistic-mode perceptual roughness, clamped to 0.08..0.6.
    pub roughness: f32,
    /// Displacement multiplier, clamped to 0..2 on upload.
    pub amplitude: f32,
    /// Explicit wave time, allowing paused or deterministic rendering.
    pub time: f32,
    /// Mean sea level in world-space meters.
    pub level: f32,
    /// Sample submerged scene color instead of using only the deep-water tint.
    pub refraction: bool,
    /// Reflect above-water meshes across the mean water plane.
    pub reflections: bool,
    /// Screen-space refraction displacement multiplier, clamped to 0..1.
    pub refraction_strength: f32,
    /// Beer-Lambert absorption coefficients per meter, in linear RGB.
    pub absorption: [f32; 3],
    /// Shoreline foam opacity multiplier (0 disables foam), clamped to 0..1.
    pub foam_strength: f32,
    /// Maximum vertical water depth for foam, in meters.
    pub foam_width: f32,
}

impl Default for Water {
    fn default() -> Self {
        Self {
            style: WaterStyle::Stylized,
            bounds: None,
            waterfall: None,
            ripple_strength: 1.0,
            roughness: 0.22,
            amplitude: 1.0,
            time: 0.0,
            level: 0.0,
            refraction: true,
            reflections: true,
            refraction_strength: 0.7,
            absorption: [0.42, 0.12, 0.055],
            foam_strength: 1.0,
            foam_width: 1.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_water_requires_finite_positive_dimensions() {
        for half in [
            Vec2::ZERO,
            Vec2::new(-1.0, 2.0),
            Vec2::splat(f32::INFINITY),
            Vec2::splat(f32::MAX),
        ] {
            assert!(WaterBounds::new(Vec2::ZERO, half).is_err());
        }
        assert!(WaterBounds::new(Vec2::splat(f32::NAN), Vec2::ONE).is_err());
        let bounds = WaterBounds::new(Vec2::new(3.0, -2.0), Vec2::new(4.0, 2.0)).unwrap();
        assert_eq!(bounds.center(), Vec2::new(3.0, -2.0));
        assert_eq!(bounds.half_extent(), Vec2::new(4.0, 2.0));
    }
}

/// Visual spill: a ballistic sheet, a receiving puddle, and looping splash droplets.
/// This does not simulate fluid volume or collisions.
#[derive(Clone, Copy, Debug)]
pub struct Waterfall {
    pub(crate) origin: glam::Vec3,
    pub(crate) direction: Vec2,
    pub(crate) width: f32,
    pub(crate) drop: f32,
}

impl Waterfall {
    pub fn new(origin: glam::Vec3, direction: Vec2, width: f32, drop: f32) -> EngineResult<Self> {
        if !origin.is_finite()
            || !direction.is_finite()
            || direction.length_squared() < 1e-8
            || !direction.length_squared().is_finite()
            || !width.is_finite()
            || !drop.is_finite()
            || !(0.05..=100.0).contains(&width)
            || !(0.05..=100.0).contains(&drop)
        {
            return Err("waterfall requires a finite origin, nonzero direction, and width/drop in 0.05..=100 meters".into());
        }
        Ok(Self {
            origin,
            direction: direction.normalize(),
            width,
            drop,
        })
    }
}

#[cfg(test)]
mod waterfall_tests {
    use super::*;
    #[test]
    fn invalid_spills_are_rejected_and_direction_is_normalized() {
        assert!(Waterfall::new(glam::Vec3::ZERO, Vec2::ZERO, 1.0, 1.0).is_err());
        assert!(Waterfall::new(glam::Vec3::ZERO, Vec2::Y, 0.0, 1.0).is_err());
        assert!(Waterfall::new(glam::Vec3::ZERO, Vec2::Y, 1.0, f32::NAN).is_err());
        assert!(Waterfall::new(glam::Vec3::splat(f32::INFINITY), Vec2::Y, 1.0, 1.0).is_err());
        let spill = Waterfall::new(glam::Vec3::Y, Vec2::new(3.0, 4.0), 1.0, 1.0).unwrap();
        assert!((spill.direction.length() - 1.0).abs() < 1e-6);
    }
}
