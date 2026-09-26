# Falling Water

The water-board example now has a gap in its front rim. A curved, translucent sheet spills through the opening, falls into a small receiving puddle, and produces impact foam, expanding ripples, and looping splash droplets.

```sh
nix develop path:.
cargo run --release --example water_board
cargo run --example water_board -- --headless /tmp/waterfall.png 1.25
cargo run --example water_board -- --no-waterfall --headless /tmp/no-waterfall.png 1.25
```

Space pauses all water effects, `[` / `]` adjust playback speed, and R resets the scene. The spill uses `Water::time` alongside the existing surface. Headless captures accept an explicit time. `--no-waterfall` disables the sheet, puddle, and spray for comparison; it does not restore the closed rim.

## API

```rust
use gigantomachia::water::{Water, Waterfall};
use glam::{Vec2, Vec3};

let spill = Waterfall::new(
    Vec3::new(0.0, 1.55, 2.31), // Outlet center, at the water surface.
    Vec2::Y,                   // Outward direction in world XZ.
    1.55,                      // Outlet width in meters.
    1.515,                     // Drop to the receiving surface in meters.
)?;
let water = Water { waterfall: Some(spill), ..Water::default() };
```

The direction is normalized at construction. Origin and direction must be finite; width and drop must be between 0.05 and 100 meters. The caller places the outlet at the water edge and the landing slightly above the receiving floor to avoid depth fighting. The outlet is independent of `Water::level`; changing the water level requires updating the spill too.

## Rendering

The renderer generates a 32×32 segmented sheet directly from vertex indices. It follows a ballistic path with 1.3 m/s horizontal launch speed and 9.81 m/s² downward acceleration, narrows slightly during the fall, and has small animated flutter. Flowing streaks and sky highlights shade the sheet. A disk at the landing point has a soft irregular boundary, animated rings, and procedural impact foam. Forty-eight camera-facing droplets follow short ballistic arcs.

The pass draws the puddle, sheet, and spray after the main water surface in linear HDR. It uses alpha blending, depth testing, no depth writes, and two-sided rendering. Existing scene shadows affect the effect. The GPU evaluates animation; no new CPU mesh assets or vertex uploads are needed per frame.

## Limitations

This is an authored visual effect, not fluid simulation. It does not conserve water volume, drain the board, collide with obstacles, or determine where the floor is. The puddle does not grow. The receiving puddle is part of this effect, not a second full-featured `Water` surface. It has no scene refraction or planar object reflections; its background is visible through alpha blending. The falling sheet and spray likewise use sky reflections and do not enter the planar reflection or shadow-caster passes.

Transparency uses a fixed draw order rather than per-pixel sorting. Very different viewing angles or intersecting transparent surfaces may show compositing artifacts. The outlet does not follow individual wave crests, so the example uses small surface waves. Increasing the wave amplitude can expose a seam at the outlet. Flow continues independently of the surface amplitude control; pause or remove `Water::waterfall` to stop it.

## Validation

CPU tests cover constructor validation and direction normalization. The Vulkan test checks visible spill coverage, time-dependent animation, identical output when time is held constant, disabling without stale pixels, stable mesh-cache residency, and resize equivalence to a fresh renderer.
