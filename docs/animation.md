# Rigid FBX Animation

The FBX viewer can play an animated cube that moves sideways, bounces, rotates, and stretches over a two-second loop. Motion comes from FBX keyframes, including a parent-node translation track.

```sh
nix develop path:.
cargo run --example fbx -- --animate
cargo run --example fbx -- path/to/animated-model.fbx --animate
cargo run --example fbx -- --animate --headless /tmp/cube-start.png 0
cargo run --example fbx -- --animate --headless /tmp/cube-middle.png 1
```

Space pauses/resumes playback, `[` / `]` change speed between 0 and 3, and R resets the camera and animation. Navigation remains available while paused. The viewer plays the first clip and displays its name, elapsed playback seconds, and speed in the title. The animated viewer fits the rest pose to two meters to leave room for motion; arbitrary clips may travel outside the camera view.

## Engine API

```rust
use gigantomachia::{asset::AnimatedFbx, render::EngineResult, scene::Scene};
use glam::Mat4;

fn example() -> EngineResult<()> {
    let asset = AnimatedFbx::load("tests/fixtures/animated_cube_ascii.fbx")?;
    let mut scene = Scene::default();
    // A real application retains asset and advances its own playback clock.
    scene.meshes = asset.sample(0, 0.5, true, Mat4::IDENTITY)?;
    Ok(())
}
```

`AnimatedFbx::from_bytes()` also supports in-memory loading. `clips()` exposes names, authored start times, and durations. `sample(clip_index, seconds, looping, placement)` returns instances in import order; time is relative to the selected clip's start. Looping wraps at the duration, including negative times. Non-looping playback clamps to the endpoints. Zero-duration clips sample their start pose. Non-finite times and invalid clip indices return errors.

`model()` exposes the immutable rest model, names, warnings, and rest bounds. Bounds do not describe the full motion. Applications insert returned instances into the appropriate portion of their scene, preserving unrelated objects. Sampling does not mutate the asset, so independent playback clocks can share one loaded asset.

The importer retains the parsed FBX and uses [ufbx scene evaluation](https://ufbx.github.io/elements/animation/) to resolve animated hierarchy and transforms. Each instance receives `placement * animated_geometry_to_world * inverse(rest_geometry_to_world)`. Rest vertices already contain the source transforms. Mesh `Arc`s remain shared between samples, so the renderer reuses vertex/index buffers and updates instance uniforms for both opaque and shadow passes.

## Scope and Limits

This milestone supports rigid node translation, rotation, and scale. Skinning, blend shapes, geometry caches, animated visibility, animated materials, textures, and clip blending are not implemented. Visibility and material colors are fixed at import. The existing static loader still returns the authored static pose and warns about ignored animation tracks.

The renderer requires finite, invertible transforms that preserve winding. A clip that passes through zero scale or changes handedness returns a sampling error; the viewer reports it and stops. Static source reflections already baked by the importer remain supported when animation preserves their handedness.

Full-scene evaluation allocates CPU data on each sample. It is appropriate for this small initial example, but baked tracks and a lighter runtime evaluator should precede large animated scenes. No skeletal animation or performance claims are implied by this demo.

## Validation

CPU tests check parent motion, geometric offsets, linear interpolation, rotation, scale, nonzero clip start, looping/clamping, placement, shared geometry, and invalid inputs. A Vulkan test checks changing pixels and shadows, stable GPU mesh residency, and an identical frame at the loop boundary. The original synthetic fixture is reproducible with `python3 tests/fixtures/generate_fbx.py`.
