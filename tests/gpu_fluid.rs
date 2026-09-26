//! Real Vulkan compute/readback checks; the frame loop itself never reads particle data back.
use gigantomachia::{
    camera::Camera,
    fluid::{BoxCollider, Fluid, GpuFluid},
    render::{EngineResult, Gpu, OffscreenTarget, Renderer},
    scene::Scene,
};
use glam::Vec3;
use std::sync::Arc;
fn bounds(min: [f32; 3], max: [f32; 3]) -> BoxCollider {
    BoxCollider::new(min.into(), max.into()).unwrap()
}
fn advance(gpu: &Gpu, fluid: &GpuFluid, colliders: &[BoxCollider], steps: u32) -> EngineResult<()> {
    for _ in 0..steps / 8 {
        fluid.step(gpu, colliders, 8)?;
    }
    fluid.step(gpu, colliders, steps % 8)?;
    Ok(())
}
#[test]
#[ignore = "requires Vulkan; run inside nix develop"]
fn gpu_gravity_matches_cpu_and_validates_domain_and_batches() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut cpu = Fluid::block(Vec3::new(-0.2, 2.0, -0.2), [3, 3, 3], 0.16)?;
    assert!(GpuFluid::new(&gpu, &cpu, Vec3::ZERO, Vec3::splat(f32::NAN)).is_err());
    assert!(GpuFluid::new(&gpu, &cpu, Vec3::ZERO, Vec3::splat(100.0)).is_err());
    let fluid = GpuFluid::new(&gpu, &cpu, Vec3::splat(-1.0), Vec3::new(1.0, 3.0, 1.0))?;
    assert!(fluid.step(&gpu, &[], 9).is_err());
    assert!(
        fluid
            .step(&gpu, &vec![bounds([-1.0; 3], [0.0; 3]); 65], 1)
            .is_err()
    );
    for _ in 0..30 {
        cpu.step(&[]);
    }
    advance(&gpu, &fluid, &[], 30)?;
    fluid.reconstruct(&gpu);
    let result = fluid.readback(&gpu)?;
    assert_eq!(result.positions.len(), cpu.particle_count());
    for (a, b) in result.positions.iter().zip(cpu.positions()) {
        assert!(a.distance(*b) < 0.001, "CPU {b:?}, GPU {a:?}");
    }
    assert!(result.vertex_count > 0 && result.vertex_count % 3 == 0);
    assert!(!result.surface_overflow);
    assert_eq!(result.outside_surface_domain, 0);
    // Leaving the surface domain must be reported; particles are retained, not clamped/deleted.
    advance(&gpu, &fluid, &[], 120)?;
    fluid.reconstruct(&gpu);
    let outside = fluid.readback(&gpu)?;
    assert_eq!(outside.positions.len(), 27);
    assert_eq!(outside.outside_surface_domain, 27);
    assert_eq!(outside.vertex_count, 0);
    assert!(pollster::block_on(gpu.device.pop_error_scope()).is_none());
    Ok(())
}
#[test]
#[ignore = "requires Vulkan; run inside nix develop"]
fn gpu_container_retains_then_drains_without_losing_particles() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let cpu = Fluid::block(Vec3::new(-0.8, 1.6, -0.8), [10, 5, 10], 0.16)?;
    let fluid = GpuFluid::new(
        &gpu,
        &cpu,
        Vec3::new(-3.0, -0.4, -3.0),
        Vec3::new(3.0, 3.5, 5.0),
    )?;
    let mut solids = vec![
        bounds([-3.0, -0.3, -3.0], [3.0, 0.0, 5.0]),
        bounds([-1.2, 1.3, -1.2], [1.2, 1.5, 1.2]),
        bounds([-1.2, 1.5, -1.2], [-1.0, 3.0, 1.2]),
        bounds([1.0, 1.5, -1.2], [1.2, 3.0, 1.2]),
        bounds([-1.2, 1.5, -1.2], [1.2, 3.0, -1.0]),
        bounds([-0.2, 0.0, 1.8], [0.2, 0.5, 2.3]),
        bounds([-1.2, 1.5, 1.0], [1.2, 3.0, 1.2]),
    ];
    advance(&gpu, &fluid, &solids, 120)?;
    let closed = fluid.readback(&gpu)?;
    assert!(
        closed
            .positions
            .iter()
            .all(|p| p.is_finite() && p.y > 1.5 && p.x.abs() < 1.0 && p.z.abs() < 1.0)
    );
    solids.pop();
    advance(&gpu, &fluid, &solids, 300)?;
    fluid.reconstruct(&gpu);
    let open = fluid.readback(&gpu)?;
    assert_eq!(open.positions.len(), 500);
    assert!(open.positions.iter().filter(|p| p.y < 1.0).count() > 10);
    assert!(!open.surface_overflow);
    for p in open.positions {
        assert!(p.is_finite() && p.y > 0.0);
        for c in &solids {
            assert!(
                !(p.cmpgt(c.min()).all() && p.cmplt(c.max()).all()),
                "particle inside collider: {p:?}"
            );
        }
    }
    assert!(pollster::block_on(gpu.device.pop_error_scope()).is_none());
    Ok(())
}
#[test]
#[ignore = "requires Vulkan; run inside nix develop"]
fn gpu_generated_surface_renders_pauses_resizes_and_disappears_on_removal() -> EngineResult<()> {
    let gpu = pollster::block_on(Gpu::headless())?;
    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let seed = Fluid::block(Vec3::new(-0.35, 0.8, -0.35), [5, 5, 5], 0.16)?;
    let fluid = Arc::new(GpuFluid::new(
        &gpu,
        &seed,
        Vec3::splat(-2.0),
        Vec3::splat(2.0),
    )?);
    let mut scene = Scene {
        camera: Camera::looking_at(Vec3::new(3.0, 2.0, 3.0), Vec3::Y * 0.6)?,
        ..Default::default()
    };
    let mut renderer = Renderer::new(&gpu, OffscreenTarget::FORMAT, 320, 180)?;
    let target = OffscreenTarget::new(&gpu, 320, 180)?;
    renderer.render(&gpu, &target.view(), &scene);
    let empty = target.read_rgba8(&gpu)?;
    scene.gpu_fluids.push(fluid.clone());
    renderer.render(&gpu, &target.view(), &scene);
    let initial = target.read_rgba8(&gpu)?;
    assert!(initial.iter().zip(&empty).filter(|(a, b)| a != b).count() > 100);
    renderer.render(&gpu, &target.view(), &scene);
    assert_eq!(initial, target.read_rgba8(&gpu)?);
    let camera = scene.camera.clone();
    scene.camera.travel(Vec3::X, 0.05, false);
    renderer.render(&gpu, &target.view(), &scene);
    assert_ne!(initial, target.read_rgba8(&gpu)?);
    // Moving the camera while paused must not advance the simulation.
    assert_eq!(fluid.readback(&gpu)?.positions, seed.positions());
    scene.camera = camera;
    advance(&gpu, &fluid, &[], 30)?;
    fluid.reconstruct(&gpu);
    renderer.render(&gpu, &target.view(), &scene);
    let fallen = target.read_rgba8(&gpu)?;
    assert_ne!(initial, fallen);
    renderer.resize(&gpu, 640, 360)?;
    renderer.resize(&gpu, 320, 180)?;
    renderer.render(&gpu, &target.view(), &scene);
    assert_eq!(fallen, target.read_rgba8(&gpu)?);
    // Replacing the Arc resets state and frees old resources after queued work completes.
    scene.gpu_fluids[0] = Arc::new(GpuFluid::new(
        &gpu,
        &seed,
        Vec3::splat(-2.0),
        Vec3::splat(2.0),
    )?);
    renderer.render(&gpu, &target.view(), &scene);
    let reset = target.read_rgba8(&gpu)?;
    let difference = initial
        .iter()
        .zip(reset)
        .filter(|(a, b)| a.abs_diff(*b) > 2)
        .count();
    assert!(
        difference < 100,
        "reset should restore the initial surface: {difference}"
    );
    scene.gpu_fluids.clear();
    renderer.render(&gpu, &target.view(), &scene);
    assert_eq!(empty, target.read_rgba8(&gpu)?);
    assert!(pollster::block_on(gpu.device.pop_error_scope()).is_none());
    Ok(())
}
