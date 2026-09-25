struct Uniforms {
    view_projection: mat4x4<f32>,
    inverse_view_projection: mat4x4<f32>,
    camera_time: vec4<f32>,
    water: vec4<f32>,
}
@group(0) @binding(0) var<uniform> scene: Uniforms;

const PI: f32 = 3.14159265;
const SUN: vec3<f32> = vec3<f32>(-0.36, 0.27, -0.893);

fn sky(direction: vec3<f32>) -> vec3<f32> {
    let up = clamp(direction.y, 0.0, 1.0);
    let horizon = vec3<f32>(0.58, 0.73, 0.78);
    let zenith = vec3<f32>(0.055, 0.23, 0.46);
    var color = mix(horizon, zenith, pow(up, 0.45));
    let sun_angle = max(dot(direction, normalize(SUN)), 0.0);
    color += vec3<f32>(1.0, 0.69, 0.36) * pow(sun_angle, 24.0) * 0.22;
    color += vec3<f32>(6.0, 4.6, 2.9) * smoothstep(0.99965, 0.9999, sun_angle);
    return color;
}

// Simple exposure compression, then the sRGB render target performs encoding.
fn tone_map(color: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(1.0) - exp(-color * 1.15);
}
