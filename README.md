# Gigantomachia

A draft of a small 3D engine for building games entirely in code, without a GUI editor.

The first milestone is **real-time water rendering**: a navigable ocean surface with Gerstner waves, analytic normals, procedural sky reflections, Fresnel reflectance, and sunlight highlights. No external assets or GUI editor are required. ECS and asset loading remain future work.

## Technology Choices

- **Rust**: Both the engine and the game are written in the same language and built with Cargo.
- **wgpu + WGSL**: Used as the rendering API and shader language. The initial Linux implementation enables only the Vulkan backend.
- **winit + glam**: Window lifecycle, keyboard/mouse input, camera matrices, and vector math.
- **Nix flake**: Provides the Rust toolchain, Vulkan loader, and Wayland/X11 libraries.
- **hecs** remains a candidate for a later ECS milestone; it is not a dependency yet.

By using wgpu to handle GPU resources and command submission, the project can focus on implementing cameras, meshes, and materials.

Removing the editor does not make the initial Rust/wgpu build lightweight. The initial goal is to keep the feature set small and the runtime structure simple; performance and binary size will be measured using demos.

## Getting Started

```sh
# Include the flake even if it has not been added to Git yet.
nix develop path:.
cargo run --release --example water
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
| R | Reset camera, waves, and time |
| Esc | Exit |

The window title displays amplitude, speed, and pause state. Camera height is restricted to 3.5–80 meters because underwater rendering is not implemented.

Headless rendering uses the same pipelines and does not need a display server:

```sh
cargo run --example gpu_info
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
cargo test --example water -- --ignored
vulkaninfo --summary
```

The development shell targets `x86_64-linux` and `aarch64-linux`.

GPU drivers/ICDs must be provided by the host system. On NixOS, enable `hardware.graphics.enable = true;` and configure the appropriate system driver for your GPU. On other Linux distributions, the distribution-provided GPU drivers and their integration with Nix libraries need to be verified separately.

The `gpu_info` example and `water --headless` mode do not create a window. If a software Vulkan device is selected, it may report `Device type: Cpu`; the interactive demo is intended for a hardware GPU.

Dependencies are pinned using `flake.lock` and `Cargo.lock`.

The project uses the wgpu 27.0.1 series as its initial API baseline and does not automatically track the latest release.

## Project Structure

```text
src/app.rs            Window lifecycle, input, and demo clock
src/camera.rs         Above-water fly camera
src/render.rs         Shared Vulkan initialization
src/water.rs          Water grid, uniforms, sky/water passes, depth
src/shaders/water.wgsl  Gerstner displacement and water/sky shading
examples/water.rs     Interactive demo, PNG capture, and GPU test
examples/gpu_info.rs  Development environment verification
flake.nix             Linux development shell
docs/design.md        Architecture and milestones
docs/water.md         Rendering model, limits, and validation
```

See the [design draft](docs/design.md) and [water rendering notes](docs/water.md).

Project documentation is written in English. Completed work is validated and committed to Git.

References: [wgpu feature flags](https://docs.rs/crate/wgpu/27.0.1/features), [winit event handling](https://docs.rs/winit/0.30.13/winit/application/trait.ApplicationHandler.html).
