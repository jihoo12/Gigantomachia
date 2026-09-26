struct SpillVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) @interpolate(flat) kind: u32,
    @location(4) fade: f32,
    @location(5) aeration: f32,
    @location(6) thickness: f32,
}

fn random(seed: f32) -> f32 {
    return fract(sin(seed * 127.1 + 31.7) * 43758.5453);
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash2(i), hash2(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash2(i + vec2<f32>(0.0, 1.0)), hash2(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

fn fbm(p0: vec2<f32>) -> f32 {
    var p = p0;
    var sum = 0.0;
    var weight = 0.5;
    for (var octave = 0; octave < 4; octave = octave + 1) {
        sum += value_noise(p) * weight;
        p = mat2x2<f32>(vec2<f32>(1.62, 1.17), vec2<f32>(-1.17, 1.62)) * p + vec2<f32>(13.1, 7.7);
        weight *= 0.5;
    }
    return sum / 0.9375;
}

@group(1) @binding(0) var opaque_color: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
@group(1) @binding(2) var opaque_depth: texture_depth_2d;
@group(1) @binding(3) var reflection_color: texture_2d<f32>;

// Shared producer-domain fluid state. The spill samples the upper water domain at the lip.
const FLOW_W: u32 = 64u;
const FLOW_H: u32 = 128u;
@group(2) @binding(0) var<storage, read> flow_state: array<vec4<f32>>;

fn spill_flow_sample(world: vec2<f32>) -> vec4<f32> {
    let b = scene.water_bounds;
    let uv = clamp((world - (b.xy - b.zw)) / (b.zw * 2.0), vec2<f32>(0.0), vec2<f32>(1.0));
    let x = u32(uv.x * f32(FLOW_W - 1u));
    let y = u32(uv.y * f32(FLOW_H - 1u));
    return flow_state[y * FLOW_W + x];
}

fn refracted_scene(clip: vec4<f32>, normal: vec3<f32>, thickness: f32) -> vec3<f32> {
    let size = vec2<f32>(textureDimensions(opaque_color));
    let uv = clip.xy / size;
    let distortion = normal.xz * (0.010 + min(thickness, 0.25) * 0.055) * scene.effects.y;
    let sample_uv = clamp(uv + distortion, vec2<f32>(0.002), vec2<f32>(0.998));
    let background = textureSampleLevel(opaque_color, color_sampler, sample_uv, 0.0).rgb;
    let transmission = exp(-scene.absorption.rgb * max(thickness, 0.015));
    return background * transmission;
}

fn corner(index: u32) -> vec2<f32> {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0)
    );
    return corners[index];
}

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SpillVertex {
    let origin = scene.waterfall_origin.xyz;
    let width = scene.waterfall_origin.w;
    let forward = normalize(vec3<f32>(scene.waterfall_shape.x, 0.0, scene.waterfall_shape.y));
    let across = vec3<f32>(forward.z, 0.0, -forward.x);
    let drop = max(scene.waterfall_shape.z, 0.05);
    let time = scene.camera_time.w;

    // 64 independent source cells across the physical lip, 16 ballistic segments each.
    let quad = index / 6u;
    let lip_cell = quad % 64u;
    let segment = quad / 64u;
    let c = corner(index % 6u);
    let lateral_u = (f32(lip_cell) + c.x) / 64.0;
    let path_u = (f32(segment) + c.y) / 16.0;

    // Sample the UPPER fluid domain at the physical lip. The renderer binds the upper
    // state here, so geometry is created from simulation state rather than a sheet mask.
    let lateral = (lateral_u - 0.5) * width;
    let source_world = origin.xz + across.xz * lateral;
    // Reconstruct a continuous lip state from neighbouring producer cells. Each ribbon
    // still carries local flux, but adjacent vertices now share interpolated boundary
    // conditions instead of behaving like disconnected strands.
    let cell_world = width / 64.0;
    let fluid_l = spill_flow_sample(source_world - across.xz * cell_world);
    let fluid_c = spill_flow_sample(source_world);
    let fluid_r = spill_flow_sample(source_world + across.xz * cell_world);
    let fluid = fluid_c * 0.50 + (fluid_l + fluid_r) * 0.25;
    let outward = max(dot(fluid.yz, forward.xz), 0.0);
    let depth = max(0.055 + fluid.x, 0.0);
    let flux = depth * outward;

    // A shallow-water cell at rest still spills under gravity at an open lip.
    let gravity_exit = sqrt(2.0 * 9.81 * max(depth, 0.001));
    let exit_speed = max(outward, gravity_exit * 0.34);
    let flight = sqrt(2.0 * drop / 9.81);
    let t = path_u * flight;
    let lateral_velocity = dot(fluid.yz, across.xz);

    // Continuity: accelerating water occupies a smaller cross-section as it falls.
    let vertical_speed = 9.81 * t;
    let speed_ratio = sqrt(max(exit_speed, 0.05) / max(sqrt(exit_speed * exit_speed + vertical_speed * vertical_speed), 0.05));
    let contracted = (lateral_u - 0.5) * width * mix(1.0, speed_ratio, path_u);
    let jitter = (fbm(vec2<f32>(f32(lip_cell) * 0.31, time * 0.17 + path_u * 2.0)) - 0.5)
        * width * 0.008 * path_u;

    // The simulated boundary is at the water-domain edge (z ~= 2.32), while the
    // wooden front rim extends to z ~= 2.50. Move the free-flight origin just beyond
    // that solid lip; otherwise the ballistic surface visibly passes through the wood.
    // Scale the clearance from the small gap between the declared spill origin and the
    // water bound so the renderer remains tied to scene geometry rather than a waterfall
    // effect constant.
    let domain_edge = scene.water_bounds.xy + forward.xz * scene.water_bounds.zw;
    let boundary_gap = max(dot(domain_edge - origin.xz, forward.xz), 0.0);
    let lip_clearance = max(boundary_gap + 0.18, 0.19);
    let flight_origin = origin + forward * lip_clearance;

    var out: SpillVertex;
    out.kind = 1u;
    out.world = flight_origin
        + across * (contracted + lateral_velocity * t + jitter)
        + forward * (exit_speed * t)
        - vec3<f32>(0.0, 0.5 * 9.81 * t * t, 0.0);
    let tangent = forward * exit_speed + across * lateral_velocity - vec3<f32>(0.0, 9.81 * t, 0.0);
    out.normal = normalize(cross(across, tangent));
    out.uv = vec2<f32>(lateral_u, path_u);
    // Coverage is now a direct consequence of source-cell flux. A tiny floor represents
    // gravity-driven overflow while the newly-created upper domain settles.
    out.aeration = clamp(max(flux * 32.0, depth * 2.8), 0.0, 1.0);
    out.thickness = max(depth * speed_ratio, 0.004);
    out.fade = smoothstep(0.006, 0.030, depth) * smoothstep(0.0, 0.018, max(flux, depth * 0.012));
    out.clip = scene.view_projection * vec4<f32>(out.world, 1.0);
    return out;
}

@fragment fn fs_main(in: SpillVertex) -> @location(0) vec4<f32> {
    let time = scene.camera_time.w;
    let view = normalize(scene.camera_time.xyz - in.world);
    let visibility = shadow_visibility(in.world, in.normal);

    // Multi-scale capillary perturbation. Unlike the old periodic vertical sine streaks,
    // these layers have no single visible repetition direction.
    let coarse = fbm(vec2<f32>(in.uv.x * 8.0 + time * 0.22, in.uv.y * 5.0 - time * 0.61));
    let fine = fbm(vec2<f32>(in.uv.x * 31.0 - time * 0.47, in.uv.y * 19.0 + time * 0.83));
    let micro_x = (coarse - 0.5) * 0.14 + (fine - 0.5) * 0.055;
    let micro_z = (fbm(vec2<f32>(in.uv.x * 17.0 + 9.7, in.uv.y * 23.0 - time * 0.74)) - 0.5) * 0.09;
    var normal = normalize(in.normal + vec3<f32>(micro_x, 0.0, micro_z));
    if dot(normal, view) < 0.0 { normal = -normal; }

    // Physical dielectric Fresnel for air/water (IOR ~= 1.333).
    let facing = max(dot(normal, view), 0.0);
    let fresnel = 0.02037 + 0.97963 * pow(1.0 - facing, 5.0);
    let reflected = sky_lighting(reflect(-view, normal), visibility);

    // Thin falling water is nearly colourless. The weak Beer-Lambert term only becomes
    // noticeable where the sheet is optically thick; blue paint is deliberately absent.
    let absorption = vec3<f32>(0.42, 0.12, 0.055);
    let transmission = exp(-absorption * in.thickness);
    let ambient_transmission = refracted_scene(in.clip, normal, in.thickness) * transmission;
    var color = mix(ambient_transmission, reflected, fresnel);

    // Air entrainment, not an arbitrary streak texture, creates white water.
    let breakup_noise = fbm(vec2<f32>(in.uv.x * 21.0 + time * 0.31, in.uv.y * 11.0 - time * 0.58));
    let aeration = clamp(in.aeration * mix(0.55, 1.0, breakup_noise), 0.0, 1.0);
    let whitewater = vec3<f32>(0.86, 0.91, 0.90) * (0.52 + 0.48 * visibility);
    color = mix(color, whitewater, aeration * 0.78);

    // Coverage follows optical thickness, Fresnel and entrained air. This keeps the
    // coherent centre transparent while edges/broken regions become bright and opaque.
    let optical = 1.0 - exp(-in.thickness * 5.0);
    let edge = pow(abs(in.uv.x * 2.0 - 1.0), 9.0);
    let breakup_field = fbm(vec2<f32>(in.uv.x * 9.0 - time * 0.42, in.uv.y * 7.0 + time * 0.63));
    let discharge = in.aeration;
    // Weak lateral bands are genuinely dry. Strong neighbouring bands overlap and read
    // as naturally merging rivulets rather than one alpha-masked sheet.
    let coverage_noise = fbm(vec2<f32>(in.uv.x * 18.0 + time * 0.17, in.uv.y * 4.5 - time * 0.24));
    let threshold = mix(0.16, 0.34, smoothstep(0.15, 0.95, in.uv.y));
    let holes = 1.0 - smoothstep(threshold - 0.10, threshold + 0.08, discharge * (0.82 + 0.28 * coverage_noise));
    // 'holes' is now the dry-band probability itself, so it replaces the old
    // breakup_zone/edge_loss controls that belonged to the single-curtain model.
    if breakup_field < holes {
        discard;
    }
    let wet_coverage = 1.0 - holes;
    let alpha = clamp(
        (0.025 + optical * 0.23 + fresnel * 0.24 + aeration * 0.34 + edge * 0.03)
            * mix(0.45, 1.0, wet_coverage),
        0.015, 0.84
    );
    return vec4<f32>(color, alpha);
}
