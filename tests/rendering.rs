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

fn plane(half_size: f32, slope: f32, color: [f32; 3]) -> EngineResult<Arc<Mesh>> {
    let normal = Vec3::new(-slope, 1.0, 0.0).normalize().to_array();
    let vertices = [
        (-half_size, -half_size),
        (-half_size, half_size),
        (half_size, -half_size),
        (half_size, half_size),
    ]
    .map(|(x, z)| Vertex {
        position: [x, x * slope, z],
        normal,
        color,
    });
    Ok(Arc::new(Mesh::new(
        vertices.to_vec(),
        vec![0, 1, 2, 2, 1, 3],
    )?))
}

fn pixel_at(camera: &Camera, point: Vec3, width: usize, height: usize) -> usize {
    let ndc = camera
        .view_projection(width as f32 / height as f32)
        .project_point3(point);
    let x = ((ndc.x * 0.5 + 0.5) * width as f32) as usize;
    let y = ((0.5 - ndc.y * 0.5) * height as f32) as usize;
    assert!(x < width && y < height);
    (y * width + x) * 4
}

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
        sun: Default::default(),
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
        sun: Default::default(),
        meshes: vec![object],
        water: None,
    };
    let land = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water = Some(Water {
        refraction: false,
        foam_strength: 0.0,
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

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn directional_shadows_darken_mesh_and_water_receivers() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    let ground = MeshInstance::new(plane(18.0, 0.0, [0.6; 3])?);
    let mut caster = MeshInstance::new(plane(3.0, 0.0, [0.4; 3])?);
    caster.set_transform(Mat4::from_translation(Vec3::Y * 5.0))?;
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(12.0, 14.0, 20.0), Vec3::ZERO)?,
        meshes: vec![ground, caster],
        ..Default::default()
    };
    scene.sun.direction = Vec3::new(-1.0, 2.0, -1.0);
    scene.sun.shadows = false;
    let lit = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.sun.shadows = true;
    let shaded = frame(&gpu, &mut renderer, &target, &scene)?;
    let darker = |a: &[u8], b: &[u8]| {
        a.as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0)
            .filter(|(x, y)| u16::from(x[0]) > u16::from(y[0]) + 8)
            .count()
    };
    assert!(
        darker(&lit, &shaded) > 80,
        "the raised mesh must cast a shadow on the ground"
    );
    scene.meshes.remove(0);
    scene.water = Some(Water {
        amplitude: 0.0,
        refraction: false,
        foam_strength: 0.0,
        ..Default::default()
    });
    let shadow_water = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.sun.shadows = false;
    let lit_water = frame(&gpu, &mut renderer, &target, &scene)?;
    assert!(
        darker(&lit_water, &shadow_water) > 20,
        "the same caster must shadow the water"
    );
    scene.meshes.clear();
    let empty_lit = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.sun.shadows = true;
    assert_eq!(
        empty_lit,
        frame(&gpu, &mut renderer, &target, &scene)?,
        "removed casters must not leave stale shadows"
    );
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn refraction_transmits_shallows_distorts_and_absorbs_with_depth() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    let mut bottom = MeshInstance::new(plane(35.0, 0.0, [0.9, 0.15, 0.04])?);
    bottom.set_transform(Mat4::from_translation(-Vec3::Y * 0.5))?;
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(0.0, 10.0, 14.0), Vec3::ZERO)?,
        meshes: vec![bottom],
        water: Some(Water {
            amplitude: 0.0,
            refraction: false,
            foam_strength: 0.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    scene.sun.shadows = false;
    let opaque = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water.as_mut().unwrap().refraction = true;
    let shallow = frame(&gpu, &mut renderer, &target, &scene)?;
    let center = (90 * 320 + 160) * 4;
    assert!(
        u16::from(shallow[center]) > u16::from(opaque[center]) + 30,
        "the shallow red seabed must transmit through water"
    );
    scene.meshes[0].set_transform(Mat4::from_translation(-Vec3::Y * 25.0))?;
    let deep = frame(&gpu, &mut renderer, &target, &scene)?;
    assert!(
        u16::from(shallow[center]) > u16::from(deep[center]) + 30,
        "red light must be absorbed as depth increases"
    );

    // A finite colored submerged patch provides edges that visibly move under refraction.
    scene.meshes[0].mesh = plane(4.0, 0.0, [0.9, 0.15, 0.04])?;
    scene.meshes[0].set_transform(Mat4::from_translation(-Vec3::Y * 2.0))?;
    scene.water.as_mut().unwrap().amplitude = 0.6;
    scene.water.as_mut().unwrap().refraction_strength = 0.0;
    let undistorted = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water.as_mut().unwrap().refraction_strength = 1.0;
    let distorted = frame(&gpu, &mut renderer, &target, &scene)?;
    let changed = undistorted
        .as_chunks::<4>()
        .0
        .iter()
        .zip(distorted.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 30,
        "refraction strength must alter submerged color sampling"
    );

    renderer.resize(&gpu, 173, 257)?;
    let portrait = OffscreenTarget::new(&gpu, 173, 257)?;
    let resized = frame(&gpu, &mut renderer, &portrait, &scene)?;
    let mut fresh = Renderer::new(&gpu, OffscreenTarget::FORMAT, 173, 257)?;
    assert_eq!(
        resized,
        frame(&gpu, &mut fresh, &portrait, &scene)?,
        "resize must recreate all scene color/depth sampling bindings"
    );
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn foam_tracks_shallow_depth_without_covering_dry_land_or_open_ocean() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    let mut beach = MeshInstance::new(plane(12.0, 0.2, [0.5, 0.35, 0.16])?);
    beach.set_transform(Mat4::from_translation(-Vec3::Y))?;
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(0.0, 10.0, 14.0), Vec3::ZERO)?,
        meshes: vec![beach],
        water: Some(Water {
            amplitude: 0.0,
            foam_strength: 0.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    scene.sun.shadows = false;
    let clear = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water.as_mut().unwrap().foam_strength = 1.0;
    let foamy = frame(&gpu, &mut renderer, &target, &scene)?;
    let shore = pixel_at(&scene.camera, Vec3::new(3.0, 0.0, 0.0), 320, 180);
    assert!(
        u16::from(foamy[shore]) > u16::from(clear[shore]) + 10,
        "foam must brighten the shallow contact region"
    );
    for point in [Vec3::new(-8.0, 0.0, 0.0), Vec3::new(8.0, 0.6, 0.0)] {
        let i = pixel_at(&scene.camera, point, 320, 180);
        assert_eq!(
            &clear[i..i + 4],
            &foamy[i..i + 4],
            "deep water and dry land must not gain shoreline foam"
        );
    }
    scene.water.as_mut().unwrap().time = 1.25;
    assert_ne!(
        foamy,
        frame(&gpu, &mut renderer, &target, &scene)?,
        "shore foam must evolve with wave time"
    );
    scene.meshes.clear();
    let ocean = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.water.as_mut().unwrap().foam_strength = 0.0;
    assert_eq!(
        ocean,
        frame(&gpu, &mut renderer, &target, &scene)?,
        "clear depth must never become a false shoreline"
    );
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}

#[test]
#[ignore = "requires a Vulkan adapter; run inside nix develop"]
fn imported_fbx_uses_standard_mesh_rendering() -> EngineResult<()> {
    let model =
        gigantomachia::asset::load_fbx_bytes(include_bytes!("fixtures/static_scene_binary.fbx"))?;
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(9.0, 7.0, 12.0), Vec3::new(0.5, 2.0, 0.0))?,
        ..Default::default()
    };
    let empty = frame(&gpu, &mut renderer, &target, &scene)?;
    scene.meshes = model.meshes;
    let imported = frame(&gpu, &mut renderer, &target, &scene)?;
    let changed = empty
        .as_chunks::<4>()
        .0
        .iter()
        .zip(imported.as_chunks::<4>().0)
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 500, "imported meshes must occupy visible pixels");
    assert_eq!(renderer.resident_meshes(), 2);
    if let Some(error) = pollster::block_on(gpu.device.pop_error_scope()) {
        return Err(error.into());
    }
    Ok(())
}
