# 3D Engine Design Draft

## Goals

This is a library for building small, personal-scale 3D games entirely in Rust code.

The main ideas borrowed from Bevy are the separation of data and systems and a code-first execution model. The project does not aim to recreate a general-purpose plugin framework or parallel scheduler from the start.

The initial target is Linux desktop with a single window, a single GPU, and forward rendering. The first concrete goal is an interactive water demo, before ECS or general-purpose scene construction.

The first version will not include an editor, visual scripting, networking, physics, audio, animation, terrain, ray tracing, or a render graph.

## Module Boundaries

The project will initially be organized into modules within a single library. Modules can be split into separate crates later if necessary.

| Module | Responsibility | Introduced |
| --- | --- | --- |
| `app` | winit lifecycle, surface, controls, demo clock | Implemented |
| `camera` | Fly movement and perspective matrices | Implemented |
| `render` | Shared Vulkan adapter/device initialization | Implemented |
| `water` | Grid, uniforms, sky/water pipelines, depth | Implemented |
| `scene` | Transform, Camera, MeshRenderer, ECS world | Future |
| `input` | Standalone game input abstraction | Future; demo controls currently live in `app` |
| `asset` | Handles, loading, GPU uploads | Future |

In the future scene layer, game logic will modify ECS components, while the renderer will extract transforms and asset handles for drawing. The water demo directly supplies its camera and water settings to the renderer.

GPU resources are owned by renderer-side storage, and entities contain only handles to those resources.

Systems will initially run on a single thread in an explicit order.

## Execution Flow

The water demo uses `input events → camera/clock update → sky pass → water pass → present`.

- Camera motion uses elapsed time capped at 100 ms. The wave clock accumulates elapsed time multiplied by speed; changing speed does not jump the wave phase.
- Pausing freezes only wave time. Camera motion and parameter controls remain active.
- No gameplay simulation exists yet. A 60 Hz fixed-step accumulator is reserved for a future scene/physics layer.
- Headless rendering accepts an explicit time, bypassing the event loop for reproducible captures.
- Rendering stops for zero-sized or occluded windows. Resize reconfigures the surface and recreates depth storage.
- Lost/outdated surfaces are reconfigured, timeouts are retried on a later frame, and other surface errors are reported before exit.
- Focus loss clears held input, and suspend drops the window/surface resources for recreation on resume.

## 3D Conventions

- The engine uses a right-handed coordinate system with Y-up. The camera looks along local -Z, and distances are measured in meters.
- Rotations use quaternions, and transforms are composed as `translation * rotation * scale`.
- Projection depth uses wgpu's 0..1 range. Depth comparison is `Less`, with a clear value of 1.0.
- Mesh front faces use counter-clockwise winding, with back-face culling enabled by default.
- Lighting calculations are performed in linear color space, while color textures and output use sRGB formats.
- Initially, only a single perspective camera and opaque meshes are supported.

## Milestone Completion Criteria

1. **Water Surface (implemented)**: Open a window with a perspective camera and depth buffer. Render a Gerstner-displaced grid with animated normals, procedural sky reflections, Fresnel reflectance, and sunlight highlights. Support camera navigation, pause/reset, amplitude/speed controls, resize, and deterministic headless PNG capture. Validate actual GPU rendering, animation, a zero-amplitude surface, and resized targets.

2. **Water in a Scene**: Add a simple submerged object or floor, sample scene color/depth for refraction and absorption, and use water depth for shoreline foam. Define above/below-water behavior before allowing the camera to submerge.

3. **Scene Construction in Code**: Introduce a small ECS, reusable mesh/material handles, and a fixed-step game update. Keep the water renderer reusable as a scene element.

4. **Real Assets and Measurement**: Load static glTF/GLB meshes and color textures. Measure CPU/GPU frame time, memory, and distribution size with a small scene before adding animation, PBR, or a render graph.

Each milestone remains runnable as an example. Camera projection/movement and mesh winding have CPU tests. The opt-in Vulkan test renders frames, checks visible animation, verifies zero-amplitude stability, and exercises resize/readback alignment under wgpu validation. Visual quality is also checked with a rendered PNG.

See [water rendering notes](water.md) for the current equations and limitations.
