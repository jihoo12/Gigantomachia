struct SpillVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) @interpolate(flat) kind: u32,
    @location(4) fade: f32,
}
fn random(seed: f32) -> f32 { return fract(sin(seed * 127.1 + 31.7) * 43758.5453); }
fn corner(index: u32) -> vec2<f32> {
    let corners = array<vec2<f32>, 6>(vec2<f32>(0.0,0.0), vec2<f32>(1.0,0.0), vec2<f32>(1.0,1.0), vec2<f32>(0.0,0.0), vec2<f32>(1.0,1.0), vec2<f32>(0.0,1.0));
    return corners[index];
}
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SpillVertex {
    let origin = scene.waterfall_origin.xyz;
    let width = scene.waterfall_origin.w;
    let forward = vec3<f32>(scene.waterfall_shape.x, 0.0, scene.waterfall_shape.y);
    let across = vec3<f32>(forward.z, 0.0, -forward.x);
    let drop = scene.waterfall_shape.z;
    let flight = sqrt(2.0 * drop / 9.81);
    let landing = origin + forward * (1.3 * flight) - vec3<f32>(0.0,drop,0.0);
    let time = scene.camera_time.w;
    var out: SpillVertex;
    out.fade = 1.0;
    out.normal = vec3<f32>(0.0,1.0,0.0);
    if index < 288u {
        out.kind = 0u;
        let slice = index / 3u;
        let vertex = index % 3u;
        let angle = (f32(slice) + select(0.0,1.0,vertex == 2u)) * (2.0 * PI / 96.0);
        let radius = select(1.0,0.0,vertex == 0u);
        out.uv = vec2<f32>(cos(angle),sin(angle)) * radius;
        let uneven = 1.0 + 0.04 * sin(angle*5.0) + 0.025*cos(angle*9.0);
        out.world = landing + (across*out.uv.x*(width*0.5+0.85) + forward*out.uv.y*1.15)*uneven;
    } else if index < 6432u {
        out.kind = 1u;
        let i = index - 288u;
        let cell = i / 6u;
        let uv = (vec2<f32>(f32(cell%32u),f32(cell/32u)) + corner(i%6u)) / 32.0;
        let t = uv.y * flight;
        let speed = 9.81*t;
        let flutter = sin(uv.x*19.0 + time*4.2 - uv.y*9.0) * sin(uv.y*PI) * 0.025;
        out.world = origin + across*((uv.x-0.5)*width*(1.0-0.12*uv.y))
            + forward*(1.3*t + flutter) - vec3<f32>(0.0,0.5*9.81*t*t,0.0);
        out.normal = normalize(forward*speed + vec3<f32>(0.0,1.3,0.0));
        out.uv = uv;
    } else {
        out.kind = 2u;
        let i = index-6432u;
        let id = f32(i/6u);
        let age = fract(time*(1.3+random(id+5.0)*0.4) + random(id));
        let t = age*0.42;
        let angle = random(id+17.0)*2.0*PI;
        let velocity = across*cos(angle) + forward*sin(angle);
        let center = landing + across*((random(id+3.0)-0.5)*width*0.9)
            + velocity*t*(0.6+random(id+8.0)) + vec3<f32>(0.0,1.9*t-4.905*t*t,0.0);
        let view = normalize(scene.camera_time.xyz-center);
        let right = normalize(cross(vec3<f32>(0.0,1.0,0.001),view));
        let up = normalize(cross(view,right));
        let uv = corner(i%6u)*2.0-vec2<f32>(1.0);
        out.world = center + (right*uv.x + up*uv.y*1.5)*(0.012+random(id+1.0)*0.018);
        out.normal = view;
        out.uv = uv;
        out.fade = sin(age*PI);
    }
    out.clip = scene.view_projection * vec4<f32>(out.world,1.0);
    return out;
}
@fragment fn fs_main(in: SpillVertex) -> @location(0) vec4<f32> {
    let time = scene.camera_time.w;
    let view = normalize(scene.camera_time.xyz-in.world);
    let visibility = shadow_visibility(in.world,in.normal);
    if in.kind == 2u {
        let opacity = (1.0-smoothstep(0.45,1.0,length(in.uv)))*in.fade*0.75;
        return vec4<f32>(vec3<f32>(0.65,0.85,0.9)*(0.6+0.4*visibility),opacity);
    }
    if in.kind == 0u {
        let radius = length(in.uv);
        let edge = 1.0-smoothstep(0.78,1.0,radius);
        let rings = sin(radius*32.0-time*6.0);
        let normal = normalize(vec3<f32>(in.uv.x*rings*0.07,1.0,in.uv.y*rings*0.07));
        let fresnel = 0.02+0.98*pow(1.0-max(dot(normal,view),0.0),5.0);
        let foam = exp(-dot(in.uv*vec2<f32>(1.0,3.0),in.uv*vec2<f32>(1.0,3.0))*5.0)
            * (0.65+0.35*sin(in.uv.x*53.0+time*4.0)*sin(in.uv.y*71.0-time*7.0));
        let color = mix(mix(vec3<f32>(0.015,0.13,0.16),sky(reflect(-view,normal)),fresnel),vec3<f32>(0.65,0.8,0.81)*visibility,foam);
        return vec4<f32>(color,edge*(0.35+foam*0.5));
    }
    let stream = sin(in.uv.x*91.0 + sin(in.uv.y*9.0-time*5.0)*1.7)*0.5+0.5;
    let streak = pow(stream,5.0);
    var normal = normalize(in.normal + vec3<f32>(sin(in.uv.x*39.0-time*2.0)*0.06,0.0,0.0));
    if dot(normal,view)<0.0 { normal = -normal; }
    let fresnel = 0.025+0.975*pow(1.0-max(dot(normal,view),0.0),5.0);
    let rim = pow(abs(in.uv.x*2.0-1.0),12.0);
    let color = mix(vec3<f32>(0.04,0.24,0.29),sky(reflect(-view,normal)),0.35+0.65*fresnel)
        + vec3<f32>(0.25,0.31,0.32)*(streak*0.3+rim*0.4)*visibility;
    return vec4<f32>(color,clamp(0.22+fresnel*0.4+streak*0.22+rim*0.2,0.0,0.85));
}
