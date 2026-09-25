# Procedural Island

The island example composes an ordinary opaque mesh and the existing water surface through the shared engine renderer:

```sh
nix develop path:.
cargo run --release --example island
cargo run --example island -- --headless /tmp/island.png 1.25
```

## Generation

`terrain::Island` is a CPU generator with configurable `center`, `radius`, `peak_height`, `cells`, and `seed`. Defaults are a 30-meter nominal radius, 18-meter relief scale, 160 cells per axis, seed 7, and world XZ center `(0, -12)`. `peak_height` controls mountain relief, not an exact upper elevation bound.

The mesh spans 3.6 times the nominal radius and has 25,921 vertices and 51,200 triangles. Its offshore shelf descends from about -4 meters to -69 meters before the square boundary. This makes the finite mesh edge disappear through water absorption instead of exposing a rectangular shallow-water patch. A perturbed elliptical radius creates an irregular coastline; a smooth coastal shelf forms the beach; seeded value noise modifies the central mountain profile. No external noise package or runtime random generator is needed.

Normals are computed from central height differences. Vertex colors blend wet/dry sand, grass, and rock using elevation and slope, with seeded variation. A regular indexed grid with CCW winding is submitted as `Mesh`; the renderer applies the same directional/ambient lighting used for any vertex-colored opaque mesh.

The example uses lower wave amplitude (0.55) and an elevated camera to make the island visible on startup. Navigation keeps the camera above the sampled heightfield plus a clearance margin. The height sampler is an analytic approximation to the triangulated terrain and is not a collision system.

## Engine Integration

The renderer does not know about `Island`. Its input is `Scene { camera, meshes, water, sun }`, and the example inserts `MeshInstance::new(Arc::new(island.mesh()?))`. Other procedural generators or future file loaders can return the same `Mesh` type.

The renderer first draws mesh shadow depth, then opaque HDR scene color/depth. Water depth-tests against a copy of opaque depth, so dry land remains visible. Submerged geometry is sampled through refractive water with depth-dependent absorption, while near-contact depth produces animated foam. The same mesh asset is reused by the shadow and color passes and stays cached while the scene references it.

## Limits

- This is one static heightfield, with no caves, overhangs, erosion simulation, terrain LOD, or editing tools.
- Sand, grass, and rock are colors, not texture assets, vegetation, or a PBR material system.
- Cast shadows, refractive shallows, and shoreline foam are implemented. Island reflections, underwater rendering, and caustics remain absent. Refraction uses only geometry visible in the opaque camera pass, not ray tracing.
- Terrain uses the same distance haze as the finite water patch. Large islands and far-away views need a future ocean/terrain LOD and atmosphere treatment.
- A seed reproduces geometry on the same implementation; cross-platform bit-exact floating-point output is not promised.

## Validation

```sh
cargo test --locked --all-targets
cargo test --locked --test rendering -- --ignored
cargo run --locked --example island -- --frames 10
```

CPU tests check dry land, submerged boundaries, upward winding, finite unit normals, repeatable seeds, and invalid configuration rejection. GPU tests check visible island coverage, shared mesh residency, instance transforms, resource eviction, a controlled mesh above/below flat water, shadow receivers, refraction/absorption, and shallow-only foam. Inspect a headless PNG to assess the coastline and material transitions.

Use `1`, `2`, and `3` to toggle shadows, refraction, and foam. The flags `--no-shadows`, `--no-refraction`, and `--no-foam` also work with `--headless` for repeatable comparisons.
