struct Object {
    model: mat4x4<f32>,
    normal: mat4x4<f32>,
}
@group(1) @binding(0) var<uniform> object: Object;

@vertex fn vs_main(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return scene.light_view_projection * object.model * vec4<f32>(position, 1.0);
}
