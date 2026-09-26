# Finite 3D Fluid Objects

`fluid::GpuFluid` keeps finite equal-mass particles, velocities, neighbor search, and reconstructed surface geometry on the GPU. The CPU reference implementation remains available as `fluid::Fluid`. Gravity, density constraints, and collision geometry determine their motion. It is separate from `terrain::OceanSurface`, which is an authored, effectively unlimited water surface used by the ocean and island demos.

The `water_board` example uses **no ocean surface and no scripted waterfall**. It starts with 1,350 particles inside an elevated container. After one simulated second the front gate disappears, allowing gravity and pressure to move water over the edge. The same particles fall, hit the floor, and spread around a solid obstacle. There is no prebuilt stream, receiving puddle, emitter, teleportation, or particle recycling.

```sh
nix develop path:.
# GPU simulation and GPU surface reconstruction are the default.
cargo run --release --example water_board
# Optional CPU reference path for comparison.
cargo run --release --example water_board -- --cpu
# Keep the gate closed until O is pressed.
cargo run --release --example water_board -- --closed
# Compare the same quantity of water at the same simulated time.
cargo run --release --example water_board -- --closed --headless /tmp/fluid-closed.png 3
cargo run --release --example water_board -- --headless /tmp/fluid-open.png 3
```

Space pauses physics. O opens/closes the gate. R restores the initial fluid and container. `[` / `]` change playback speed, and WASD/QE plus right mouse drag move the camera. Closing the gate into existing water projects intersecting particles outside the new collider; it is not a continuously moving rigid gate. Headless captures simulate fixed steps up to the requested time (0–60 seconds), reconstructing every two steps to approximate a 60 Hz workload. Console output reports completed simulation/reconstruction wall time, particle count, nominal volume, and particles below the board. GPU captures also report surface overflow and particles outside the reconstruction domain. This timing excludes startup, final drawing, and PNG encoding; it is not interactive FPS.

The old authored effect remains available as `cargo run --release --example waterfall_visual`. It is explicitly a visual-effect example, not the fluid implementation.

## Engine Boundaries

- `Scene::ocean: Option<terrain::OceanSurface>` holds authored surface rendering. The former `Scene::water` field has been renamed. `water::Water` remains the underlying compatible parameter type, also exported as `terrain::OceanSurface`.
- `Scene::gpu_fluids` holds shared `Arc<GpuFluid>` handles created on the same device as the renderer. Cloning the scene shares simulation state; it does not duplicate particles.
- `fluid::Fluid` is the independently owned CPU reference simulation object. Application code controls fixed steps, colliders, resets, and lifetime.
- `Fluid::surface()` returns a `FluidSurface` for `Scene::fluids`. The renderer only consumes its geometry; it never advances physics.
- `BoxCollider` represents a solid, axis-aligned volume. Visible meshes do not automatically become colliders. The demo creates matching geometry and colliders from the same bounds.

```rust
use gigantomachia::{fluid::{Fluid, BoxCollider}, scene::Scene};
use glam::Vec3;

let mut fluid = Fluid::block(Vec3::new(-0.4, 1.0, -0.4), [6, 5, 6], 0.16)?;
let floor = BoxCollider::new(Vec3::new(-4.0, -0.2, -4.0), Vec3::new(4.0, 0.0, 4.0))?;
fluid.step(&[floor]); // Exactly 1/120 simulated second.
let scene = Scene { fluids: vec![fluid.surface()?], ..Scene::default() };
```

A block accepts 1–8,192 particles and spacing 0.03–1 meter. Its initial coordinates must be within 10 km of the origin. `volume()` reports constant nominal particle volume, not an exact measurement of the reconstructed surface. Particle positions and velocities are exposed read-only. No public mutation can inject invalid particle states.

## GPU Path

The application uses `Application::prepare_render(&Gpu)` after input/update to initialize resources and enqueue simulation. `GpuFluid::step(gpu, colliders, steps)` accepts up to eight fixed steps and 64 colliders per batch. Collider snapshots are uploaded before each batch submission, preserving changes such as opening a gate between steps. Call `reconstruct(gpu)` once after the frame's steps, then render the same handle through `Scene::gpu_fluids`. Paused frames reuse geometry without simulation or reconstruction.

```rust
use gigantomachia::fluid::GpuFluid;
use std::sync::Arc;

let simulated = Arc::new(GpuFluid::new(
    &gpu, &fluid, Vec3::new(-2.5, -0.4, -1.9), Vec3::new(2.5, 3.0, 4.5),
)?);
simulated.step(&gpu, &[floor], 2)?;
simulated.reconstruct(&gpu);
let scene = Scene { gpu_fluids: vec![simulated], ..Scene::default() };
```

Compute passes implement gravity/prediction, a spatial hash, five density-constraint iterations, collision projection, and velocity smoothing. The hash uses atomic linked lists without a fixed per-cell particle limit. Queries check full cell coordinates to prevent hash collisions from double-counting neighbors. Separate dispatches separate reads from writes to neighboring state. Floating-point accumulation order varies with GPU scheduling, so CPU/GPU trajectories are close initially but are not bitwise deterministic over long runs.

Surface density sampling and marching tetrahedra also run on the GPU. Generated vertices and indirect draw arguments are consumed directly by the existing fluid shading pass. There is no per-frame particle/mesh download, CPU surface reconstruction, or CPU wait for simulation completion. `GpuFluid::readback` is an explicitly blocking diagnostic tool for tests and captures, not part of the interactive path. GPU work still shares rendering resources and can limit frame rate under heavy loads.

The reconstruction domain is explicitly bounded to at most 600,000 samples; it does not confine the physics. Keep it large enough to cover the particles plus their 1.8-spacing surface support. Particles leaving this domain remain in the simulation but their surface is clipped; readback reports this condition. Each fluid reserves 600,000 vertices (about 21.6 MB) and additional grid/particle buffers. Overflow clamps the indirect draw safely and sets a diagnostic flag; it does not delete particles. The default board's receiving basin keeps the demonstrated flow inside its domain.

GPU resources belong to their originating device. On suspend, `Application::release_gpu` clears the board's handles and pending work; resume recreates the simulation from its CPU seed. R likewise replaces the handle to reset. The CPU path remains useful for tools, deterministic checks, and performance comparisons; choose it explicitly with `--cpu`.

## Solver

Both backends implement a small position-based fluid prototype inspired by [Macklin and Müller's Position Based Fluids](https://mmacklin.com/pbf_sig_preprint.pdf). Each fixed step predicts positions under gravity, solves neighbor density constraints, projects against solid boxes, reconstructs velocity, and blends neighboring velocities for viscosity.

A spatial hash finds neighbors within twice the particle spacing. Five Jacobi iterations use a compact polynomial density kernel and its analytic gradient. Rest density is calibrated to the initial cubic lattice. Constraints only resist excess density: they do not attract isolated particles to repair free-surface density deficits. Per-iteration displacement and speed limits prevent very large corrections. A swept point test against radius-expanded boxes limits tunneling through thin walls.

The application accumulator caps catch-up at eight steps per rendered frame. Under load, simulated time falls behind real time rather than increasing the physics timestep. There is no promise of real-time performance at the particle limit.

## Surface and Rendering

The renderer does not display falling-water sprites. The CPU `Fluid::surface()` and GPU reconstruction splat the current particle positions into a scalar density field, then extract an isosurface with marching tetrahedra. Analytic field gradients provide smooth normals. Separated droplets, connected streams, and puddles result from particle distribution. Surface reconstruction is visual; it does not feed back into physics.

Grid spacing is 0.65 times particle spacing. CPU reconstruction rejects domains above 600,000 grid samples or surfaces above two million vertices. A widely dispersed simulation may therefore continue physically but exceed this prototype's display budget; the demo reports the error and stops rather than silently dropping particles.

`Scene::fluids` uses its own GPU pass with reusable vertex/index buffers. Geometry is uploaded on each rendered frame; buffers grow only when capacity is insufficient. Shading includes procedural-sky Fresnel reflection, scene shadows, approximate background transmission, and normal-based screen distortion. Fluids write their own depth. They are not rendered as the existing horizontal ocean surface.

## Current Limits

This is a first small-scale solver, not a general CFD package. Compression is approximate, boundaries have no density ghost particles, and the reconstructed surface may look lumpy or lose thin connections at this resolution. Nominal mass is conserved by retaining particles; exact incompressibility and geometric volume conservation are not guaranteed. The solver has no surface tension, air phase, rigid-body coupling, mesh colliders, or turbulence model. GPU acceleration improves execution speed, not the underlying physical model or surface resolution.

The visual fluid surface has no physically measured thickness, inter-fluid refraction, planar object reflections, or fluid shadow casting yet. Its transmission samples the opaque scene and uses a fixed tint. An ocean and a fluid can coexist visually but do not exchange mass or forces. Terrain heightfields require explicitly supplied collision geometry to interact with fluids.

## Verification

CPU tests cover free fall, closed-container retention, release through an opened wall, falling from an elevated container, floor/obstacle exclusion, deterministic fixed stepping, constant particle count and nominal volume, input validation, and surface generation. A Vulkan test renders a changing reconstructed surface without an ocean, checks deterministic unchanged frames, resize equivalence, and removal without stale pixels. Existing ocean, refraction, reflection, foam, FBX, and animation regressions remain in the suite.

GPU regressions compare free-fall positions against the CPU reference, retain water in a closed container, release it after a collider change, check floor/obstacle exclusion, report domain escape, render generated geometry directly, and verify pause, reset, resize, and removal. Run them with:

```sh
cargo test --release --test gpu_fluid -- --ignored --test-threads=1
```

A release-build measurement on an AMD Radeon RX 9060 XT (RADV, Vulkan), using the default 1,350-particle board for three simulated seconds with reconstruction every two steps, took 0.774 s on the GPU path versus 5.381 s on the CPU path (about 7× faster). These are one-run simulation/reconstruction wall times, including final diagnostic synchronization, not rendered FPS or a guarantee for other hardware.
