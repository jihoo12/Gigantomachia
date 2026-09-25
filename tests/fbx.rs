//! Self-contained static FBX importer regressions, without a GPU or third-party model files.

use gigantomachia::asset::{load_fbx, load_fbx_bytes};
use glam::{Mat4, Vec3};

const ASCII: &[u8] = include_bytes!("fixtures/static_scene_ascii.fbx");
const BINARY: &[u8] = include_bytes!("fixtures/static_scene_binary.fbx");
const BINARY_7500: &[u8] = include_bytes!("fixtures/static_scene_binary_7500.fbx");

#[test]
fn ascii_binary_and_64_bit_records_import_the_same_static_scene() {
    let ascii = load_fbx_bytes(ASCII).unwrap();
    assert_eq!(ascii.names, ["LeftTower", "MirrorTower"]);
    assert!(
        (ascii.bounds_min - Vec3::new(-3.0, 0.5, -1.0)).length() < 1e-5,
        "{:?}",
        ascii.bounds_min
    );
    assert!(
        (ascii.bounds_max - Vec3::new(4.0, 4.5, 1.0)).length() < 1e-5,
        "{:?}",
        ascii.bounds_max
    );
    for bytes in [BINARY, BINARY_7500] {
        let other = load_fbx_bytes(bytes).unwrap();
        assert_eq!(ascii.names, other.names);
        assert_eq!(ascii.bounds_min, other.bounds_min);
        assert_eq!(ascii.bounds_max, other.bounds_max);
        for (a, b) in ascii.meshes.iter().zip(&other.meshes) {
            assert_eq!(a.mesh.vertices(), b.mesh.vertices());
            assert_eq!(a.mesh.indices(), b.mesh.indices());
        }
    }
}

#[test]
fn hierarchy_geometry_transform_mirrors_normals_and_colors_survive_conversion() {
    let model = load_fbx_bytes(ASCII).unwrap();
    for instance in &model.meshes {
        assert_eq!(instance.transform(), Mat4::IDENTITY);
        assert_eq!(instance.mesh.indices().len(), 36);
        for vertex in instance.mesh.vertices() {
            assert!((Vec3::from_array(vertex.normal).length() - 1.0).abs() < 1e-5);
        }
        for triangle in instance.mesh.indices().as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| instance.mesh.vertices()[i as usize]);
            let geometric = (Vec3::from_array(b.position) - Vec3::from_array(a.position))
                .cross(Vec3::from_array(c.position) - Vec3::from_array(a.position))
                .normalize();
            assert!(
                geometric.dot(Vec3::from_array(a.normal)) > 0.1,
                "mirroring must preserve front faces and outward normals"
            );
        }
        assert!(
            instance
                .mesh
                .vertices()
                .iter()
                .any(|v| (v.color[0] - 0.4).abs() < 1e-5 && (v.color[1] - 0.15).abs() < 1e-5),
            "material color must multiply vertex color"
        );
        assert!(
            instance
                .mesh
                .vertices()
                .iter()
                .any(|v| (v.color[2] - 0.8).abs() < 1e-5)
        );
    }
    assert!(model.warnings.iter().any(|w| w.contains("Textures")));
    assert!(model.warnings.iter().any(|w| w.contains("UV")));
}

#[test]
fn z_up_centimeters_become_y_up_meters() {
    let model = load_fbx_bytes(include_bytes!("fixtures/z_up_ascii.fbx")).unwrap();
    assert!((model.bounds_max.y - 1.0).abs() < 1e-5);
    assert!((model.bounds_max.x - 1.0).abs() < 1e-5);
    assert!(model.bounds_min.z.abs() < 1e-5 && model.bounds_max.z.abs() < 1e-5);
}

#[test]
fn concave_polygon_triangulation_preserves_area() {
    let model = load_fbx_bytes(include_bytes!("fixtures/concave_ascii.fbx")).unwrap();
    let mesh = &model.meshes[0].mesh;
    assert_eq!(mesh.indices().len(), 12);
    let area: f32 = mesh
        .indices()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|triangle| {
            let [a, b, c] =
                triangle.map(|i| Vec3::from_array(mesh.vertices()[i as usize].position));
            (b - a).cross(c - a).length() * 0.5
        })
        .sum();
    assert!((area - 3.0).abs() < 1e-5);
}

#[test]
fn corrupt_non_fbx_and_missing_files_return_useful_errors() {
    for bytes in [
        b"not an FBX".as_slice(),
        &BINARY[..40],
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3",
    ] {
        assert!(load_fbx_bytes(bytes).is_err());
    }
    let error = load_fbx("tests/fixtures/this-model-does-not-exist.fbx")
        .unwrap_err()
        .to_string();
    assert!(error.contains("this-model-does-not-exist.fbx"));
}

#[test]
fn singular_transforms_are_reported_with_the_node_name() {
    let source = String::from_utf8(ASCII.to_vec()).unwrap().replace(
        "\"Lcl Scaling\", \"Lcl Scaling\", \"\", \"A\", -1.0, 2.0, 0.5",
        "\"Lcl Scaling\", \"Lcl Scaling\", \"\", \"A\", 0.0, 2.0, 0.5",
    );
    let error = load_fbx_bytes(source.as_bytes()).unwrap_err().to_string();
    assert!(
        error.contains("MirrorTower") && error.contains("singular"),
        "{error}"
    );
}

#[test]
fn left_handed_conversion_preserves_up_and_front_faces() {
    let source = String::from_utf8(ASCII.to_vec()).unwrap().replace(
        "\"FrontAxisSign\", \"int\", \"\", \"A\", 1",
        "\"FrontAxisSign\", \"int\", \"\", \"A\", -1",
    );
    let model = load_fbx_bytes(source.as_bytes()).unwrap();
    assert!((model.bounds_min.y - 0.5).abs() < 1e-5);
    assert!((model.bounds_max.y - 4.5).abs() < 1e-5);
    for instance in model.meshes {
        for triangle in instance.mesh.indices().as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| instance.mesh.vertices()[i as usize]);
            let cross = (Vec3::from_array(b.position) - Vec3::from_array(a.position))
                .cross(Vec3::from_array(c.position) - Vec3::from_array(a.position));
            assert!(cross.dot(Vec3::from_array(a.normal)) > 0.0);
        }
    }
}
