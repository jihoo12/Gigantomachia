// GPU-resident PBF. Each dispatch is a global dependency boundary; no in-place neighbor updates.
struct Params {
    counts: vec4<u32>, shape: vec4<u32>, origin: vec4<f32>, material: vec4<f32>,
}
struct Particle {
    position: vec4<f32>, previous: vec4<f32>, velocity: vec4<f32>,
    scratch: vec3<f32>, lambda: f32,
}
struct Box { minimum: vec4<f32>, maximum: vec4<f32> }
@group(0) @binding(0) var<uniform> cfg: Params;
@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(2) var<storage, read_write> heads: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> links: array<u32>;
@group(0) @binding(4) var<storage, read> boxes: array<Box>;
@group(0) @binding(5) var<storage, read_write> field: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> vertices: array<f32>;
@group(0) @binding(7) var<storage, read_write> args: array<atomic<u32>>;
const DT: f32 = 1.0 / 120.0;
const NIL: u32 = 0xffffffffu;
fn limited(v: vec3<f32>, cap: f32) -> vec3<f32> { return v * min(1.0, cap / max(length(v), 1e-12)); }
fn cell(p: vec3<f32>) -> vec3<i32> { return vec3<i32>(floor(p / (cfg.material.x * 2.0))); }
fn hash(c: vec3<i32>) -> u32 {
    let u = bitcast<vec3<u32>>(c);
    return ((u.x * 73856093u) ^ (u.y * 19349663u) ^ (u.z * 83492791u)) & (cfg.counts.z - 1u);
}
fn kernel(d: vec3<f32>, h: f32) -> vec4<f32> {
    let w = max(0.0, 1.0 - dot(d,d)/(h*h));
    return vec4<f32>(d * (-6.0*w*w/(h*h)), w*w*w);
}
fn project(previous: vec3<f32>, p: vec3<f32>, box: Box) -> vec3<f32> {
    let radius = cfg.material.x * 0.42;
    let low = box.minimum.xyz - vec3<f32>(radius);
    let high = box.maximum.xyz + vec3<f32>(radius);
    let delta = p - previous;
    var enter = 0.0;
    var exit = 1.0;
    var face = -1;
    var boundary = 0.0;
    var result = p;
    for (var axis = 0; axis < 3; axis++) {
        if abs(delta[axis]) < 1e-8 {
            if previous[axis] < low[axis] || previous[axis] > high[axis] { return p; }
        } else {
            let a = (low[axis] - previous[axis]) / delta[axis];
            let b = (high[axis] - previous[axis]) / delta[axis];
            let near = min(a,b);
            if near >= enter {
                enter = near;
                face = axis;
                boundary = select(high[axis]+1e-5, low[axis]-1e-5, delta[axis]>0.0);
            }
            exit = min(exit, max(a,b));
            if enter > exit { return p; }
        }
    }
    if face >= 0 { result[face] = boundary; }
    else if all(p >= low) && all(p <= high) {
        var best = 1e30;
        for (var axis = 0; axis < 3; axis++) {
            for (var side = 0; side < 2; side++) {
                let edge = select(low[axis]-1e-5, high[axis]+1e-5, side==1);
                let distance = abs(p[axis]-edge);
                if distance < best { best=distance; face=axis; boundary=edge; }
            }
        }
        result[face] = boundary;
    }
    return result;
}
fn collide(old: vec3<f32>, position: vec3<f32>) -> vec3<f32> {
    var p = position;
    for (var j=0u; j<cfg.counts.y; j++) { p = project(old, p, boxes[j]); }
    return p;
}
@compute @workgroup_size(64)
fn predict(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=cfg.counts.x { return; }
    let p=particles[i].position.xyz;
    let v=limited(particles[i].velocity.xyz+vec3<f32>(0.0,-9.81*DT,0.0), cfg.material.x*0.45/DT);
    particles[i].previous=vec4<f32>(p,0.0);
    particles[i].position=vec4<f32>(collide(p,p+v*DT),0.0);
}
@compute @workgroup_size(64)
fn clear_grid(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x<cfg.counts.z { atomicStore(&heads[id.x], NIL); }
}
@compute @workgroup_size(64)
fn build_grid(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=cfg.counts.x { return; }
    links[i]=atomicExchange(&heads[hash(cell(particles[i].position.xyz))],i);
}
// Hash collisions are filtered by the full cell coordinate, so no neighbor is counted twice.
// mode 0: density + squared gradient sum, 1: correction, 2: viscosity, 3: surface field.
fn gather(p: vec3<f32>, i: u32, mode: u32) -> vec4<f32> {
    let center=cell(p);
    let h=cfg.material.x*select(2.0,1.8,mode==3u);
    var total=vec4<f32>(0.0);
    var gradient=vec3<f32>(0.0);
    var squares=0.0;
    for (var z=-1; z<=1; z++) { for (var y=-1; y<=1; y++) { for (var x=-1; x<=1; x++) {
        let c=center+vec3<i32>(x,y,z);
        var j=atomicLoad(&heads[hash(c)]);
        loop {
            if j==NIL { break; }
            let other=particles[j].position.xyz;
            if all(cell(other)==c) {
                let k=kernel(p-other,h);
                switch mode {
                    case 0u: {
                        total.w += k.w;
                        if i!=j { let g=k.xyz/cfg.material.y; gradient+=g; squares+=dot(g,g); }
                    }
                    case 1u: {
                        if i!=j { total+=vec4<f32>((particles[i].lambda+particles[j].lambda)*k.xyz/cfg.material.y,0.0); }
                    }
                    case 2u: { total+=vec4<f32>(particles[j].velocity.xyz*k.w,k.w); }
                    default: { total+=vec4<f32>(-k.xyz,k.w); }
                }
            }
            j=links[j];
        }
    } } }
    if mode==0u { return vec4<f32>(squares+dot(gradient,gradient),0.0,0.0,total.w); }
    return total;
}
@compute @workgroup_size(64)
fn density(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=cfg.counts.x { return; }
    let sum=gather(particles[i].position.xyz,i,0u);
    let h=cfg.material.x*2.0;
    particles[i].lambda=-max(sum.w/cfg.material.y-1.0,0.0)/(sum.x+0.01/(h*h));
}
@compute @workgroup_size(64)
fn correct(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=cfg.counts.x { return; }
    particles[i].scratch=limited(gather(particles[i].position.xyz,i,1u).xyz,cfg.material.x*0.2);
}
@compute @workgroup_size(64)
fn apply_correction(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=cfg.counts.x { return; }
    particles[i].position=vec4<f32>(collide(particles[i].previous.xyz,particles[i].position.xyz+particles[i].scratch),0.0);
}
@compute @workgroup_size(64)
fn velocity(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i<cfg.counts.x { particles[i].velocity=(particles[i].position-particles[i].previous)/DT; }
}
@compute @workgroup_size(64)
fn viscosity(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=cfg.counts.x { return; }
    let sum=gather(particles[i].position.xyz,i,2u);
    particles[i].scratch=limited(mix(particles[i].velocity.xyz,sum.xyz/max(sum.w,1e-6),0.04),cfg.material.x*0.45/DT);
}
@compute @workgroup_size(64)
fn commit_velocity(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i<cfg.counts.x { particles[i].velocity=vec4<f32>(particles[i].scratch,0.0); }
}
fn coordinate(i: u32, shape: vec3<u32>) -> vec3<u32> { return vec3<u32>(i%shape.x,(i/shape.x)%shape.y,i/(shape.x*shape.y)); }
fn field_id(c: vec3<u32>) -> u32 { return (c.z*cfg.shape.y+c.y)*cfg.shape.x+c.x; }
fn point(c: vec3<u32>) -> vec3<f32> { return cfg.origin.xyz+vec3<f32>(c)*cfg.origin.w; }
@compute @workgroup_size(64)
fn sample_field(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=cfg.shape.x*cfg.shape.y*cfg.shape.z { return; }
    field[id.x]=gather(point(coordinate(id.x,cfg.shape.xyz)),0u,3u);
}
const CORNERS=array<vec3<u32>,8>(vec3<u32>(0,0,0),vec3<u32>(1,0,0),vec3<u32>(1,1,0),vec3<u32>(0,1,0),vec3<u32>(0,0,1),vec3<u32>(1,0,1),vec3<u32>(1,1,1),vec3<u32>(0,1,1));
const TETS=array<vec4<u32>,6>(vec4<u32>(0,5,1,6),vec4<u32>(0,1,2,6),vec4<u32>(0,2,3,6),vec4<u32>(0,3,7,6),vec4<u32>(0,7,4,6),vec4<u32>(0,4,5,6));
const EDGES=array<vec2<u32>,6>(vec2<u32>(0,1),vec2<u32>(1,2),vec2<u32>(2,0),vec2<u32>(0,3),vec2<u32>(1,3),vec2<u32>(2,3));
const TRIANGLES=array<u32,16>(0,1,1,2,1,2,2,1,1,2,2,1,2,1,1,0);
const TABLE=array<array<u32,6>,16>(
    array<u32,6>(0,0,0,0,0,0),array<u32,6>(0,2,3,0,0,0),array<u32,6>(0,1,4,0,0,0),array<u32,6>(1,2,3,1,3,4),
    array<u32,6>(1,2,5,0,0,0),array<u32,6>(0,1,5,0,5,3),array<u32,6>(0,2,5,0,5,4),array<u32,6>(3,4,5,0,0,0),
    array<u32,6>(3,4,5,0,0,0),array<u32,6>(0,2,5,0,5,4),array<u32,6>(0,1,5,0,5,3),array<u32,6>(1,2,5,0,0,0),
    array<u32,6>(1,2,3,1,3,4),array<u32,6>(0,1,4,0,0,0),array<u32,6>(0,2,3,0,0,0),array<u32,6>(0,0,0,0,0,0));
fn unit(v: vec3<f32>) -> vec3<f32> { return v/max(length(v),1e-12); }
fn emit_triangle(p: array<vec3<f32>,3>, n: array<vec3<f32>,3>) {
    let geometric=cross(p[1]-p[0],p[2]-p[0]);
    if dot(geometric,geometric)<1e-14 { return; }
    let reverse=dot(geometric,n[0]+n[1]+n[2])<0.0;
    let base=atomicAdd(&args[0],3u);
    if base+3u>cfg.counts.w { atomicStore(&args[4],1u); return; }
    for (var k=0u;k<3u;k++) {
        var source=k;
        if reverse && k>0u { source=3u-k; }
        let offset=(base+k)*9u;
        for (var a=0u;a<3u;a++) { vertices[offset+a]=p[source][a]; vertices[offset+3u+a]=n[source][a]; }
        vertices[offset+6u]=0.015; vertices[offset+7u]=0.16; vertices[offset+8u]=0.23;
    }
}
@compute @workgroup_size(64)
fn extract_surface(@builtin(global_invocation_id) id: vec3<u32>) {
    let shape=cfg.shape.xyz-vec3<u32>(1);
    if id.x>=shape.x*shape.y*shape.z { return; }
    let c=coordinate(id.x,shape);
    var samples: array<vec4<f32>,8>;
    var points: array<vec3<f32>,8>;
    var mask=0u;
    for (var k=0u;k<8u;k++) {
        samples[k]=field[field_id(c+CORNERS[k])]; points[k]=point(c+CORNERS[k]);
        mask |= u32(samples[k].w>=0.6)<<k;
    }
    if mask==0u || mask==255u { return; }
    for (var t=0u;t<6u;t++) {
        let tet=TETS[t]; var bits=0u;
        for (var k=0u;k<4u;k++) { bits|=u32(samples[tet[k]].w>=0.6)<<k; }
        for (var tri=0u;tri<TRIANGLES[bits];tri++) {
            var p: array<vec3<f32>,3>; var n: array<vec3<f32>,3>;
            for (var k=0u;k<3u;k++) {
                let edge=EDGES[TABLE[bits][tri*3u+k]];
                let a=tet[edge.x]; let b=tet[edge.y];
                let f=clamp((0.6-samples[a].w)/(samples[b].w-samples[a].w),0.0,1.0);
                p[k]=mix(points[a],points[b],f); n[k]=unit(mix(samples[a].xyz,samples[b].xyz,f));
            }
            emit_triangle(p,n);
        }
    }
}
@compute @workgroup_size(64)
fn finish_surface(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x!=0u { return; }
    atomicStore(&args[0],min(atomicLoad(&args[0]),cfg.counts.w));
    atomicStore(&args[1],1u);
}
@compute @workgroup_size(64)
fn surface_diagnostics(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=cfg.counts.x { return; }
    let p=particles[id.x].position.xyz;
    let radius=vec3<f32>(cfg.material.x*1.8);
    if any(p-radius<cfg.origin.xyz) || any(p+radius>point(cfg.shape.xyz-vec3<u32>(1))) {
        atomicAdd(&args[5],1u);
    }
}
