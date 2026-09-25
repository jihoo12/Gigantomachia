# Water Rendering

The water surface combines Gerstner displacement, sky reflections, directional shadows, screen-space refraction, depth absorption, and animated shoreline foam. It is a visual model, not a fluid simulation; no imported textures or models are required.

## Geometry and Waves

The default `Stylized` mesh is a 256 by 256 cell grid covering 256 meters, with 66,049 vertices and 131,072 triangles. It follows the camera horizontally in one-meter steps. Wave phase is evaluated in world coordinates, so recentering does not restart the waves. The outer part of the patch fades into matching sky haze between 65 and 120 meters.

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

In both examples, the amplitude control multiplies all four amplitudes, with a range of 0–2. The maximum sum of vertical amplitudes is 2.94 meters; the demo camera stays above 3.5 meters. Analytic X/Z derivatives produce the geometric normal via `normalize(cross(tangent_z, tangent_x))`. Two short fragment-shader ripples add surface detail and fade with distance to reduce aliasing. Zero amplitude disables both displacement and ripples.

## Realistic Surface Option

Enable the new surface style in either water demo:

```sh
cargo run --release --example water -- --realistic-water
cargo run --release --example island -- --realistic-water
# Compare identical camera/time settings.
cargo run --example water -- --headless /tmp/water-stylized.png 1.25
cargo run --example water -- --realistic-water --headless /tmp/water-realistic.png 1.25
```

Press `4` to switch between `Stylized` and `Realistic` while running. The default remains `Stylized`. Both styles share wave displacement, time, absorption, refraction, shadows, and shoreline foam. Reset restores the startup style, including the CLI option.

`Realistic` addresses broad, faceted-looking highlights with three changes:

- A 512×512 cell mesh reduces spacing to 0.5 meters across the same patch. It contains 263,169 vertices and 524,288 triangles. The renderer allocates this additional grid on first use and retains it for subsequent toggles (about 8 MiB of vertex/index buffers). Camera recentering remains in one-meter increments for both styles.
- Large-wave analytic normals are evaluated per fragment using interpolated, undisplaced wave coordinates. Eight directional short waves, from 3.1 m to 4.7 cm, use noise-modulated phases and strengths to break up regular bands and add shading detail without increasing displacement. Screen derivatives filter out unresolved ripples; distance fades detail near the horizon. Zero amplitude disables all of these waves.
- A GGX sun highlight replaces the artistic power highlight. Correlated Smith visibility and Schlick water Fresnel control the reflection; normal variation across pixels broadens the highlight to reduce sparkle aliasing. The reflected sky excludes its sharp sun disk to avoid counting direct sunlight twice. A darker body tint and restrained backlighting reduce the broad turquoise bands.

The direct specular equations follow the microfacet model described in [Filament's material reference](https://google.github.io/filament/main/filament.html). Sun intensity and scattering are still artistic choices; this option is not a physically complete ocean simulation. Sky reflection is not prefiltered by roughness, and the lighting is not calibrated to physical exposure units.

```rust
use gigantomachia::water::{Water, WaterStyle};

let water = Water {
    style: WaterStyle::Realistic,
    ripple_strength: 1.0,
    roughness: 0.22,
    ..Water::default()
};
```

`ripple_strength` changes short-wave slopes (0–2), and `roughness` controls the direct sun highlight (0.08–0.6, perceptual). These fields only affect the realistic style. Non-finite values fall back to their defaults during upload. The extra vertices and fragment calculations cost GPU time; use the toggle to choose the desired quality on your hardware.

## Render Passes and Color Space

1. Render opaque mesh casters into a 2048² `Depth32Float` directional shadow map.
2. Render sky and opaque meshes into `Rgba16Float` HDR scene color plus sampled `Depth32Float` scene depth.
3. Copy HDR color and depth to separate composition attachments. The water pass samples the original buffers while writing to the copies, avoiding texture read/write feedback.
4. Draw water with depth testing so dry land is preserved. Combine transmitted scene color, sky reflection, sunlight, and foam in linear HDR space.
5. Apply exponential exposure compression once in `post.wgsl`; the final sRGB target performs encoding.

The engine owns these resources and recreates all screen-sized attachments and bindings together on resize. Windowed and headless rendering use the same passes. At 1280×720, the two HDR colors, two screen depths, and shadow map occupy approximately 37 MiB, excluding meshes, output/surface textures, and driver overhead.

## Directional Shadows

`Scene::sun` contains the direction toward the sun and a shadow toggle. The orthographic shadow volume covers 200 meters in each light-space XY axis and 360 meters of light-space depth around the camera's horizontal position at sea level. Its XY center is snapped to shadow texels. A fallback up vector handles a vertical sun.

Mesh instances reuse the same geometry and transforms in shadow and color passes. The receiver uses 3×3 comparison filtering, raster depth bias, and a small normal/reference offset. Shadows fade at the map edge; receivers outside its volume are lit. Only direct sunlight is attenuated: ambient sky light remains. Water receives mesh shadows on direct scattering, sun reflection/highlights, and foam, but does not cast its own shadow.

## Refraction and Absorption

The fragment's screen UV and opaque depth reconstruct the submerged scene position through the inverse view-projection matrix. Clear depth is treated as open ocean, never as a seabed.

The shader refracts the incident camera ray with an air-to-water ratio of `1 / 1.333`, projects a short point along that ray, and uses the resulting screen offset to sample opaque HDR color. Displacement is capped at 2.5% of the viewport per axis and fades at the shoreline. Offscreen, sky, and foreground samples fall back to the undistorted location, reducing land leaking into the water.

Transmission follows a Beer-Lambert approximation:

```text
transmission = exp(-absorption_rgb * path_length)
body = submerged_scene_color * transmission
     + water_scattering_color * (1 - transmission)
```

Path length is measured between the surface and sampled scene position and capped at 80 meters. Red is absorbed faster than green and blue by default. Deep or missing geometry falls back to the ocean tint. Schlick Fresnel (`F0 = 0.02037`) blends the transmitted body into the reflected procedural sky at grazing angles.

This is a screen-space approximation, not ray tracing or physical volumetric transport. It cannot reveal geometry hidden from the opaque camera pass. Disocclusion, screen edges, steep waves, and thin foreground silhouettes can still produce artifacts. Deepen a finite seabed before its boundary if that edge should disappear under absorption; the island generator does this.

## Shoreline Foam

Foam uses undistorted vertical water depth, so refraction does not move it off the terrain contact. A shallow-depth mask combines moving world-space noise, a repeating breaker band, and a narrow contact rim. Foam is lit by the same directional shadow visibility. It evolves with `Water::time`, so pausing the example also pauses foam.

There is no shoreline texture or island-specific branch. Any submerged mesh approaching the surface can produce contact foam. Clear depth and water deeper than `foam_width` produce none. The effect is procedural and has no fluid advection, wave breaking simulation, or persistent foam history.

## Parameters and Comparison Controls

`Water` contains these fields in addition to amplitude, time, and sea level:

| Field | Default | Behavior |
| --- | --- | --- |
| `style` | `WaterStyle::Stylized` | Selects the existing or realistic surface |
| `ripple_strength` | `1.0` | Realistic short-wave slope multiplier, clamped to 0–2 |
| `roughness` | `0.22` | Realistic sun-highlight perceptual roughness, clamped to 0.08–0.6 |
| `refraction` | `true` | Enables submerged color transmission and absorption |
| `refraction_strength` | `0.7` | Screen displacement multiplier, clamped to 0–1; zero still transmits undistorted color |
| `absorption` | `[0.42, 0.12, 0.055]` | RGB absorption per meter, clamped to 0–10 |
| `foam_strength` | `1.0` | Opacity multiplier, clamped to 0–1; zero disables foam |
| `foam_width` | `1.4` | Vertical depth range in meters, clamped to 0.05–10 |

The demo keys `1`, `2`, and `3` toggle shadows, refraction, and foam independently; `4` switches surface style. The window title shows their state. `R` restores startup settings, including any command-line effect flags. `--no-refraction` disables transmission, not the separate foam or shadow effects.

## Current Limits

- Reflections contain the procedural sky only; there are no scene-object reflections, underwater views, caustics, or buoyancy.
- Only one water surface and one directional shadow map are supported. There are no shadow cascades or point-light shadows. Bias and finite resolution can cause shadow detachment or residual acne at difficult angles.
- The finite water patch still uses strong horizon haze. This is not an infinite-ocean LOD system.
- Waves and foam remain procedural; the realistic style adds microfacet direct sunlight but no full volume model. There is no temporal antialiasing or MSAA, so fine shoreline detail can shimmer.
- Rendering uses one CPU thread and FIFO presentation. The additional shadow pass, HDR attachments, and samples cost GPU time and memory; no performance target is claimed.
- Long-running clocks and large world coordinates use single-precision floats. Floating-origin and wave-phase precision management remain future work.

## Validation

Run these commands inside `nix develop path:.`:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo test --locked --test rendering -- --ignored
cargo run --locked --example island -- --headless /tmp/island.png 1.25
cargo run --locked --example island -- --headless /tmp/island-base.png 1.25 --no-shadows --no-refraction --no-foam
cargo run --locked --example water -- --frames 10
```

Opt-in Vulkan integration tests cover existing scene/resource behavior plus cast shadows on land/water, stale shadow removal, shallow transmission, increased absorption with depth, refraction strength, shallow-only animated foam, dry-land/open-ocean preservation, and resize equivalence to a fresh renderer. A realistic-water test additionally covers style switching, ripple/roughness controls, animation, zero-amplitude stability, interaction with refraction/foam/shadows, and resize equivalence. Output remains opaque RGBA because transmission is composed in the shader, not through alpha blending. Tests do not compare exact reference pixels across different drivers.

For an interactive check, resize the window, minimize/restore it, move in diagonal directions, lose focus while moving, pause and adjust the waves, and reset the scene. Confirm that camera movement is independent of wave pause and that zero amplitude produces a calm surface.
