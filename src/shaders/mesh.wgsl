struct Object {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
}
@group(1) @binding(0) var<uniform> object: Object;

struct MeshVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
}

@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>) -> MeshVertex {
    let world = object.model * vec4<f32>(position, 1.0);
    var out: MeshVertex;
    out.world = world.xyz;
    out.normal = normalize((object.normal * vec4<f32>(normal, 0.0)).xyz);
    out.color = color;
    out.clip = scene.view_projection * world;
    return out;
}

@fragment fn fs_main(in: MeshVertex) -> @location(0) vec4<f32> {
    // Clip only the mirrored pass; submerged geometry must not become a reflection.
    if scene.reflection.w > 0.5 && in.world.y < scene.reflection.y + 0.002 { discard; }
    let normal = normalize(in.normal);
    let diffuse = max(dot(normal, scene.sun.xyz), 0.0);
    let ambient = mix(vec3<f32>(0.18, 0.20, 0.18), vec3<f32>(0.42, 0.50, 0.57), normal.y * 0.5 + 0.5);
    var color = in.color * (ambient + vec3<f32>(1.0, 0.87, 0.68) * diffuse * 1.15 * shadow_visibility(in.world, normal));
    let ray = normalize(in.world - scene.camera_time.xyz);
    let haze = smoothstep(65.0, 120.0, length(in.world.xz - scene.camera_time.xz));
    color = mix(color, sky(ray), haze);
    return vec4<f32>(color, 1.0);
}
