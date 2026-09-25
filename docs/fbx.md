# Static FBX Import

`asset::load_fbx(path)` and `asset::load_fbx_bytes(bytes)` load ASCII and binary FBX through [ufbx](https://github.com/ufbx/ufbx). The parser is bundled through its Rust crate; no Autodesk SDK is needed. The Nix shell provides the C compiler required to build it.

```rust
use gigantomachia::{asset::load_fbx, render::EngineResult, scene::Scene};

fn load_scene() -> EngineResult<Scene> {
    let model = load_fbx("assets/island.fbx")?;
    for warning in &model.warnings {
        eprintln!("FBX: {warning}");
    }
    let mut scene = Scene::default();
    scene.meshes.extend(model.meshes);
    // Set scene.camera to frame your model; the loader does not move the camera.
    Ok(scene)
}
```

`FbxModel` exposes mesh instances, corresponding source node names, world-space bounds, and warnings. Import is synchronous and CPU-only. Meshes use the existing opaque and shadow passes and can be added to scenes containing water.

## Geometry and Appearance

- Visible polygon nodes are imported, respecting ancestor visibility. Empty nodes are skipped; a file without visible polygon meshes returns an error.
- Source axis and unit metadata are converted to right-handed, Y-up meters. Handedness conversion mirrors X so it preserves the up direction.
- Parent and geometric transforms are baked into vertices. Resulting instances start at identity; source hierarchy and instancing are flattened into separate meshes.
- Polygons are triangulated. Corner normals and color seams are retained, missing normals are generated, and inverse-transpose normal transforms handle nonuniform scale. Mirrored node transforms have their triangle winding corrected.
- The first vertex color set multiplies the material base/diffuse color and factor. Material assignments are resolved per polygon and node. Missing colors use gray. RGB values are treated as linear and clamped to 0–1, matching the current renderer.

The transform and corner-index handling follow the ufbx [node](https://ufbx.github.io/elements/nodes/) and [mesh](https://ufbx.github.io/elements/meshes/) APIs.

## Current Limits

This is static geometry support, not full FBX scene playback. Skinning, blend shapes, geometry caches, polygon holes, singular transforms, and invalid vertex attributes return errors. Bake deformation and triangulate polygon holes before export.

Textures (including embedded images) are not loaded, and UVs are not retained. All imported geometry is opaque: transparency, normal maps, metallic/roughness, and other material properties are not implemented. Animation tracks are not evaluated; authored static transforms are used. FBX cameras, lights, and non-polygon geometry are ignored. Warnings report detected textures, UVs, animation, cameras/lights, curves, and vertex alpha; they are not an exhaustive material-feature report. Referenced external files are never opened.

## Viewer

```sh
nix develop path:.
# Bundled fixture: two colored meshes, one with mirrored/nonuniform scale.
cargo run --example fbx
cargo run --release --example fbx -- path/to/model.fbx
cargo run --example fbx -- path/to/model.fbx --headless /tmp/model.png
cargo run --example fbx -- path/to/model.fbx --frames 10
```

The viewer scales the longest model dimension to eight meters, centers it above a gray floor, and positions the camera. This placement belongs to the example; the loader preserves authored dimensions. Extremely large models may exceed the renderer's supported transform scale. WASD/QE, right mouse drag, Shift, R, Esc, and the shadow toggle (1) use the shared demo controls. Navigation has no terrain clearance in this viewer.

## Verification

`cargo test --test fbx` checks equivalent ASCII and binary 7400/7500 imports, hierarchy/geometric transforms, mirrored winding, normals, material/vertex colors, hidden nodes, axis/unit conversion, concave triangulation, and error cases. `cargo test --test rendering -- --ignored` includes an actual Vulkan render of an imported binary fixture. Fixtures are original, generated assets; see [fixture notes](../tests/fixtures/README.md). Broader exporter compatibility should be checked with production assets as they become available.
