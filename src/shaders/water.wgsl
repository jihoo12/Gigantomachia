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

@fragment fn fs_main(in: WaterVertex) -> @location(0) vec4<f32> {
    let view = normalize(scene.camera_time.xyz - in.world);
    let distance = length(scene.camera_time.xyz - in.world);
    // Fade short ripples before they become subpixel at the horizon.
    let ripple_fade = 1.0 - smoothstep(15.0, 65.0, distance);
    let ripple = vec2<f32>(
        cos(dot(in.world.xz, vec2<f32>(2.1, 1.6)) - scene.camera_time.w * 2.3),
        cos(dot(in.world.xz, vec2<f32>(-1.3, 2.6)) - scene.camera_time.w * 1.8)
    ) * 0.045 * scene.water.x * ripple_fade;
    let normal = normalize(in.normal + vec3<f32>(ripple.x, 0.0, ripple.y));
    let facing = max(dot(normal, view), 0.0);
    let fresnel = 0.02037 + (1.0 - 0.02037) * pow(1.0 - facing, 5.0);
    let reflected = reflect(-view, normal);
    let light = normalize(SUN);
    let half_vector = normalize(light + view);
    let specular = pow(max(dot(normal, half_vector), 0.0), 220.0);
    let crest = smoothstep(-0.8, 1.3, in.world.y - scene.water.w);
    let deep = vec3<f32>(0.006, 0.075, 0.105);
    let teal = vec3<f32>(0.015, 0.20, 0.19);
    let body = mix(deep, teal, crest * 0.45) * (0.65 + 0.35 * max(dot(normal, light), 0.0));
    var color = mix(body, sky(reflected), fresnel);
    color += vec3<f32>(1.0, 0.78, 0.46) * specular * 4.0;
    // The finite camera-centered patch disappears into matching horizon haze.
    let haze = smoothstep(65.0, 120.0, length(in.world.xz - scene.camera_time.xz));
    color = mix(color, sky(normalize(in.world - scene.camera_time.xyz)), haze);
    return vec4<f32>(tone_map(color), 1.0);
}
