//! Optional water surface parameters. Playback controls belong to the application.

#[derive(Clone, Copy, Debug)]
pub struct Water {
    /// Displacement multiplier, clamped to 0..2 on upload.
    pub amplitude: f32,
    /// Explicit wave time, allowing paused or deterministic rendering.
    pub time: f32,
    /// Mean sea level in world-space meters.
    pub level: f32,
    /// Sample submerged scene color instead of using only the deep-water tint.
    pub refraction: bool,
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
            amplitude: 1.0,
            time: 0.0,
            level: 0.0,
            refraction: true,
            refraction_strength: 0.7,
            absorption: [0.42, 0.12, 0.055],
            foam_strength: 1.0,
            foam_width: 1.4,
        }
    }
}
