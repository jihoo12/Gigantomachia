//! Ocean-only scene using the same engine as the island example.

mod support;

use gigantomachia::{render::EngineResult, scene::Scene, water::Water};

fn main() -> EngineResult<()> {
    let scene = Scene {
        ocean: Some(Water::default()),
        ..Default::default()
    };
    support::run(support::Demo::new("Water", scene, None))
}
