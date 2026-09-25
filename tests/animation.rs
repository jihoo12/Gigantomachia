use gigantomachia::asset::AnimatedFbx;
use glam::{Mat4, Vec3};
use std::sync::Arc;

const CUBE: &[u8] = include_bytes!("fixtures/animated_cube_ascii.fbx");

#[test]
fn animated_parent_translation_rotation_and_scale_follow_fbx_keys() {
    let asset = AnimatedFbx::from_bytes(CUBE).unwrap();
    assert_eq!(asset.clips()[0].name, "Bounce");
    assert_eq!(asset.clips()[0].start, 1.0);
    assert_eq!(asset.clips()[0].duration, 2.0);
    let start = asset.sample(0, 0.0, false, Mat4::IDENTITY).unwrap();
    let middle = asset.sample(0, 1.0, false, Mat4::IDENTITY).unwrap();
    // Authored rest vertex (1, 0.5, 1) includes the geometric Y offset.
    let p = Vec3::new(1.0, 0.5, 1.0);
    let a = start[0].transform().transform_point3(p);
    let b = middle[0].transform().transform_point3(p);
    assert!((a - Vec3::new(-1.0, 0.5, 1.0)).length() < 1e-5, "{a:?}");
    assert!((b - Vec3::new(1.0, 2.75, -1.0)).length() < 1e-5, "{b:?}");
    assert!(Arc::ptr_eq(&start[0].mesh, &middle[0].mesh));
    let quarter = asset.sample(0, 0.5, false, Mat4::IDENTITY).unwrap();
    let c = quarter[0].transform().transform_point3(p);
    assert!((c - Vec3::new(1.0, 1.625, -1.0)).length() < 1e-5, "{c:?}");
}

#[test]
fn looping_clamping_placement_and_invalid_input() {
    let asset = AnimatedFbx::from_bytes(CUBE).unwrap();
    let sample = |t, looping| asset.sample(0, t, looping, Mat4::IDENTITY).unwrap()[0].transform();
    assert_eq!(sample(0.0, true), sample(2.0, true));
    assert_eq!(sample(0.5, true), sample(2.5, true));
    assert_eq!(sample(-0.5, true), sample(1.5, true));
    assert_eq!(sample(-10.0, false), sample(0.0, false));
    assert_eq!(sample(10.0, false), sample(2.0, false));
    let placement = Mat4::from_translation(Vec3::new(10.0, 0.0, 4.0));
    assert_eq!(
        asset.sample(0, 0.5, false, placement).unwrap()[0].transform(),
        placement * sample(0.5, false)
    );
    assert!(asset.sample(1, 0.0, false, Mat4::IDENTITY).is_err());
    assert!(asset.sample(0, f64::NAN, false, Mat4::IDENTITY).is_err());
    assert!(asset.sample(0, 0.0, false, Mat4::ZERO).is_err());
    assert!(AnimatedFbx::from_bytes(include_bytes!("fixtures/static_scene_ascii.fbx")).is_err());
}
