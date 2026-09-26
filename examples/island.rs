//! The island demo only builds CPU scene data; rendering belongs to the engine.

mod support;

use gigantomachia::{
    camera::Camera,
    render::EngineResult,
    scene::{MeshInstance, Scene},
    terrain::Island,
    water::Water,
};
use glam::Vec3;
use std::sync::Arc;

fn main() -> EngineResult<()> {
    let island = Island::default();
    let scene = Scene {
        fluids: Vec::new(),
        gpu_fluids: Vec::new(),
        camera: Camera::looking_at(Vec3::new(34.0, 24.0, 42.0), Vec3::new(0.0, 4.0, -12.0))?,
        sun: Default::default(),
        meshes: vec![MeshInstance::new(Arc::new(island.mesh()?))],
        ocean: Some(Water {
            amplitude: 0.55,
            ..Default::default()
        }),
    };
    support::run(support::Demo::new("Island", scene, Some(island)))
}
