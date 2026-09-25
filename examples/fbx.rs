//! Static FBX viewer. File import stays CPU-only and rendering uses the normal engine passes.

mod support;

use gigantomachia::{
    asset::load_fbx,
    camera::Camera,
    mesh::{Mesh, Vertex},
    render::EngineResult,
    scene::{MeshInstance, Scene},
};
use glam::{Mat4, Vec3};
use std::{path::PathBuf, sync::Arc};

fn main() -> EngineResult<()> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.iter().any(|arg| arg == "--help") {
        println!(
            "FBX viewer\n  cargo run --example fbx -- [model.fbx] [--headless output.png | --frames COUNT]\n\nWithout a path, loads the bundled static-scene fixture.\nWASD/QE: move | RMB drag: look | Shift: faster | 1: shadows | R: reset | Esc: exit\nStatic meshes and base/vertex colors only; textures, skinning, and animation playback are unsupported."
        );
        return Ok(());
    }
    let path = if args
        .first()
        .is_some_and(|arg| !arg.to_string_lossy().starts_with("--"))
    {
        PathBuf::from(args.remove(0))
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/static_scene_ascii.fbx")
    };
    let mut model = load_fbx(&path)?;
    for warning in &model.warnings {
        eprintln!("FBX: {warning}");
    }
    println!(
        "Loaded {}: {} meshes, bounds {:?} .. {:?} meters",
        path.display(),
        model.meshes.len(),
        model.bounds_min,
        model.bounds_max
    );
    // Viewer-only placement: preserve model proportions, fit the longest axis to eight meters.
    let size = model.bounds_max - model.bounds_min;
    let scale = 8.0 / size.max_element().max(0.001);
    let center = (model.bounds_min + model.bounds_max) * 0.5;
    let offset = Vec3::new(-center.x, -model.bounds_min.y, -center.z) * scale;
    let placement =
        Mat4::from_scale_rotation_translation(Vec3::splat(scale), glam::Quat::IDENTITY, offset);
    for instance in &mut model.meshes {
        instance.set_transform(placement)?;
    }
    let vertices = [
        [-7.0, -0.05, -7.0],
        [-7.0, -0.05, 7.0],
        [7.0, -0.05, -7.0],
        [7.0, -0.05, 7.0],
    ]
    .map(|position| Vertex {
        position,
        normal: [0.0, 1.0, 0.0],
        color: [0.25, 0.29, 0.30],
    });
    model.meshes.push(MeshInstance::new(Arc::new(Mesh::new(
        vertices.to_vec(),
        vec![0, 1, 2, 2, 1, 3],
    )?)));
    let scene = Scene {
        camera: Camera::looking_at(
            Vec3::new(10.0, 7.0, 12.0),
            Vec3::new(0.0, size.y * scale * 0.5, 0.0),
        )?,
        meshes: model.meshes,
        ..Default::default()
    };
    let options = args
        .into_iter()
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "viewer options must be UTF-8")
        })
        .collect::<Result<Vec<_>, _>>()?;
    support::run_with_args(support::Demo::new("FBX", scene, None), options)
}
