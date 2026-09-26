const FLOW_W: u32 = 64u;
const FLOW_H: u32 = 128u;
@group(2) @binding(0) var<storage, read> flow_state: array<vec4<f32>>;

fn flow_sample(world: vec2<f32>) -> vec4<f32> {
    let b = scene.secondary_bounds;
    let uv = clamp((world - (b.xy - b.zw)) / (b.zw * 2.0), vec2<f32>(0.0), vec2<f32>(1.0));
    let x = u32(uv.x * f32(FLOW_W - 1u));
    let y = u32(uv.y * f32(FLOW_H - 1u));
    return flow_state[y * FLOW_W + x];
}

struct WaveSample {
    displacement: vec3<f32>,
    tangent_x: vec3<f32>,
    tangent_z: vec3<f32>,
}

// Analytic derivatives of Gerstner displacement, summed before taking the normal.
fn wave(point: vec2<f32>, direction: vec2<f32>, wavelength: f32, height: f32, amplitude_scale: f32) -> WaveSample {
    let d = normalize(direction);
    let k = 2.0 * PI / wavelength;
    let amplitude = height * amplitude_scale;
    let phase = k * dot(d, point) - sqrt(9.81 * k) * scene.camera_time.w;
    let s = sin(phase);
    let c = cos(phase);
    // Confined water moves vertically so it cannot drift outside its rectangle.
    let q = select(0.45, 0.0, scene.water_bounds.z > 0.0);
    var out: WaveSample;
    out.displacement = vec3<f32>(q * amplitude * d.x * c, amplitude * s, q * amplitude * d.y * c);
    out.tangent_x = vec3<f32>(-q * amplitude * k * d.x * d.x * s, amplitude * k * d.x * c, -q * amplitude * k * d.x * d.y * s);
    out.tangent_z = vec3<f32>(-q * amplitude * k * d.x * d.y * s, amplitude * k * d.y * c, -q * amplitude * k * d.y * d.y * s);
    return out;
}

struct WaterVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) wave_point: vec2<f32>,
    @location(3) surface_data: vec3<f32>, // amplitude, 1 for secondary, level
}

@vertex fn vs_main(@location(0) grid: vec2<f32>, @builtin(instance_index) instance: u32) -> WaterVertex {
    let secondary = instance > 0u && scene.secondary_water.z > 0.5;
    let amplitude_scale = select(scene.water.x, scene.secondary_water.x, secondary);
    let level = select(scene.water.w, scene.secondary_water.y, secondary);
    let bounds = select(scene.water_bounds, scene.secondary_bounds, secondary);
    var point = grid + scene.water.yz;
    if bounds.z > 0.0 {
        point = bounds.xy + (grid / 128.0) * bounds.zw;
    }
    let a = wave(point, vec2<f32>(0.9, 0.35), 38.0, 0.75, amplitude_scale);
    let b = wave(point, vec2<f32>(-0.4, 0.9), 19.0, 0.38, amplitude_scale);
    let c = wave(point, vec2<f32>(0.7, -0.6), 11.0, 0.22, amplitude_scale);
    let d = wave(point, vec2<f32>(-0.8, -0.2), 7.0, 0.12, amplitude_scale);
    var displacement = a.displacement + b.displacement + c.displacement + d.displacement;
    var tx = vec3<f32>(1.0, 0.0, 0.0) + a.tangent_x + b.tangent_x + c.tangent_x + d.tangent_x;
    var tz = vec3<f32>(0.0, 0.0, 1.0) + a.tangent_z + b.tangent_z + c.tangent_z + d.tangent_z;

    // Couple the confined surface to the waterfall: water is drawn toward the spill,
    // accelerates at the lip, and forms a shallow drawdown instead of ending as a calm plane.
    if scene.waterfall_origin.w > 0.0 && !secondary {
        let spill = scene.waterfall_origin.xyz;
        let flow = normalize(vec2<f32>(scene.waterfall_shape.x, scene.waterfall_shape.y));
        let across = vec2<f32>(flow.y, -flow.x);
        let rel = point - spill.xz;
        let downstream = dot(rel, flow);
        let lateral = dot(rel, across);
        let half_width = scene.waterfall_origin.w * 0.5;
        let channel = 1.0 - smoothstep(half_width * 0.75, half_width * 1.45, abs(lateral));
        let approach = smoothstep(-2.4, -0.05, downstream) * (1.0 - smoothstep(-0.05, 0.75, downstream));
        let suction = channel * approach;
        let pulse = sin(scene.camera_time.w * 5.2 - downstream * 7.0 + lateral * 3.1) * 0.5 + 0.5;
        displacement.y -= suction * (0.045 + 0.035 * pulse);
        displacement.x += flow.x * suction * 0.055;
        displacement.z += flow.y * suction * 0.055;
        // Steepen the surface toward the outlet so highlights visibly stream into it.
        tx.y -= flow.x * suction * 0.10 + across.x * lateral * channel * 0.035;
        tz.y -= flow.y * suction * 0.10 + across.y * lateral * channel * 0.035;
    }
    if secondary {
        let sim = flow_sample(point);
        displacement.y += sim.x;
        // Velocity slightly leans the mesh in the actual transported direction.
        displacement.x += sim.y * 0.018;
        displacement.z += sim.z * 0.018;
    }
    let world = vec3<f32>(point.x, level, point.y) + displacement;
    var out: WaterVertex;
    out.world = world;
    out.wave_point = point;
    out.surface_data = vec3<f32>(amplitude_scale, select(0.0, 1.0, secondary), level);
    out.normal = normalize(cross(tz, tx));
    out.clip = scene.view_projection * vec4<f32>(world, 1.0);
    return out;
}

@group(1) @binding(0) var opaque_color: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
@group(1) @binding(2) var opaque_depth: texture_depth_2d;
@group(1) @binding(3) var reflection_color: texture_2d<f32>;

fn scene_depth(uv: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(opaque_depth));
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1));
    return textureLoad(opaque_depth, pixel, 0);
}

fn world_at(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let clip = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), depth, 1.0);
    let world = scene.inverse_view_projection * clip;
    return world.xyz / world.w;
}

fn hash(point: vec2<f32>) -> f32 {
    return fract(sin(dot(point, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(point: vec2<f32>) -> f32 {
    let cell = floor(point);
    let f = fract(point);
    let t = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(cell), hash(cell + vec2<f32>(1.0, 0.0)), t.x),
        mix(hash(cell + vec2<f32>(0.0, 1.0)), hash(cell + vec2<f32>(1.0)), t.x), t.y);
}

// Analytic gradient of a compact radial kernel on a simplex lattice.
// Unlike crossed sine waves, this produces short, irregular ridges without long repeated bands.
fn gradient_corner(cell: vec2<f32>, offset: vec2<f32>) -> vec2<f32> {
    let angle = hash(cell) * (2.0 * PI);
    let g = vec2<f32>(cos(angle), sin(angle));
    let weight = max(0.5 - dot(offset, offset), 0.0);
    let w2 = weight * weight;
    return w2 * w2 * g - 8.0 * w2 * weight * dot(g, offset) * offset;
}

fn slope_noise(point: vec2<f32>) -> vec2<f32> {
    let cell = floor(point + (point.x + point.y) * 0.3660254);
    let a = point - cell + (cell.x + cell.y) * 0.21132487;
    let step_cell = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), a.x > a.y);
    let b = a - step_cell + vec2<f32>(0.21132487);
    let c = a - vec2<f32>(0.57735026);
    return 70.0 * (gradient_corner(cell, a) + gradient_corner(cell + step_cell, b)
        + gradient_corner(cell + vec2<f32>(1.0), c));
}

fn flowing_layer(point: vec2<f32>, scale: f32, drift: vec2<f32>, axis: vec2<f32>) -> vec2<f32> {
    let rotation = mat2x2<f32>(axis, vec2<f32>(-axis.y, axis.x));
    let coordinate = rotation * point * scale + drift * scene.camera_time.w;
    let footprint = max(length(dpdx(coordinate)), length(dpdy(coordinate)));
    let resolved = 1.0 - smoothstep(0.12, 0.55, footprint);
    // Transform the sampled slope back to world space before combining layers.
    return transpose(rotation) * slope_noise(coordinate) * resolved;
}

fn flowing_ripples(point: vec2<f32>) -> vec2<f32> {
    var slope = flowing_layer(point, 0.65, vec2<f32>(0.10, 0.04), vec2<f32>(0.8, 0.6)) * 0.028;
    slope += flowing_layer(point + vec2<f32>(13.7, 4.2), 1.51, vec2<f32>(-0.08, 0.13), vec2<f32>(0.6, -0.8)) * 0.018;
    slope += flowing_layer(point + vec2<f32>(-6.2, 19.8), 3.73, vec2<f32>(0.17, 0.09), vec2<f32>(0.3846154, 0.9230769)) * 0.010;
    slope += flowing_layer(point + vec2<f32>(21.1, -8.3), 8.91, vec2<f32>(-0.14, -0.19), vec2<f32>(-0.9230769, 0.3846154)) * 0.005;
    return slope;
}

fn detailed_normal(point: vec2<f32>, world: vec3<f32>, distance: f32, amplitude_scale: f32) -> vec3<f32> {
    // Evaluate at the interpolated undisplaced coordinate, not the displaced XZ position.
    let a = wave(point, vec2<f32>(0.9, 0.35), 38.0, 0.75, amplitude_scale);
    let b = wave(point, vec2<f32>(-0.4, 0.9), 19.0, 0.38, amplitude_scale);
    let c = wave(point, vec2<f32>(0.7, -0.6), 11.0, 0.22, amplitude_scale);
    let d = wave(point, vec2<f32>(-0.8, -0.2), 7.0, 0.12, amplitude_scale);
    let tx = vec3<f32>(1.0, 0.0, 0.0) + a.tangent_x + b.tangent_x + c.tangent_x + d.tangent_x;
    let tz = vec3<f32>(0.0, 0.0, 1.0) + a.tangent_z + b.tangent_z + c.tangent_z + d.tangent_z;
    let base = normalize(cross(tz, tx));
    let slope = flowing_ripples(world.xz) * scene.surface.y * scene.water.x
        * (1.0 - smoothstep(55.0, 115.0, distance));
    return normalize(base - vec3<f32>(slope.x, 0.0, slope.y) * base.y);
}

// GGX distribution, correlated Smith visibility, and Schlick dielectric Fresnel.
fn sun_glint(normal: vec3<f32>, view: vec3<f32>, alpha: f32) -> vec3<f32> {
    let light = scene.sun.xyz;
    let h = normalize(view + light);
    let nv = max(dot(normal, view), 0.001);
    let nl = max(dot(normal, light), 0.0);
    let nh = max(dot(normal, h), 0.0);
    let vh = max(dot(view, h), 0.0);
    let a2 = alpha * alpha;
    let denominator = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / max(PI * denominator * denominator, 0.000001);
    let masking = 0.5 / max(nl * sqrt(nv * nv * (1.0 - a2) + a2)
        + nv * sqrt(nl * nl * (1.0 - a2) + a2), 0.0001);
    let fresnel = 0.02037 + 0.97963 * pow(1.0 - vh, 5.0);
    return vec3<f32>(1.8, 1.58, 1.30) * distribution * masking * fresnel * nl;
}

// Alpha is coverage of reflected opaque geometry; clear pixels retain the procedural sky.
fn reflected_scene(world: vec3<f32>, normal: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    // The current reflection target is rendered for the primary plane only.
    if scene.reflection.x < 0.5 || (scene.secondary_water.z > 0.5 && abs(world.y - scene.secondary_water.y) < 0.45) {
        return fallback;
    }
    let plane_point = vec3<f32>(world.x, scene.water.w, world.z);
    let warped = plane_point + vec3<f32>(normal.x, 0.0, normal.z) * 0.25;
    let clip = scene.reflection_view_projection * vec4<f32>(warped, 1.0);
    if clip.w <= 0.0 { return fallback; }
    let uv = clip.xy / clip.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) { return fallback; }
    let sample_color = textureSampleLevel(reflection_color, color_sampler, uv, 0.0);
    let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    let fade = smoothstep(0.0, 0.025, edge);
    // Linear filtering of clear pixels produces premultiplied edge color.
    return fallback * (1.0 - sample_color.a * fade) + sample_color.rgb * fade;
}

@fragment fn fs_main(in: WaterVertex) -> @location(0) vec4<f32> {
    let view = normalize(scene.camera_time.xyz - in.world);
    let distance = length(scene.camera_time.xyz - in.world);
    let ripple_fade = 1.0 - smoothstep(15.0, 65.0, distance);
    let ripple = flowing_ripples(in.world.xz) * 0.5 * scene.water.x * ripple_fade;
    var normal = normalize(in.normal + vec3<f32>(ripple.x, 0.0, ripple.y));
    if scene.surface.x > 0.5 {
        normal = detailed_normal(in.wave_point, in.world, distance, in.surface_data.x);
    }
    if scene.waterfall_origin.w > 0.0 && in.surface_data.y < 0.5 {
        let spill = scene.waterfall_origin.xyz;
        let flow2 = normalize(vec2<f32>(scene.waterfall_shape.x, scene.waterfall_shape.y));
        let across2 = vec2<f32>(flow2.y, -flow2.x);
        let rel2 = in.world.xz - spill.xz;
        let along2 = dot(rel2, flow2);
        let lateral2 = dot(rel2, across2);
        let half_width2 = scene.waterfall_origin.w * 0.5;
        let channel2 = 1.0 - smoothstep(half_width2 * 0.8, half_width2 * 1.5, abs(lateral2));
        let approach2 = smoothstep(-2.8, -0.10, along2) * (1.0 - smoothstep(-0.10, 0.65, along2));
        let flow_strength = channel2 * approach2;
        let tf = scene.camera_time.w;
        let coarse = noise(vec2<f32>(lateral2 * 3.4 + tf * 0.19, along2 * 2.2 - tf * 0.31)) - 0.5;
        let medium = noise(vec2<f32>(lateral2 * 10.7 - tf * 0.41, along2 * 6.3 + tf * 0.67)) - 0.5;
        let fine = noise(vec2<f32>(lateral2 * 27.0 + tf * 0.83, along2 * 15.0 - tf * 1.07)) - 0.5;
        let cross_slope = coarse * 0.55 + medium * 0.32 + fine * 0.13;
        let along_slope = medium * 0.22 + fine * 0.10;
        normal = normalize(normal - vec3<f32>(
            flow2.x * flow_strength * (0.070 + along_slope)
                + across2.x * cross_slope * flow_strength * 0.15,
            0.0,
            flow2.y * flow_strength * (0.070 + along_slope)
                + across2.y * cross_slope * flow_strength * 0.15
        ));
    }
    // Unresolved normal variance broadens highlights rather than producing subpixel sparkles.
    let normal_dx = dpdx(normal);
    let normal_dy = dpdy(normal);
    let variance = min(0.04, 0.5 * (dot(normal_dx, normal_dx) + dot(normal_dy, normal_dy)));
    var alpha = sqrt(pow(scene.surface.z, 4.0) + variance);
    // The outlet contains unresolved turbulent slopes. Feed that variance into GGX
    // instead of letting a single broad mirror lobe paint a white stripe over the surface.
    if scene.waterfall_origin.w > 0.0 && in.surface_data.y < 0.5 {
        let spill_r = scene.waterfall_origin.xyz;
        let flow_r = normalize(vec2<f32>(scene.waterfall_shape.x, scene.waterfall_shape.y));
        let across_r = vec2<f32>(flow_r.y, -flow_r.x);
        let rel_r = in.world.xz - spill_r.xz;
        let along_r = dot(rel_r, flow_r);
        let lateral_r = dot(rel_r, across_r);
        let channel_r = 1.0 - smoothstep(
            scene.waterfall_origin.w * 0.38,
            scene.waterfall_origin.w * 0.82,
            abs(lateral_r)
        );
        let approach_r = smoothstep(-2.8, -0.15, along_r) * (1.0 - smoothstep(-0.15, 0.55, along_r));
        let turbulent_roughness = channel_r * approach_r;
        alpha = clamp(alpha + turbulent_roughness * 0.085, 0.025, 0.22);
    }
    let facing = max(dot(normal, view), 0.0);
    let fresnel = 0.02037 + (1.0 - 0.02037) * pow(1.0 - facing, 5.0);
    let visibility = shadow_visibility(in.world, normal);
    let light = scene.sun.xyz;
    let half_vector = normalize(light + view);
    let specular = pow(max(dot(normal, half_vector), 0.0), 220.0);
    let crest = smoothstep(-0.8, 1.3, in.world.y - in.surface_data.z);
    let deep = vec3<f32>(0.006, 0.075, 0.105);
    let teal = vec3<f32>(0.015, 0.20, 0.19);
    var scatter = mix(deep, teal, crest * 0.45) * (0.65 + 0.35 * max(dot(normal, light), 0.0) * visibility);

    if scene.surface.x > 0.5 {
        // Restrained body tint; most surface contrast now comes from reflection and transmission.
        scatter = vec3<f32>(0.004, 0.048, 0.065) * (0.8 + 0.2 * max(dot(normal, light), 0.0) * visibility);
        let backlight = pow(max(dot(view, -light), 0.0), 3.0);
        scatter += vec3<f32>(0.005, 0.045, 0.035) * crest * backlight * visibility;
    }

    let uv = in.clip.xy / vec2<f32>(textureDimensions(opaque_depth));
    let original_depth = scene_depth(uv);
    // A clear depth means sky, never a seabed: keep deep-water tint and zero foam.
    let has_bottom = original_depth < 0.999999 && original_depth > in.clip.z;
    var vertical_depth = 100.0;
    var body = scatter;
    if has_bottom {
        let bottom = world_at(uv, original_depth);
        vertical_depth = max(in.world.y - bottom.y, 0.0);
        if scene.effects.x > 0.5 && bottom.y < in.world.y {
            let thickness = min(length(bottom - in.world), 80.0);
            let ray = refract(-view, normal, 1.0 / 1.333);
            let refracted_point = in.world + ray * min(thickness, 12.0);
            let projected = scene.view_projection * vec4<f32>(refracted_point, 1.0);
            let refracted_uv = projected.xy / projected.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
            let offset = clamp((refracted_uv - uv) * scene.effects.y, vec2<f32>(-0.025), vec2<f32>(0.025))
                * smoothstep(0.0, 0.6, vertical_depth);
            let candidate = uv + offset;
            let candidate_depth = scene_depth(candidate);
            let candidate_world = world_at(candidate, candidate_depth);
            var sample_uv = uv;
            var sample_thickness = thickness;
            // Reject offscreen, sky, and foreground samples instead of pulling land across the water edge.
            if all(candidate >= vec2<f32>(0.0)) && all(candidate <= vec2<f32>(1.0))
                && candidate_depth < 0.999999 && candidate_depth > in.clip.z && candidate_world.y < in.world.y {
                sample_uv = candidate;
                sample_thickness = min(length(candidate_world - in.world), 80.0);
            }
            let transmission = exp(-scene.absorption.rgb * sample_thickness);
            let bottom_color = textureSampleLevel(opaque_color, color_sampler, sample_uv, 0.0).rgb;
            body = bottom_color * transmission + scatter * (vec3<f32>(1.0) - transmission);
        }
    }
    let reflected = reflect(-view, normal);
    var color = mix(body, reflected_scene(in.world, normal, sky_lighting(reflected, visibility)), fresnel);
    color += vec3<f32>(1.0, 0.78, 0.46) * specular * 4.0 * visibility;
    if scene.surface.x > 0.5 {
        // The direct sun is integrated by GGX, so exclude the sharp sky sun disk here.
        let reflection = reflected_scene(in.world, normal, sky_lighting(reflected, 0.0));
        color = mix(body, reflection, fresnel) + sun_glint(normal, view, alpha) * visibility;
    }

    // Use undistorted depth for a shoreline that stays attached to the actual terrain.
    let shore = (1.0 - smoothstep(0.0, scene.effects.w, vertical_depth)) * select(0.0, 1.0, has_bottom);
    let drift = scene.camera_time.w * vec2<f32>(0.24, -0.18);
    let bubbles = noise(in.world.xz * 2.8 + drift) * 0.65 + noise(in.world.xz * 6.1 - drift) * 0.35;
    let breaker = sin(vertical_depth * 8.0 - scene.camera_time.w * 1.8 + bubbles * 3.0) * 0.5 + 0.5;
    let foam_pattern = smoothstep(0.35, 0.72, bubbles * 0.7 + breaker * 0.3);
    let contact = 1.0 - smoothstep(0.0, min(0.12, scene.effects.w), vertical_depth);
    var foam = clamp(shore * (0.25 + 0.75 * foam_pattern) + contact * 0.3, 0.0, 1.0) * scene.effects.z;

    // Foam is generated by impact/compression and transported by the simulated velocity field.
    if in.surface_data.y > 0.5 {
        let sim = flow_sample(in.world.xz);
        let speed = length(sim.yz);
        let transported_foam = clamp(sim.w * (0.72 + speed * 0.16), 0.0, 1.0);
        foam = clamp(foam + transported_foam, 0.0, 1.0);
        // Turbulent flow perturbs the optical body without drawing a prescribed wake shape.
        let aeration = clamp(sim.w * 0.42 + speed * 0.035, 0.0, 0.55);
        color = mix(color, vec3<f32>(0.19, 0.31, 0.32) * (0.7 + 0.3 * visibility), aeration);
    }
    let foam_color = vec3<f32>(0.80, 0.88, 0.86) * (0.45 + 0.55 * visibility);
    color = mix(color, foam_color, foam);
    let haze = select(smoothstep(65.0, 120.0, length(in.world.xz - scene.camera_time.xz)),
        0.0, scene.water_bounds.z > 0.0);
    color = mix(color, sky(normalize(in.world - scene.camera_time.xyz)), haze);
    return vec4<f32>(color, 1.0);
}
