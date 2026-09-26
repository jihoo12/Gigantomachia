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
@group(0) @binding(6) var<storage, read_write> grid_counts: array<atomic<u32>>;
@group(0) @binding(7) var<storage, read_write> grid_particles: array<u32>;
@group(0) @binding(8) var<storage, read_write> density: array<f32>;
@group(0) @binding(9) var<storage, read_write> emitter_state: array<atomic<u32>>;

const GRID_DIM = vec3<u32>(64u, 32u, 96u);
const GRID_ORIGIN = vec3<f32>(-8.0, -1.0, -4.0);
const CELL_SIZE = 0.16;

fn grid_cell(pos: vec3<f32>) -> vec3<u32> {
    let c=vec3<i32>(floor((pos-GRID_ORIGIN)/CELL_SIZE));
    return vec3<u32>(clamp(c,vec3<i32>(0),vec3<i32>(GRID_DIM)-vec3<i32>(1)));
}
fn grid_index(c: vec3<u32>) -> u32 { return c.x + GRID_DIM.x*(c.y + GRID_DIM.y*c.z); }

@compute @workgroup_size(64)
fn clear_grid(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x < 196608u { atomicStore(&grid_counts[id.x],0u); }
}

@compute @workgroup_size(64)
fn insert_grid(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=p.counts.y || src[i].w<0.5 { return; }
    let cell=grid_index(grid_cell(src[i].xyz));
    let slot=atomicAdd(&grid_counts[cell],1u);
    if slot<32u { grid_particles[cell*32u+slot]=i; }
}

fn in_grid(c: vec3<i32>) -> bool {
    return all(c >= vec3<i32>(0)) && all(c < vec3<i32>(GRID_DIM));
}

@compute @workgroup_size(64)
fn compute_density(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=p.counts.y || src[i].w<0.5 { return; }
    let xi=src[i].xyz;
    let base=vec3<i32>(floor((xi-GRID_ORIGIN)/CELL_SIZE));
    let h=CELL_SIZE;
    var rho=0.0;
    for(var dz=-1;dz<=1;dz++) {
        for(var dy=-1;dy<=1;dy++) {
            for(var dx=-1;dx<=1;dx++) {
                let cell=base+vec3<i32>(dx,dy,dz);
                if in_grid(cell) {
                    let ci=grid_index(vec3<u32>(cell));
                    let n=min(atomicLoad(&grid_counts[ci]),32u);
                    for(var slot=0u;slot<n;slot++) {
                        let j=grid_particles[ci*32u+slot];
                        let q=length(xi-src[j].xyz)/h;
                        if q<1.0 {
                            let w=1.0-q;
                            rho+=w*w*w;
                        }
                    }
                }
            }
        }
    }
    density[i]=rho;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; if i>=p.counts.y { return; }
    let fx=f32(i%32u)/31.0; let fy=f32((i/32u)%8u)/7.0; let fz=f32((i/256u)%16u)/15.0;
    let spawn=mix(p.emitter_min.xyz,p.emitter_max.xyz,vec3<f32>(fx,fy,fz));
    var pos=src[i].xyz; var vel=vel_src[i].xyz;
    var emit_now=false;
    if src[i].w<0.5 {
        let ticket=atomicAdd(&emitter_state[0],1u);
        if ticket<atomicLoad(&emitter_state[1]) {
            emit_now=true;
            pos=spawn;
            vel=p.emitter_velocity.xyz;
        } else {
            dst[i]=vec4<f32>(0.0);
            vel_dst[i]=vec4<f32>(0.0);
            return;
        }
    }

    // Weakly-compressible SPH pressure. Density is dimensionless for now because the
    // kernel is normalized only relative to particle spacing; rest density therefore
    // uses the same scale instead of pretending to be kg/m^3.
    if src[i].w>=0.5 && !emit_now {
        let xi=src[i].xyz;
        let base=vec3<i32>(floor((xi-GRID_ORIGIN)/CELL_SIZE));
        let h=CELL_SIZE;
        let rest_density=4.0;
        let stiffness=7.0;
        let pressure_i=max(density[i]-rest_density,0.0)*stiffness;
        var pressure_accel=vec3<f32>(0.0);
        var viscosity_delta=vec3<f32>(0.0);
        var viscosity_weight=0.0;
        for(var dz=-1;dz<=1;dz++) {
            for(var dy=-1;dy<=1;dy++) {
                for(var dx=-1;dx<=1;dx++) {
                    let cell=base+vec3<i32>(dx,dy,dz);
                    if in_grid(cell) {
                        let ci=grid_index(vec3<u32>(cell));
                        let n=min(atomicLoad(&grid_counts[ci]),32u);
                        for(var slot=0u;slot<n;slot++) {
                            let j=grid_particles[ci*32u+slot];
                            if j!=i {
                                let delta=xi-src[j].xyz;
                                let dist=length(delta);
                                if dist>0.0001 && dist<h {
                                    let pressure_j=max(density[j]-rest_density,0.0)*stiffness;
                                    let grad=(1.0-dist/h)*(1.0-dist/h);
                                    let rho_j=max(density[j],1.0);
                                    pressure_accel+=(delta/dist)*((pressure_i+pressure_j)*0.5/rho_j)*grad;
                                    let visc_w=1.0-dist/h;
                                    viscosity_delta+=(vel_src[j].xyz-vel)*visc_w;
                                    viscosity_weight+=visc_w;
                                }
                            }
                        }
                    }
                }
            }
        }
        let max_pressure_accel=35.0;
        let a_len=length(pressure_accel);
        if a_len>max_pressure_accel { pressure_accel*=max_pressure_accel/a_len; }
        vel+=pressure_accel*p.dt_gravity.x;
        if viscosity_weight>0.0 {
            // XSPH-style velocity smoothing: damp local jitter without erasing bulk flow.
            vel+=viscosity_delta/viscosity_weight*0.12;
        }
    }
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
            let vn=dot(vel,n);
            if vn<0.0 {
                // Water should settle and slide on solids instead of bouncing like a ball.
                vel-=n*vn;
            }
            let normal=n*dot(vel,n);
            vel=normal+(vel-normal)*0.94;
        }
    }
    if pos.y < -0.5 || abs(pos.x)>12.0 || abs(pos.z)>14.0 { dst[i]=vec4<f32>(0.0); vel_dst[i]=vec4<f32>(0.0); return; }
    dst[i]=vec4<f32>(pos,1.0); vel_dst[i]=vec4<f32>(vel,0.0);
}
