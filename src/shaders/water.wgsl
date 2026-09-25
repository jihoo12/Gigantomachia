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
    let q = 0.45;
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
}

@vertex fn vs_main(@location(0) grid: vec2<f32>) -> WaterVertex {
    let point = grid + scene.water.yz;
    let a = wave(point, vec2<f32>(0.9, 0.35), 38.0, 0.75);
    let b = wave(point, vec2<f32>(-0.4, 0.9), 19.0, 0.38);
    let c = wave(point, vec2<f32>(0.7, -0.6), 11.0, 0.22);
    let d = wave(point, vec2<f32>(-0.8, -0.2), 7.0, 0.12);
    let world = vec3<f32>(point.x, scene.water.w, point.y) + a.displacement + b.displacement + c.displacement + d.displacement;
    let tx = vec3<f32>(1.0, 0.0, 0.0) + a.tangent_x + b.tangent_x + c.tangent_x + d.tangent_x;
    let tz = vec3<f32>(0.0, 0.0, 1.0) + a.tangent_z + b.tangent_z + c.tangent_z + d.tangent_z;
    var out: WaterVertex;
    out.world = world;
    out.normal = normalize(cross(tz, tx));
    out.clip = scene.view_projection * vec4<f32>(world, 1.0);
    return out;
}

@group(1) @binding(0) var opaque_color: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
@group(1) @binding(2) var opaque_depth: texture_depth_2d;

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

@fragment fn fs_main(in: WaterVertex) -> @location(0) vec4<f32> {
    let view = normalize(scene.camera_time.xyz - in.world);
    let distance = length(scene.camera_time.xyz - in.world);
    let ripple_fade = 1.0 - smoothstep(15.0, 65.0, distance);
    let ripple = vec2<f32>(
        cos(dot(in.world.xz, vec2<f32>(2.1, 1.6)) - scene.camera_time.w * 2.3),
        cos(dot(in.world.xz, vec2<f32>(-1.3, 2.6)) - scene.camera_time.w * 1.8)
    ) * 0.045 * scene.water.x * ripple_fade;
    let normal = normalize(in.normal + vec3<f32>(ripple.x, 0.0, ripple.y));
    let facing = max(dot(normal, view), 0.0);
    let fresnel = 0.02037 + (1.0 - 0.02037) * pow(1.0 - facing, 5.0);
    let visibility = shadow_visibility(in.world, normal);
    let light = scene.sun.xyz;
    let half_vector = normalize(light + view);
    let specular = pow(max(dot(normal, half_vector), 0.0), 220.0);
    let crest = smoothstep(-0.8, 1.3, in.world.y - scene.water.w);
    let deep = vec3<f32>(0.006, 0.075, 0.105);
    let teal = vec3<f32>(0.015, 0.20, 0.19);
    let scatter = mix(deep, teal, crest * 0.45) * (0.65 + 0.35 * max(dot(normal, light), 0.0) * visibility);

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
    var color = mix(body, sky_lighting(reflected, visibility), fresnel);
    color += vec3<f32>(1.0, 0.78, 0.46) * specular * 4.0 * visibility;

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
    let haze = smoothstep(65.0, 120.0, length(in.world.xz - scene.camera_time.xz));
    color = mix(color, sky(normalize(in.world - scene.camera_time.xyz)), haze);
    return vec4<f32>(color, 1.0);
}
