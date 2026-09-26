struct FluidVertex {
    @builtin(position) clip:vec4<f32>,
    @location(0) world:vec3<f32>,
    @location(1) normal:vec3<f32>,
}
@vertex fn vs_main(@location(0) position:vec3<f32>,@location(1) normal:vec3<f32>,@location(2) color:vec3<f32>)->FluidVertex {
    var out:FluidVertex;
    out.clip=scene.view_projection*vec4<f32>(position,1.0);
    out.world=position;out.normal=normal;
    return out;
}
@group(1) @binding(0) var opaque_color:texture_2d<f32>;
@group(1) @binding(1) var color_sampler:sampler;
@group(1) @binding(2) var opaque_depth:texture_depth_2d;
@fragment fn fs_main(in:FluidVertex)->@location(0) vec4<f32> {
    let normal=normalize(in.normal);
    let view=normalize(scene.camera_time.xyz-in.world);
    let uv=in.clip.xy/vec2<f32>(textureDimensions(opaque_depth));
    let size=vec2<i32>(textureDimensions(opaque_depth));
    let offset=normal.xz*0.008;
    var sample_uv=clamp(uv+offset,vec2<f32>(0.001),vec2<f32>(0.999));
    let depth=textureLoad(opaque_depth,clamp(vec2<i32>(sample_uv*vec2<f32>(size)),vec2<i32>(0),size-vec2<i32>(1)),0);
    if depth<=in.clip.z {sample_uv=uv;}
    let background=textureSampleLevel(opaque_color,color_sampler,sample_uv,0.0).rgb;
    let fresnel=0.02037+0.97963*pow(1.0-max(dot(normal,view),0.0),5.0);
    let visibility=shadow_visibility(in.world,normal);
    let body=background*vec3<f32>(0.48,0.78,0.86)+vec3<f32>(0.004,0.032,0.045);
    let reflected=sky_lighting(reflect(-view,normal),visibility);
    let highlight=pow(max(dot(normal,normalize(view+scene.sun.xyz)),0.0),160.0)*visibility;
    return vec4<f32>(mix(body,reflected,fresnel)+vec3<f32>(0.7,0.8,0.85)*highlight,1.0);
}
