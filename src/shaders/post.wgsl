@group(0) @binding(0) var hdr: texture_2d<f32>;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment fn fs_main(@builtin(position) pixel: vec4<f32>) -> @location(0) vec4<f32> {
    let linear = textureLoad(hdr, vec2<i32>(pixel.xy), 0).rgb;
    // Tone-map exactly once after water composition; the sRGB target handles encoding.
    return vec4<f32>(vec3<f32>(1.0) - exp(-max(linear, vec3<f32>(0.0)) * 1.15), 1.0);
}
