# Water Rendering

The first demo renders an opaque ocean surface and a procedural sky without imported textures or models. It is a visual wave model, not a fluid simulation.

## Geometry and Waves

The mesh is a 256 by 256 cell grid covering 256 meters, with 66,049 vertices and 131,072 triangles. It follows the camera horizontally in one-meter steps. Wave phase is evaluated in world coordinates, so recentering does not restart the waves. The outer part of the patch fades into matching sky haze between 65 and 120 meters.

Four Gerstner waves are summed in the vertex shader. For normalized horizontal direction `d`, wavelength `L`, amplitude `A`, and steepness factor `q = 0.45`:

```text
k = 2π / L
phase = k * dot(d, position.xz) - sqrt(9.81 * k) * time
displacement = (q * A * d.x * cos(phase),
                A * sin(phase),
                q * A * d.y * cos(phase))
```

| Wavelength (m) | Base amplitude (m) | Direction before normalization |
| --- | --- | --- |
| 38 | 0.75 | (0.9, 0.35) |
| 19 | 0.38 | (-0.4, 0.9) |
| 11 | 0.22 | (0.7, -0.6) |
| 7 | 0.12 | (-0.8, -0.2) |

The amplitude control multiplies all four amplitudes, with a range of 0–2. The maximum sum of vertical amplitudes is 2.94 meters; the demo camera stays above 3.5 meters. Analytic X/Z derivatives produce the geometric normal via `normalize(cross(tangent_z, tangent_x))`. Two short fragment-shader ripples add surface detail and fade with distance to reduce aliasing. Zero amplitude disables both displacement and ripples.

## Shading

- Draw a fullscreen sky triangle first, reconstructing view rays with the inverse view-projection matrix.
- Draw the indexed water grid with CCW front faces, back-face culling, and a `Depth32Float` depth attachment.
- Reflect the camera ray about the water normal and sample the procedural sky function.
- Use Schlick Fresnel with `F0 = 0.02037` to blend a deep teal body color into the reflection at grazing angles.
- Add a directional sunlight highlight and a small crest-dependent color variation.
- Apply simple exponential exposure compression in linear space, then let the sRGB target encode the output.

The same WGSL module and pipelines render into window surfaces and offscreen textures. The headless example copies RGBA output into a row-aligned readback buffer and writes a PNG.

## Current Limits

- Reflections contain the procedural sky only; there are no scene-object reflections or screen-space reflections.
- The surface is opaque. There is no refraction, depth-dependent absorption, foam, caustics, underwater shading, or buoyancy yet.
- The patch is finite, hidden with deliberately strong horizon haze. High viewpoints reveal the haze treatment; this is not an infinite-ocean LOD system.
- Ripples and highlights use a simple artistic model rather than a full microfacet BRDF. There is no temporal antialiasing or MSAA.
- Rendering uses one CPU thread and FIFO presentation. This milestone has no performance target or benchmark claim.
- Long-running clocks and very large world coordinates use single-precision floats; floating-origin and phase precision management are future work.

## Validation

Run these commands inside `nix develop path:.`:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --example water -- --ignored
cargo run --locked --example water -- --headless /tmp/water.png 1.25
cargo run --locked --example water -- --frames 10
```

The opt-in GPU test checks wgpu validation, visible changes between two wave times, opaque output, a stable flat surface when amplitude is zero, and resizing to a portrait target with unaligned readback rows. It does not compare exact reference pixels across different drivers.

For an interactive check, resize the window, minimize/restore it, move in diagonal directions, lose focus while moving, pause and adjust the waves, and reset the scene. Confirm that camera movement is independent of wave pause and that zero amplitude produces a calm surface.
