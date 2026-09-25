# Gigantomachia

A draft of a small 3D engine for building games entirely in code, without a GUI editor.

The engine now renders **water and a procedural island** through a shared scene renderer. The ocean uses Gerstner waves, analytic normals, sky reflections, Fresnel reflectance, screen-space refraction, depth absorption, and animated shoreline foam. Directional shadow maps shade both land and water. Planar reflections show above-water meshes in the water. An optional realistic water style adds denser geometry, per-pixel wave normals, flowing irregular ripples, and GGX sunlight. The island has an irregular coastline, sandy beaches, grass, and rock colors. No external assets or GUI editor are required. ASCII/binary FBX meshes and rigid node animation can also be loaded through the engine asset API; ECS remains future work.

## Technology Choices

- **Rust**: Both the engine and the game are written in the same language and built with Cargo.
- **wgpu + WGSL**: Used as the rendering API and shader language. The initial Linux implementation enables only the Vulkan backend.
- **ufbx**: Static FBX parsing with baked transforms and vertex/material colors.
- **winit + glam**: Window lifecycle, keyboard/mouse input, camera matrices, and vector math.
- **Nix flake**: Provides the Rust toolchain, Vulkan loader, and Wayland/X11 libraries.
- **hecs** remains a candidate for a later ECS milestone; it is not a dependency yet.

By using wgpu to handle GPU resources and command submission, the project can focus on implementing cameras, meshes, and materials.

Removing the editor does not make the initial Rust/wgpu build lightweight. The initial goal is to keep the feature set small and the runtime structure simple; performance and binary size will be measured using demos.

## Getting Started

```sh
# Include the flake even if it has not been added to Git yet.
nix develop path:.
cargo run --release --example island
# Enable more detailed water shading (press 4 to compare styles).
cargo run --release --example water -- --realistic-water
# Shallow water on a wooden board with a raised rim.
cargo run --release --example water_board
# The original ocean-only scene is still available.
cargo run --release --example water
# View the bundled FBX fixture, or provide your own model path.
cargo run --release --example fbx -- path/to/model.fbx
# Play the bundled moving, rotating cube.
cargo run --example fbx -- --animate
```

For a quicker development build, omit `--release`. Release builds improve CPU-side frame performance.

| Control | Action |
| --- | --- |
| WASD | Move horizontally |
| Q / E | Move down / up (above the water) |
| Shift | Move faster |
| Right mouse drag | Look around |
| Space | Pause/resume waves; the camera remains active |
| `-` / `+` (or `=`) | Decrease/increase wave amplitude, 0–2 |
| `[` / `]` | Decrease/increase wave speed, 0–3 |
| 1 | Toggle directional shadows |
| 2 | Toggle refraction and depth absorption |
| 3 | Toggle shoreline foam |
| 4 | Switch stylized/realistic water |
| 5 | Toggle scene reflections |
| R | Reset camera, waves, effects, and time to startup settings |
| Esc | Exit |

The water and island examples share these controls. The FBX viewer uses the same navigation and shadow toggle without water controls or terrain clearance. The window title displays amplitude, speed, pause state, and effect toggles. Example camera controls keep 3.5 meters of clearance above sea level or the island heightfield, with an 80-meter altitude limit. This is a navigation convenience, not a physics system; the engine camera itself has no water/terrain constraints.

Headless rendering uses the same pipelines and does not need a display server:

```sh
cargo run --example gpu_info
cargo run --example island -- --headless /tmp/island.png
# Disable effects independently for comparison (also works in windowed mode).
cargo run --example island -- --headless /tmp/island-base.png --no-shadows --no-refraction --no-foam
cargo run --example water -- --headless /tmp/water.png
# An optional time in seconds makes captures reproducible on the same GPU/driver.
cargo run --example water -- --headless /tmp/water-t0.png 0
# Exit after ten successful presentations for a window smoke test.
cargo run --example water -- --frames 10
```

Validation commands:

```sh
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
# Opt-in test: requires a Vulkan adapter, but no display server.
cargo test --test rendering -- --ignored
vulkaninfo --summary
```

The development shell targets `x86_64-linux` and `aarch64-linux`.

GPU drivers/ICDs must be provided by the host system. On NixOS, enable `hardware.graphics.enable = true;` and configure the appropriate system driver for your GPU. On other Linux distributions, the distribution-provided GPU drivers and their integration with Nix libraries need to be verified separately.

The `gpu_info` example and the `--headless` modes do not create a window. If a software Vulkan device is selected, it may report `Device type: Cpu`; the interactive demo is intended for a hardware GPU.

Dependencies are pinned using `flake.lock` and `Cargo.lock`.

The project uses the wgpu 27.0.1 series as its initial API baseline and does not automatically track the latest release.

## Project Structure

```text
src/app.rs             Generic Application host and window lifecycle
src/input.rs           Held/pressed keys and accumulated mouse movement
src/scene.rs           Camera, sunlight, optional water, mesh instances
src/mesh.rs            Validated immutable CPU geometry
src/asset/             CPU-only FBX import and rigid node animation
src/camera.rs          Perspective fly camera
src/water.rs           Water parameters, independent of demo playback
src/terrain.rs         Seeded island heightfield generator
src/render/            GPU, shadow/HDR/water passes, frame targets, readback
src/shaders/           Lighting, shadows, refraction, foam, and tone mapping
examples/water.rs      Ocean scene setup
examples/water_board.rs Bounded shallow water on a wooden board
examples/island.rs     Island scene setup
examples/fbx.rs        FBX viewer with optional animation
examples/support/     Demo controls, CLI, and PNG writing
examples/gpu_info.rs   Development environment verification
tests/rendering.rs     Public engine API GPU integration tests
tests/fbx.rs           ASCII/binary FBX import regressions
flake.nix              Linux development shell
docs/design.md         Engine boundaries and extension guide
docs/water.md          Water model and limitations
docs/island.md         Island generation and limitations
```

See the [engine design and API guide](docs/design.md), [water notes](docs/water.md), [island notes](docs/island.md), [FBX import guide](docs/fbx.md), and [animation guide](docs/animation.md). The renderer owns GPU resources and frame submission; examples only provide scene data and application behavior.

Project documentation is written in English. Completed work is validated and committed to Git.

References: [wgpu feature flags](https://docs.rs/crate/wgpu/27.0.1/features), [winit event handling](https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html).
