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

// Shared receiving-domain fluid state.  The spill samples the same velocity/height
// field that the lower water surface renders instead of inventing four visual lobes.
const FLOW_W: u32 = 64u;
const FLOW_H: u32 = 128u;
@group(2) @binding(0) var<storage, read> flow_state: array<vec4<f32>>;

fn spill_flow_sample(world: vec2<f32>) -> vec4<f32> {
    let b = scene.secondary_bounds;
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
    let forward = vec3<f32>(scene.waterfall_shape.x, 0.0, scene.waterfall_shape.y);
    let across = vec3<f32>(forward.z, 0.0, -forward.x);
    let drop = scene.waterfall_shape.z;
    let flight = sqrt(2.0 * drop / 9.81);
    let exit_speed = 0.72;
    let landing = origin + forward * (exit_speed * flight) - vec3<f32>(0.0, drop, 0.0);
    let time = scene.camera_time.w;

    var out: SpillVertex;
    out.fade = 1.0;
    out.aeration = 0.0;
    out.thickness = 0.15;
    out.normal = vec3<f32>(0.0, 1.0, 0.0);

    // Receiving-water disturbance. Kept subtle: the impact whitewater, not a visible
    // geometric disk, should describe the landing point.
    if index < 288u {
        out.kind = 0u;
        let slice = index / 3u;
        let vertex = index % 3u;
        let angle = (f32(slice) + select(0.0, 1.0, vertex == 2u)) * (2.0 * PI / 96.0);
        let radius = select(1.0, 0.0, vertex == 0u);
        out.uv = vec2<f32>(cos(angle), sin(angle)) * radius;
        let irregular = 0.94 + 0.06 * fbm(vec2<f32>(angle * 2.7, time * 0.18));
        out.world = landing
            + (across * out.uv.x * (width * 0.5 + 0.65) + forward * out.uv.y * 0.92) * irregular;
        out.aeration = (1.0 - smoothstep(0.05, 0.75, length(out.uv))) * 0.75;
        out.thickness = 0.08;
    // Small board runoff.  Instead of one continuous curtain, lateral discharge
    // bands form several neighbouring rivulets. Their centres drift independently and
    // their widths vary with time, so the lip remains wet without looking funnelled.
    } else if index < 6432u {
        out.kind = 1u;
        let i = index - 288u;
        let cell = i / 6u;
        let uv = (vec2<f32>(f32(cell % 32u), f32(cell / 32u)) + corner(i % 6u)) / 32.0;
        let crest_end = 0.20;
        let crest_u = min(uv.y / crest_end, 1.0);
        let fall_u = max((uv.y - crest_end) / (1.0 - crest_end), 0.0);
        let t = fall_u * flight;
        let speed = exit_speed + 9.81 * t;

        // Read the fluid state along the ballistic landing footprint.  Height and
        // horizontal speed become the local boundary flux; neighbouring cells therefore
        // merge/split the falling surface naturally instead of four authored rivulets.
        let x = uv.x;
        let sample_world = landing.xz + across.xz * ((x - 0.5) * width);
        let fluid = spill_flow_sample(sample_world);
        let flow_speed = length(fluid.yz);
        let flux = clamp(0.22 + fluid.x * 9.0 + flow_speed * 1.8, 0.04, 1.0);
        let local_noise = fbm(vec2<f32>(x * 7.0 + time * 0.10, uv.y * 2.4 - time * 0.16));
        let turbulent = fbm(vec2<f32>(x * 15.0 - time * 0.29, uv.y * 6.0 + time * 0.38));

        // Lateral velocity in the shared domain bends the stream.  Contraction comes
        // from falling acceleration (mass continuity), not hand-authored band centres.
        let lateral_velocity = dot(vec2<f32>(fluid.y, fluid.z), across.xz);
        let contraction = mix(1.0, 0.72, smoothstep(0.22, 1.0, uv.y));
        let contracted_x = 0.5 + (x - 0.5) * contraction;
        let lateral = (contracted_x - 0.5) * width
            + lateral_velocity * t * 0.24
            + (local_noise - 0.5) * width * 0.018 * smoothstep(0.18, 0.85, uv.y);
        let forward_noise = (turbulent - 0.5) * 0.028 * sin(uv.y * PI);
        let crest_forward = crest_u * crest_u * 0.20;
        let crest_drop = crest_u * crest_u * crest_u * 0.07;

        out.world = origin
            + across * lateral
            + forward * (crest_forward + exit_speed * t + forward_noise)
            - vec3<f32>(0.0, crest_drop + 0.5 * 9.81 * t * t, 0.0);
        out.normal = normalize(forward * speed + vec3<f32>(0.0, exit_speed, 0.0)
            + across * ((turbulent - 0.5) * 0.34));
        out.uv = uv;
        // Flux controls continuity and thickness. Low-flux cells become narrow/thin,
        // while high-flux neighbours visually join into one free surface.
        out.aeration = flux;
        out.thickness = mix(0.072, 0.034, uv.y)
            * mix(0.82, 1.14, local_noise)
            * mix(0.38, 1.0, flux);
    // Secondary spray. Billboards are stretched along the ballistic velocity so they
    // read as droplets/ligaments instead of round game particles.
    } else {
        out.kind = 2u;
        let i = index - 6432u;
        let id = f32(i / 6u);
        let age = fract(time * (1.05 + random(id + 5.0) * 0.55) + random(id));
        let t = age * (0.34 + random(id + 23.0) * 0.22);
        let angle = random(id + 17.0) * 2.0 * PI;
        let horizontal = across * cos(angle) + forward * sin(angle);
        let launch = 1.25 + random(id + 8.0) * 1.55;
        let vertical = 1.15 + random(id + 11.0) * 1.85;
        let center = landing
            + across * ((random(id + 3.0) - 0.5) * width * 0.95)
            + horizontal * t * launch
            + vec3<f32>(0.0, vertical * t - 4.905 * t * t, 0.0);

        let velocity = horizontal * launch + vec3<f32>(0.0, vertical - 9.81 * t, 0.0);
        let view = normalize(scene.camera_time.xyz - center);
        let screen_right = normalize(cross(vec3<f32>(0.0, 1.0, 0.001), view));
        let screen_up = normalize(cross(view, screen_right));
        let projected_velocity = velocity - view * dot(velocity, view);
        let tangent = select(screen_up, normalize(projected_velocity), length(projected_velocity) > 0.05);
        let side = normalize(cross(view, tangent));
        let uv = corner(i % 6u) * 2.0 - vec2<f32>(1.0);

        let class_seed = random(id + 41.0);
        let ligament = 1.0 - step(0.16, class_seed);
        let mist = step(0.90, class_seed);
        let droplet = 1.0 - max(ligament, mist);

        let base_radius = mix(0.006, 0.018, random(id + 1.0));
        let radius = base_radius * (droplet + ligament * 0.72 + mist * 2.8);
        let stretch = droplet * mix(1.15, 2.2, random(id + 19.0))
            + ligament * mix(3.0, 6.0, random(id + 29.0))
            + mist * mix(0.65, 1.25, random(id + 37.0));
        out.world = center + side * uv.x * radius + tangent * uv.y * radius * stretch;
        out.normal = view;
        out.uv = uv;
        out.fade = sin(age * PI) * mix(1.0, 0.42, mist);
        out.aeration = droplet * 0.42 + ligament * 0.28 + mist * 0.88;
        out.thickness = radius * mix(2.0, 0.45, mist);
    }

    out.clip = scene.view_projection * vec4<f32>(out.world, 1.0);
    return out;
}

@fragment fn fs_main(in: SpillVertex) -> @location(0) vec4<f32> {
    let time = scene.camera_time.w;
    let view = normalize(scene.camera_time.xyz - in.world);
    let visibility = shadow_visibility(in.world, in.normal);

    if in.kind == 2u {
        let r = length(in.uv);
        let coverage = 1.0 - smoothstep(0.42, 1.0, r);
        let fresnel = 0.02037 + 0.97963 * pow(1.0 - max(dot(in.normal, view), 0.0), 5.0);
        let reflected = sky_lighting(reflect(-view, in.normal), visibility);
        let refracted = refracted_scene(in.clip, in.normal, in.thickness);
        let clear_water = mix(refracted, reflected, fresnel);
        let whitewater = vec3<f32>(0.88, 0.92, 0.91) * (0.58 + 0.42 * visibility);
        let color = mix(clear_water, whitewater, in.aeration * 0.72);
        let opacity = coverage * in.fade * mix(0.16, 0.42, in.aeration);
        return vec4<f32>(color, opacity);
    }

    if in.kind == 0u {
        let radius = length(in.uv);
        let edge = 1.0 - smoothstep(0.76, 1.0, radius);
        let n1 = fbm(in.uv * 8.0 + vec2<f32>(time * 0.35, -time * 0.28));
        let n2 = fbm(in.uv.yx * 15.0 + vec2<f32>(-time * 0.51, time * 0.23));
        let radial = normalize(vec3<f32>(in.uv.x, 0.001, in.uv.y));
        let normal = normalize(vec3<f32>(
            radial.x * (n1 - 0.5) * 0.18,
            1.0,
            radial.z * (n2 - 0.5) * 0.18
        ));
        let fresnel = 0.02037 + 0.97963 * pow(1.0 - max(dot(normal, view), 0.0), 5.0);
        let reflected = sky_lighting(reflect(-view, normal), visibility);
        let water = mix(vec3<f32>(0.025, 0.055, 0.060), reflected, fresnel);
        let impact = exp(-radius * radius * 7.5);
        let foam_noise = smoothstep(0.42, 0.67, n1 * 0.62 + n2 * 0.38);
        let foam = impact * mix(0.35, 1.0, foam_noise) * 0.92;
        let color = mix(water, vec3<f32>(0.84, 0.90, 0.89) * (0.55 + 0.45 * visibility), foam);
        return vec4<f32>(color, edge * (0.035 + foam * 0.58));
    }

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
