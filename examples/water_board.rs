//! A shallow, bounded water surface on a wooden board with a raised rim.
mod support;

use gigantomachia::{
    camera::Camera,
    fluid::{Aabb, FluidCollider, FluidEmitter, FluidWorld},
    mesh::{Mesh, Vertex},
    render::EngineResult,
    scene::{MeshInstance, Scene, Sun},
    water::{Water, WaterBounds, WaterStyle},
};
use glam::{Vec2, Vec3};
use std::sync::Arc;

fn cuboid(center: Vec3, half: Vec3, color: [f32; 3]) -> EngineResult<MeshInstance> {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (normal, u, v) in [
        (Vec3::X, Vec3::Y, Vec3::Z),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::Z, Vec3::X),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, Vec3::Y, Vec3::X),
    ] {
        let first = vertices.len() as u32;
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            vertices.push(Vertex {
                position: (center + (normal + u * x + v * y) * half).to_array(),
                normal: normal.to_array(),
                color,
            });
        }
        indices.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    Ok(MeshInstance::new(Arc::new(Mesh::new(vertices, indices)?)))
}

fn main() -> EngineResult<()> {
    let mut meshes = vec![
        cuboid(
            Vec3::new(0.0, -0.2, 0.0),
            Vec3::new(14.0, 0.2, 14.0),
            [0.22, 0.25, 0.27],
        )?,
        cuboid(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(4.0, 0.22, 2.5),
            [0.27, 0.12, 0.045],
        )?,
    ];
    // Alternating slats make transmitted color and refraction easy to see.
    for i in 0..10 {
        let color = if i % 2 == 0 {
            [0.56, 0.31, 0.12]
        } else {
            [0.40, 0.20, 0.07]
        };
        meshes.push(cuboid(
            Vec3::new(-3.6 + i as f32 * 0.8, 1.235, 0.0),
            Vec3::new(0.393, 0.015, 2.32),
            color,
        )?);
    }
    for x in [-3.91, 3.91] {
        meshes.push(cuboid(
            Vec3::new(x, 1.55, 0.0),
            Vec3::new(0.09, 0.33, 2.5),
            [0.39, 0.19, 0.065],
        )?);
    }
    meshes.push(cuboid(
        Vec3::new(0.0, 1.55, -2.41),
        Vec3::new(3.82, 0.33, 0.09),
        [0.46, 0.24, 0.085],
    )?);
    // A central opening in the front rim is physical geometry; 3D water will discover it through collision.
    for x in [-2.31, 2.31] {
        meshes.push(cuboid(
            Vec3::new(x, 1.55, 2.41),
            Vec3::new(1.51, 0.33, 0.09),
            [0.46, 0.24, 0.085],
        )?);
    }
    for x in [-3.2, 3.2] {
        for z in [-1.8, 1.8] {
            meshes.push(cuboid(
                Vec3::new(x, 0.39, z),
                Vec3::new(0.2, 0.39, 0.2),
                [0.24, 0.10, 0.03],
            )?);
        }
    }
    // A lower receiving channel gives the spill somewhere physical to land and continue.
    // The channel is deliberately wider than the outlet so impact spray reads against water,
    // rather than disappearing into the ground plane.
    meshes.push(cuboid(
        Vec3::new(0.0, 0.055, 5.25),
        Vec3::new(2.35, 0.055, 3.15),
        [0.20, 0.095, 0.035],
    )?);
    for x in [-2.30, 2.30] {
        meshes.push(cuboid(
            Vec3::new(x, 0.34, 5.25),
            Vec3::new(0.08, 0.34, 3.15),
            [0.38, 0.18, 0.055],
        )?);
    }

    // Colored markers make reflected silhouettes easy to compare with the 5 key.
    meshes.push(cuboid(
        Vec3::new(-1.2, 2.10, -1.5),
        Vec3::new(0.3, 0.85, 0.3),
        [0.72, 0.035, 0.015],
    )?);
    meshes.push(cuboid(
        Vec3::new(1.5, 1.80, -1.7),
        Vec3::new(0.3, 0.55, 0.3),
        [0.035, 0.28, 0.65],
    )?);
    let mut fluid = FluidWorld::default();
    fluid.add_collider(FluidCollider::cuboid(Vec3::new(0.0, 1.0, 0.0), Vec3::new(4.0, 0.22, 2.5))?);
    for x in [-3.91, 3.91] { fluid.add_collider(FluidCollider::cuboid(Vec3::new(x, 1.55, 0.0), Vec3::new(0.09, 0.33, 2.5))?); }
    fluid.add_collider(FluidCollider::cuboid(Vec3::new(0.0, 1.55, -2.41), Vec3::new(3.82, 0.33, 0.09))?);
    for x in [-2.31, 2.31] { fluid.add_collider(FluidCollider::cuboid(Vec3::new(x, 1.55, 2.41), Vec3::new(1.51, 0.33, 0.09))?); }
    fluid.add_collider(FluidCollider::cuboid(Vec3::new(0.0, 0.055, 5.25), Vec3::new(2.35, 0.055, 3.15))?);
    for x in [-2.30, 2.30] { fluid.add_collider(FluidCollider::cuboid(Vec3::new(x, 0.34, 5.25), Vec3::new(0.08, 0.34, 3.15))?); }
    fluid.add_emitter(FluidEmitter::new(Aabb::new(Vec3::new(0.0, 1.58, -1.55), Vec3::new(0.40, 0.12, 0.20))?, Vec3::new(0.0, 0.0, 0.72), 900.0)?);

    let scene = Scene {
        camera: Camera::looking_at(Vec3::new(6.5, 5.5, 7.5), Vec3::new(0.0, 1.1, 0.0))?,
        meshes,
        fluid: Some(fluid),
        sun: Sun {
            direction: Vec3::new(-0.6, 0.65, -0.7),
            ..Default::default()
        },
        water: Some(Water {
            bounds: Some(WaterBounds::new(Vec2::ZERO, Vec2::new(3.82, 2.32))?),
            level: 1.55,
            amplitude: 0.025,
            style: WaterStyle::Realistic,
            ripple_strength: 2.0,
            roughness: 0.26,
            foam_strength: 0.0,
            ..Default::default()
        }),
        secondary_water: Some(Water {
            bounds: Some(WaterBounds::new(Vec2::new(0.0, 5.25), Vec2::new(2.22, 3.00))?),
            level: 0.16,
            amplitude: 0.018,
            time: 0.0,
            style: WaterStyle::Realistic,
            ripple_strength: 1.45,
            roughness: 0.31,
            foam_strength: 0.12,
            foam_width: 0.35,
            reflections: false,
            ..Default::default()
        }),
    };
    support::run(support::Demo::new("Water board", scene, None))
}
