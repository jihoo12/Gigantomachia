struct Uniforms {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    light_view_projection: mat4x4<f32>,
    camera_time: vec4<f32>,
    water: vec4<f32>,
    sun: vec4<f32>,
    effects: vec4<f32>,
    absorption: vec4<f32>,
    surface: vec4<f32>,
    water_bounds: vec4<f32>,
    reflection_view_projection: mat4x4<f32>,
    reflection: vec4<f32>,
    secondary_water: vec4<f32>,
    secondary_bounds: vec4<f32>,
}
@group(0) @binding(0) var<uniform> scene: Uniforms;
@group(0) @binding(1) var shadow_depth: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

const PI: f32 = 3.14159265;

fn sky_lighting(direction: vec3<f32>, sun_visibility: f32) -> vec3<f32> {
    let up = clamp(direction.y, 0.0, 1.0);
    let horizon = vec3<f32>(0.58, 0.73, 0.78);
    let zenith = vec3<f32>(0.055, 0.23, 0.46);
    var color = mix(horizon, zenith, pow(up, 0.45));
    let sun_angle = max(dot(direction, scene.sun.xyz), 0.0);
    color += sun_visibility * vec3<f32>(1.0, 0.69, 0.36) * pow(sun_angle, 24.0) * 0.22;
    color += sun_visibility * vec3<f32>(6.0, 4.6, 2.9) * smoothstep(0.99965, 0.9999, sun_angle);
    return color;
}

fn sky(direction: vec3<f32>) -> vec3<f32> {
    return sky_lighting(direction, 1.0);
}

fn shadow_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    if scene.sun.w < 0.5 { return 1.0; }
    let clip = scene.light_view_projection * vec4<f32>(world + normal * 0.035, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z <= 0.0 || ndc.z >= 1.0 { return 1.0; }
    let texel = 1.0 / vec2<f32>(textureDimensions(shadow_depth));
    var visibility = 0.0;
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            visibility += textureSampleCompareLevel(shadow_depth, shadow_sampler,
                uv + vec2<f32>(f32(x), f32(y)) * texel, ndc.z - 0.00015);
        }
    }
    // Fade near the finite map border instead of abruptly dropping the shadow.
    let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    return mix(1.0, visibility / 9.0, smoothstep(0.0, 0.025, edge));
}
