# Finite 3D Fluid Objects

`fluid::Fluid` owns a finite set of equal-mass particles, their velocities, and a simulation clock. Gravity, density constraints, and collision geometry determine their motion. It is separate from `terrain::OceanSurface`, which is an authored, effectively unlimited water surface used by the ocean and island demos.

The new `water_board` example uses **no ocean surface and no scripted waterfall**. It starts with 1,350 particles inside an elevated container. After one simulated second the front gate disappears, allowing gravity and pressure to move water over the edge. The same particles fall, hit the floor, and spread around a solid obstacle. There is no prebuilt stream, receiving puddle, emitter, teleportation, or particle recycling.

```sh
nix develop path:.
# Use release builds: simulation and reconstruction currently run on the CPU.
cargo run --release --example water_board
# Keep the gate closed until O is pressed.
cargo run --release --example water_board -- --closed
# Compare the same quantity of water at the same simulated time.
cargo run --release --example water_board -- --closed --headless /tmp/fluid-closed.png 3
cargo run --release --example water_board -- --headless /tmp/fluid-open.png 3
```

Space pauses physics. O opens/closes the gate. R restores the initial fluid and container. `[` / `]` change playback speed, and WASD/QE plus right mouse drag move the camera. Closing the gate into existing water projects intersecting particles outside the new collider; it is not a continuously moving rigid gate. Headless captures simulate fixed steps up to the requested time (0–60 seconds) before reconstructing the final surface. Console output reports particle count, nominal volume, and particles below the board.

The old authored effect remains available as `cargo run --release --example waterfall_visual`. It is explicitly a visual-effect example, not the fluid implementation.

## Engine Boundaries

- `Scene::ocean: Option<terrain::OceanSurface>` holds authored surface rendering. The former `Scene::water` field has been renamed. `water::Water` remains the underlying compatible parameter type, also exported as `terrain::OceanSurface`.
- `fluid::Fluid` is the independently owned simulation object. Application code controls fixed steps, colliders, resets, and lifetime.
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

## Solver

The implementation is a small CPU, position-based fluid prototype inspired by [Macklin and Müller's Position Based Fluids](https://mmacklin.com/pbf_sig_preprint.pdf). Each fixed step predicts positions under gravity, solves neighbor density constraints, projects against solid boxes, reconstructs velocity, and blends neighboring velocities for viscosity.

A spatial hash finds neighbors within twice the particle spacing. Five Jacobi iterations use a compact polynomial density kernel and its analytic gradient. Rest density is calibrated to the initial cubic lattice. Constraints only resist excess density: they do not attract isolated particles to repair free-surface density deficits. Per-iteration displacement and speed limits prevent very large corrections. A swept point test against radius-expanded boxes limits tunneling through thin walls.

The application accumulator caps catch-up at eight steps per rendered frame. Under load, simulated time falls behind real time rather than increasing the physics timestep. There is no promise of real-time performance at the particle limit.

## Surface and Rendering

The renderer does not display falling-water sprites. `Fluid::surface()` splats the current particle positions into a scalar density field, then extracts an isosurface with marching tetrahedra. Analytic field gradients provide smooth normals. Separated droplets, connected streams, and puddles result from particle distribution. Surface reconstruction is visual; it does not feed back into physics.

Grid spacing is 0.65 times particle spacing. Reconstruction rejects domains above 600,000 grid samples or surfaces above two million vertices. A widely dispersed simulation may therefore continue physically but exceed this prototype's display budget; the demo reports the error and stops rather than silently dropping particles.

`Scene::fluids` uses its own GPU pass with reusable vertex/index buffers. Geometry is uploaded on each rendered frame; buffers grow only when capacity is insufficient. Shading includes procedural-sky Fresnel reflection, scene shadows, approximate background transmission, and normal-based screen distortion. Fluids write their own depth. They are not rendered as the existing horizontal ocean surface.

## Current Limits

This is a first small-scale solver, not a general CFD package. Compression is approximate, boundaries have no density ghost particles, and the reconstructed surface may look lumpy or lose thin connections at this resolution. Nominal mass is conserved by retaining particles; exact incompressibility and geometric volume conservation are not guaranteed. The solver has no surface tension, air phase, rigid-body coupling, mesh colliders, turbulence model, or GPU compute implementation.

The visual fluid surface has no physically measured thickness, inter-fluid refraction, planar object reflections, or fluid shadow casting yet. Its transmission samples the opaque scene and uses a fixed tint. An ocean and a fluid can coexist visually but do not exchange mass or forces. Terrain heightfields require explicitly supplied collision geometry to interact with fluids.

## Verification

CPU tests cover free fall, closed-container retention, release through an opened wall, falling from an elevated container, floor/obstacle exclusion, deterministic fixed stepping, constant particle count and nominal volume, input validation, and surface generation. A Vulkan test renders a changing reconstructed surface without an ocean, checks deterministic unchanged frames, resize equivalence, and removal without stale pixels. Existing ocean, refraction, reflection, foam, FBX, and animation regressions remain in the suite.
