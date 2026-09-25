# Engine Design

Gigantomachia is a small, code-first 3D engine targeting Linux, Rust, and wgpu's Vulkan backend. It currently supports a perspective camera, vertex-colored opaque meshes, a procedural sky, and an optional water surface with refraction, absorption, and shoreline foam. Directional shadows affect land and water. Water supports selectable stylized and realistic surface shading; the latter uses a lazily allocated denser grid, analytic per-fragment normals, filtered ripples, and GGX sunlight. ASCII/binary FBX import produces the same opaque meshes; rigid node animation updates their instance transforms. There is no editor, ECS, physics system, or render graph yet.

## Application, Scene, and Renderer

```text
examples/water.rs, examples/island.rs, or examples/fbx.rs
    builds Scene + implements Application through example support
                         |
                  app::run(application, config)
                  window lifecycle + Input + update
                         |
                  Renderer::render(gpu, target, scene)
                  shadow map → opaque HDR color/depth
                         |
                  refractive water → tone mapping → submission
```

| Layer | Owns | Does not own |
| --- | --- | --- |
| `Application` implementation | Scene construction, controls, animation, reset behavior, window title | GPU buffers, pipelines, surfaces |
| `app` / `input` | Event loop, window/surface lifecycle, elapsed time, input state | Wave playback policy, island generation, camera key bindings |
| `scene` / `mesh` / `water` | Camera, immutable shared geometry, per-instance transforms, sunlight, water parameters | Window or GPU handles |
| `render` | Device setup, frame uniforms, shadow/HDR/depth targets, pipelines, mesh cache, submission, readback | Keyboard handling, demo state, procedural terrain algorithms |
| `asset` | FBX parsing, coordinate conversion, CPU meshes, rigid animation sampling, import diagnostics | GPU uploads, texture decoding, playback clocks, skeletal deformation |
| `terrain` | Seeded island heightfield and CPU mesh generation | Special terrain rendering code |
| `examples/support` | Shared demo navigation, CLI, PNG encoding | Engine render passes |

The modules remain in one library crate. Separate crates, a general-purpose material system, and an ECS can be introduced when independent consumers require them. Engine source never imports example code.

## Public API

A minimal application can use the renderer without touching wgpu:

```rust
use gigantomachia::{
    app::{self, AppAction, AppConfig, Application},
    input::{Input, KeyCode},
    render::EngineResult,
    scene::Scene,
    water::Water,
};

struct Ocean { scene: Scene }

impl Application for Ocean {
    fn scene(&self) -> &Scene { &self.scene }

    fn update(&mut self, input: &Input, dt: f32) -> AppAction {
        if input.pressed(KeyCode::Escape) { return AppAction::Exit; }
        if let Some(water) = &mut self.scene.water { water.time += dt; }
        AppAction::Continue
    }
}

fn main() -> EngineResult<()> {
    let scene = Scene { water: Some(Water::default()), ..Default::default() };
    app::run(Ocean { scene }, AppConfig::default())
}
```

For a mesh, construct `Mesh::new(vertices, indices)`, share it with `Arc<Mesh>`, and add `MeshInstance::new(mesh)` to `Scene::meshes`. `Island::mesh()` returns this same mesh type. `asset::load_fbx()` returns mesh instances ready to extend `Scene::meshes`; see the [FBX guide](fbx.md). There is no island or FBX branch in the renderer. The island example demonstrates terrain setup and an explicit camera target.

`MeshInstance::set_transform` accepts finite, invertible affine transforms with a positive determinant. Normal matrices use inverse transpose, including nonuniform scale. FBX import bakes reflected source transforms and corrects winding before creating instances. Reflected runtime transforms are rejected because the current opaque pipeline uses one CCW/back-face-culling configuration.

For tools or tests, create `Gpu::headless()`, `OffscreenTarget`, and `Renderer`; render a `Scene` into the target view and call `read_rgba8()`. The target format and dimensions must match the renderer. Readback blocks until GPU work completes and is intended for captures/tests, not each interactive frame. PNG is a development dependency used only by examples.

Rigid FBX clips are sampled through `AnimatedFbx` using an application-owned clock. Samples share rest geometry and update instance transforms; see the [animation guide](animation.md).

## Resource Ownership and Lifetime

- CPU meshes are immutable and have stable IDs. Cloning a `Scene` clones mesh `Arc`s, not vertex arrays.
- The renderer uploads each distinct mesh once while it remains referenced by the current scene. Multiple instances reuse the same GPU geometry and have separate transform uniforms.
- Mesh assets absent from the next rendered scene are evicted; instance uniform slots are resized to the current instance count. Removing and later re-adding a mesh uploads it again.
- Frame resources belong to `Renderer`: a 2048² shadow depth map, opaque HDR color/depth, and separate composite HDR color/water depth. The water depth attachment receives a copy of opaque depth, while the original stays read-only for refraction/foam. Size-dependent targets and all their sampling bindings are recreated together on resize. Passes encode draw calls; only the renderer submits the frame command buffer.
- Suspend drops window/GPU resources but preserves application-owned CPU scene data. Resume recreates resources and uploads geometry as needed.

This is a small forward renderer: no GPU instancing, visibility culling, batching, asynchronous asset streaming, or multi-scene cache is implemented yet.

## Lifecycle and Timing

`Input events → Application::update → Renderer::render → present`

- Updates receive elapsed time capped at 100 ms. The host has no game simulation or wave clock. The examples accumulate `Water::time` using their own speed/pause state.
- Held keys persist; press transitions and accumulated mouse motion are consumed once per update. Quick press/release taps survive until that update. Focus loss clears input.
- Camera movement is reusable and unconstrained. Sea-level/terrain clearance is a demo policy in `examples/support`.
- Zero-sized or occluded windows stop rendering. Resize reconfigures the surface and all screen-sized HDR/depth storage. Lost/outdated surfaces are reconfigured, timeouts are retried, and other surface errors exit with a message.
- Headless rendering accepts explicit scene time. A 60 Hz fixed game update is still future work, not part of this refactor.

## 3D Conventions

Right-handed coordinates, Y-up, local camera forward -Z, meters, and column-vector matrices. Projection depth is 0..1, the depth comparison is `Less`, and depth clears to 1.0. Mesh triangles use CCW winding. Vertex colors and lighting are linear RGB; output targets are sRGB. One camera, one directional sun, opaque meshes, and one water surface are supported per frame. Scene passes produce linear `Rgba16Float` HDR color; a final pass applies exposure compression exactly once before sRGB encoding.

## Validation and Next Steps

CPU tests cover camera projection/movement, input transitions, mesh validation, grid winding, deterministic terrain, and directional shadow projection. Opt-in Vulkan integration tests use only the public engine API to check wave animation, zero-amplitude stability, resize/readback alignment, island visibility, shared geometry, transform updates, cache eviction, optional water, depth occlusion with effects disabled, cast shadows on land/water, refraction/absorption, shallow-only foam, and resize equivalence to fresh render targets.

Screen-space refraction, depth absorption, and shoreline foam are implemented; see [water rendering](water.md) for equations and limitations. Underwater views, scene-object reflections, shadow cascades, and ocean LOD remain future rendering work. Reusable material/asset handles, ECS, and static glTF loading are also still pending.
