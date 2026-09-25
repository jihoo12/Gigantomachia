struct SkyVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SkyVertex {
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var out: SkyVertex;
    out.ndc = positions[index];
    out.clip = vec4<f32>(out.ndc, 0.9999, 1.0);
    return out;
}

@fragment fn fs_main(in: SkyVertex) -> @location(0) vec4<f32> {
    let world = scene.inverse_view_projection * vec4<f32>(in.ndc, 1.0, 1.0);
    let ray = normalize(world.xyz / world.w - scene.camera_time.xyz);
    return vec4<f32>(tone_map(sky(ray)), 1.0);
}
