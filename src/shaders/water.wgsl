struct WaveSample {
    displacement: vec3<f32>,
    tangent_x: vec3<f32>,
    tangent_z: vec3<f32>,
}

// Analytic derivatives of Gerstner displacement, summed before taking the normal.
fn wave(point: vec2<f32>, direction: vec2<f32>, wavelength: f32, height: f32) -> WaveSample {
    let d = normalize(direction);
    let k = 2.0 * PI / wavelength;
    let amplitude = height * scene.water.x;
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
}

@vertex fn vs_main(@location(0) grid: vec2<f32>) -> WaterVertex {
    var point = grid + scene.water.yz;
    if scene.water_bounds.z > 0.0 {
        point = scene.water_bounds.xy + (grid / 128.0) * scene.water_bounds.zw;
    }
    let a = wave(point, vec2<f32>(0.9, 0.35), 38.0, 0.75);
    let b = wave(point, vec2<f32>(-0.4, 0.9), 19.0, 0.38);
    let c = wave(point, vec2<f32>(0.7, -0.6), 11.0, 0.22);
    let d = wave(point, vec2<f32>(-0.8, -0.2), 7.0, 0.12);
    let world = vec3<f32>(point.x, scene.water.w, point.y) + a.displacement + b.displacement + c.displacement + d.displacement;
    let tx = vec3<f32>(1.0, 0.0, 0.0) + a.tangent_x + b.tangent_x + c.tangent_x + d.tangent_x;
    let tz = vec3<f32>(0.0, 0.0, 1.0) + a.tangent_z + b.tangent_z + c.tangent_z + d.tangent_z;
    var out: WaterVertex;
    out.world = world;
    out.wave_point = point;
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

fn detailed_normal(point: vec2<f32>, world: vec3<f32>, distance: f32) -> vec3<f32> {
    // Evaluate at the interpolated undisplaced coordinate, not the displaced XZ position.
    let a = wave(point, vec2<f32>(0.9, 0.35), 38.0, 0.75);
    let b = wave(point, vec2<f32>(-0.4, 0.9), 19.0, 0.38);
    let c = wave(point, vec2<f32>(0.7, -0.6), 11.0, 0.22);
    let d = wave(point, vec2<f32>(-0.8, -0.2), 7.0, 0.12);
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
    if scene.reflection.x < 0.5 { return fallback; }
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
        normal = detailed_normal(in.wave_point, in.world, distance);
    }
    // Unresolved normal variance broadens highlights rather than producing subpixel sparkles.
    let normal_dx = dpdx(normal);
    let normal_dy = dpdy(normal);
    let variance = min(0.04, 0.5 * (dot(normal_dx, normal_dx) + dot(normal_dy, normal_dy)));
    let alpha = sqrt(pow(scene.surface.z, 4.0) + variance);
    let facing = max(dot(normal, view), 0.0);
    let fresnel = 0.02037 + (1.0 - 0.02037) * pow(1.0 - facing, 5.0);
    let visibility = shadow_visibility(in.world, normal);
    let light = scene.sun.xyz;
    let half_vector = normalize(light + view);
    let specular = pow(max(dot(normal, half_vector), 0.0), 220.0);
    let crest = smoothstep(-0.8, 1.3, in.world.y - scene.water.w);
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
    let foam = clamp(shore * (0.25 + 0.75 * foam_pattern) + contact * 0.3, 0.0, 1.0) * scene.effects.z;
    let foam_color = vec3<f32>(0.80, 0.88, 0.86) * (0.45 + 0.55 * visibility);
    color = mix(color, foam_color, foam);
    let haze = select(smoothstep(65.0, 120.0, length(in.world.xz - scene.camera_time.xz)),
        0.0, scene.water_bounds.z > 0.0);
    color = mix(color, sky(normalize(in.world - scene.camera_time.xyz)), haze);
    return vec4<f32>(color, 1.0);
}
