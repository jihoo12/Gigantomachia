// First 3D fluid-world milestone: particles are the simulation state.
// This deliberately does not know about waterfalls, upper/lower water, or ribbons.
struct Params {
    dt_gravity: vec4<f32>,
    emitter_min: vec4<f32>,
    emitter_max: vec4<f32>,
    emitter_velocity: vec4<f32>,
    counts: vec4<u32>,
};
struct Aabb { min: vec4<f32>, max: vec4<f32> };
@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> vel_src: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> vel_dst: array<vec4<f32>>;
@group(0) @binding(4) var<uniform> p: Params;
@group(0) @binding(5) var<storage, read> colliders: array<Aabb>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=p.counts.y { return; }
    let fx=f32(i%32u)/31.0; let fy=f32((i/32u)%8u)/7.0; let fz=f32((i/256u)%16u)/15.0;
    let spawn=mix(p.emitter_min.xyz,p.emitter_max.xyz,vec3<f32>(fx,fy,fz));
    var pos=src[i].xyz; var vel=vel_src[i].xyz;
    if src[i].w<0.5 { pos=spawn; vel=p.emitter_velocity.xyz; }
    vel.y-=p.dt_gravity.y*p.dt_gravity.x; pos+=vel*p.dt_gravity.x;
    let r=p.dt_gravity.z;
    for(var c=0u;c<p.counts.x;c++) {
        let lo=colliders[c].min.xyz-vec3<f32>(r); let hi=colliders[c].max.xyz+vec3<f32>(r);
        if all(pos>lo) && all(pos<hi) {
            let a=pos-lo; let b=hi-pos; var axis=0u; var side=-1.0; var depth=a.x;
            if b.x<depth { depth=b.x; side=1.0; }
            if a.y<depth { depth=a.y; axis=1u; side=-1.0; }
            if b.y<depth { depth=b.y; axis=1u; side=1.0; }
            if a.z<depth { depth=a.z; axis=2u; side=-1.0; }
            if b.z<depth { axis=2u; side=1.0; }
            var n=vec3<f32>(0.0); n[axis]=side;
            if side<0.0 { pos[axis]=lo[axis]; } else { pos[axis]=hi[axis]; }
            let vn=dot(vel,n); if vn<0.0 { vel-=n*vn*(1.0+p.dt_gravity.w); }
            let normal=n*dot(vel,n); vel=normal+(vel-normal)*0.86;
        }
    }
    if pos.y < -0.5 || abs(pos.x)>12.0 || abs(pos.z)>14.0 { pos=spawn; vel=p.emitter_velocity.xyz; }
    dst[i]=vec4<f32>(pos,1.0); vel_dst[i]=vec4<f32>(vel,0.0);
}
