use gigantomachia::fluid::{BoxCollider, FIXED_DT, Fluid};
use glam::Vec3;
fn box_(min: [f32; 3], max: [f32; 3]) -> BoxCollider {
    BoxCollider::new(Vec3::from(min), Vec3::from(max)).unwrap()
}
fn tank() -> Vec<BoxCollider> {
    vec![
        box_([-3.0, -0.3, -3.0], [3.0, 0.0, 3.0]),
        box_([-1.2, 0.0, -1.2], [-1.0, 2.0, 1.2]),
        box_([1.0, 0.0, -1.2], [1.2, 2.0, 1.2]),
        box_([-1.2, 0.0, -1.2], [1.2, 2.0, -1.0]),
        box_([-1.2, 0.0, 1.0], [1.2, 2.0, 1.2]),
    ]
}
#[test]
fn free_fall_follows_gravity_without_authored_path() {
    let mut fluid = Fluid::block(Vec3::new(0.0, 3.0, 0.0), [1, 1, 1], 0.16).unwrap();
    for _ in 0..30 {
        fluid.step(&[]);
    }
    let t = 30.0 * FIXED_DT;
    assert!((fluid.positions()[0].y - (3.0 - 0.5 * 9.81 * t * t)).abs() < 0.02);
    assert_eq!(fluid.positions()[0].x, 0.0);
    assert_eq!(fluid.particle_count(), 1);
}
#[test]
fn closed_container_retains_water_open_wall_releases_it_without_mass_loss() {
    let mut fluid = Fluid::block(Vec3::new(-0.8, 0.12, -0.8), [10, 5, 10], 0.16).unwrap();
    let colliders = tank();
    for _ in 0..120 {
        fluid.step(&colliders);
    }
    assert!(
        fluid
            .positions()
            .iter()
            .all(|p| p.is_finite() && p.x.abs() < 1.0 && p.z.abs() < 1.0 && p.y > 0.0)
    );
    let count = fluid.particle_count();
    let volume = fluid.volume();
    let mut open = colliders.clone();
    open.pop();
    for _ in 0..180 {
        fluid.step(&open);
    }
    let escaped = fluid.positions().iter().filter(|p| p.z > 1.1).count();
    assert!(
        escaped > 5,
        "opening the wall must release water: {escaped}"
    );
    assert_eq!(fluid.particle_count(), count);
    assert_eq!(fluid.volume(), volume);
}
#[test]
fn fluid_validates_input_and_reconstructs_current_positions() {
    assert!(Fluid::block(Vec3::ZERO, [100, 100, 100], 0.1).is_err());
    assert!(Fluid::block(Vec3::ZERO, [1, 1, 1], f32::NAN).is_err());
    let mut fluid = Fluid::block(Vec3::Y, [3, 3, 3], 0.16).unwrap();
    let a = fluid.surface().unwrap();
    assert!(!a.indices().is_empty());
    assert!(
        a.vertices()
            .iter()
            .all(|v| Vec3::from(v.position).is_finite() && Vec3::from(v.normal).length() > 0.9)
    );
    fluid.step(&[]);
    assert_ne!(a.vertices(), fluid.surface().unwrap().vertices());
}

#[test]
fn elevated_container_drains_down_to_floor_and_obstacle_blocks_particles() {
    let solids = vec![
        box_([-3.0, -0.3, -3.0], [3.0, 0.0, 5.0]),
        box_([-1.2, 1.3, -1.2], [1.2, 1.5, 1.2]),
        box_([-1.2, 1.5, -1.2], [-1.0, 3.0, 1.2]),
        box_([1.0, 1.5, -1.2], [1.2, 3.0, 1.2]),
        box_([-1.2, 1.5, -1.2], [1.2, 3.0, -1.0]),
        box_([-0.2, 0.0, 1.8], [0.2, 0.5, 2.3]),
    ];
    let gate = box_([-1.2, 1.5, 1.0], [1.2, 3.0, 1.2]);
    let mut closed = solids.clone();
    closed.push(gate);
    let mut fluid = Fluid::block(Vec3::new(-0.8, 1.6, -0.8), [10, 5, 10], 0.16).unwrap();
    for _ in 0..120 {
        fluid.step(&closed);
    }
    assert!(fluid.positions().iter().all(|p| p.y > 1.5));
    for _ in 0..300 {
        fluid.step(&solids);
    }
    let fallen = fluid.positions().iter().filter(|p| p.y < 1.0).count();
    assert!(
        fallen > 10,
        "gravity should carry fluid off the elevated board: {fallen}"
    );
    assert_eq!(fluid.particle_count(), 500);
    for &p in fluid.positions() {
        assert!(p.is_finite());
        assert!(p.y > 0.0, "the receiving floor must stop the fall");
        for c in &solids {
            assert!(
                !(p.cmpgt(c.min()).all() && p.cmplt(c.max()).all()),
                "particle inside solid: {p:?}"
            );
        }
    }
}

#[test]
fn repeated_fixed_steps_are_deterministic() {
    let mut a = Fluid::block(Vec3::new(-0.3, 0.5, -0.3), [4, 4, 4], 0.16).unwrap();
    let mut b = a.clone();
    for _ in 0..40 {
        a.step(&tank());
        b.step(&tank());
    }
    assert_eq!(a.positions(), b.positions());
    assert_eq!(a.velocities(), b.velocities());
}
