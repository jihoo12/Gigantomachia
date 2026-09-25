//! Opt-in integration checks for the public engine API. No window or example code is required.

use gigantomachia::{
    camera::Camera,
    mesh::{Mesh, Vertex},
    render::{EngineResult, Gpu, OffscreenTarget, Renderer},
    scene::{MeshInstance, Scene},
    terrain::Island,
    water::Water,
};
use glam::{Mat4, Vec3};
use std::sync::Arc;

fn frame(
    gpu: &Gpu,
    renderer: &mut Renderer,
    target: &OffscreenTarget,
    scene: &Scene,
) -> EngineResult<Vec<u8>> {
    renderer.render(gpu, &target.view(), scene);
    target.read_rgba8(gpu)
}

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn water_animation_flat_surface_and_resize() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    let mut scene = Scene {
        water: Some(Water::default()),
        ..Default::default()
    };
    let first = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water.as_mut().unwrap().time = 1.25;
    let second = frame(&gpu, &mut renderer, &target, &scene)?;
    let changed = first
        .as_chunks::<4>()
        .0
        .iter()
        .zip(second.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 320 * 180 / 10, "waves should visibly animate");
    assert!(
        second
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
    );
    scene.water.as_mut().unwrap().amplitude = 0.0;
    let flat = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water.as_mut().unwrap().time = 3.0;
    assert_eq!(flat, frame(&gpu, &mut renderer, &target, &scene)?);
    assert_ne!(flat, first);
    renderer.resize(&gpu, 0, 0)?;
    renderer.resize(&gpu, 173, 257)?;
    let portrait = OffscreenTarget::new(&gpu, 173, 257)?;
    assert_eq!(
        frame(&gpu, &mut renderer, &portrait, &scene)?.len(),
        173 * 257 * 4
    );
    assert!(OffscreenTarget::new(&gpu, 0, 10).is_err());
    assert!(Renderer::new(&gpu, OffscreenTarget::FORMAT, 0, 10).is_err());
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn island_scene_shares_geometry_updates_transforms_and_releases_assets() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(34.0, 24.0, 42.0), Vec3::new(0.0, 4.0, -12.0))?,
        meshes: vec![],
        water: Some(Water {
            amplitude: 0.55,
            time: 1.25,
            ..Default::default()
        }),
    };
    let ocean = frame(&gpu, &mut renderer, &target, &scene)?;
    let mesh = Arc::new(Island::default().mesh()?);
    scene.meshes.push(MeshInstance::new(mesh.clone()));
    let island = frame(&gpu, &mut renderer, &target, &scene)?;
    let changed = ocean
        .as_chunks::<4>()
        .0
        .iter()
        .zip(island.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 1500,
        "island should occupy a meaningful region of the image"
    );
    assert_eq!(renderer.resident_meshes(), 1);
    let mut second = MeshInstance::new(mesh);
    second.set_transform(Mat4::from_translation(Vec3::new(32.0, 0.0, 0.0)))?;
    scene.meshes.push(second);
    assert_ne!(island, frame(&gpu, &mut renderer, &target, &scene)?);
    assert_eq!(
        renderer.resident_meshes(),
        1,
        "shared CPU geometry must have only one GPU allocation"
    );
    scene.meshes[0].set_transform(Mat4::from_scale(Vec3::new(0.8, 1.3, 0.8)))?;
    scene.meshes.pop();
    assert_ne!(
        island,
        frame(&gpu, &mut renderer, &target, &scene)?,
        "transform changes must reach GPU uniforms"
    );
    scene.meshes.clear();
    assert_eq!(ocean, frame(&gpu, &mut renderer, &target, &scene)?);
    assert_eq!(renderer.resident_meshes(), 0);
    scene.water = None;
    assert_ne!(
        ocean,
        frame(&gpu, &mut renderer, &target, &scene)?,
        "water must be optional"
    );
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn opaque_meshes_and_water_share_depth() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 160, 90)?;
    let target = OffscreenTarget::new(&gpu, 160, 90)?;
    let vertices = [
        [-6.0, 0.0, -6.0],
        [-6.0, 0.0, 6.0],
        [6.0, 0.0, -6.0],
        [6.0, 0.0, 6.0],
    ]
    .map(|position| Vertex {
        position,
        normal: [0.0, 1.0, 0.0],
        color: [0.9, 0.04, 0.02],
    });
    let mesh = Arc::new(Mesh::new(vertices.to_vec(), vec![0, 1, 2, 2, 1, 3])?);
    let mut object = MeshInstance::new(mesh);
    assert!(object.set_transform(Mat4::from_scale(Vec3::ZERO)).is_err());
    assert!(
        object
            .set_transform(Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)))
            .is_err()
    );
    object.set_transform(Mat4::from_translation(Vec3::Y * 2.0))?;
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(0.0, 12.0, 12.0), Vec3::ZERO)?,
        meshes: vec![object],
        water: None,
    };
    let land = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water = Some(Water {
        amplitude: 0.0,
        ..Default::default()
    });
    let with_water = frame(&gpu, &mut renderer, &target, &scene)?;
    let center = (45 * 160 + 80) * 4;
    assert!(
        land[center] > land[center + 2] + 30,
        "center must hit the red test mesh"
    );
    assert_eq!(
        &land[center..center + 4],
        &with_water[center..center + 4],
        "water drawn last must not paint over land"
    );
    scene.meshes[0].set_transform(Mat4::from_translation(-Vec3::Y * 2.0))?;
    let submerged = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.meshes.clear();
    let ocean = frame(&gpu, &mut renderer, &target, &scene)?;
    assert_eq!(
        submerged, ocean,
        "opaque water must occlude the submerged mesh"
    );
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}
