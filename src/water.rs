//! Optional water surface parameters. Playback controls belong to the application.

#[derive(Clone, Copy, Debug)]
pub struct Water {
    /// Displacement multiplier, clamped to 0..2 on upload.
    pub amplitude: f32,
    /// Explicit wave time, allowing paused or deterministic rendering.
    pub time: f32,
    /// Mean sea level in world-space meters.
    pub level: f32,
}

impl Default for Water {
    fn default() -> Self {
        Self {
            amplitude: 1.0,
            time: 0.0,
            level: 0.0,
        }
    }
}
