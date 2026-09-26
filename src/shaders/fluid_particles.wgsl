// First 3D fluid-world milestone: particles are the simulation state.
// This deliberately does not know about waterfalls, upper/lower water, or ribbons.
struct Params {
    dt_gravity: vec4<f32>,      // dt, gravity, particle radius, restitution
    emitter: vec4<f32>,         // xyz position, spawn rate
    emitter_dir: vec4<f32>,     // xyz initial velocity, active particle count
    board: vec4<f32>,           // center x, top y, center z, half x
    board2: vec4<f32>,          // half z, lower top y, lower center z, lower half x
    lower: vec4<f32>,           // lower half z, damping, time, reserved
};
@group(0) @binding(0) var<storage, read> src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> vel_src: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> vel_dst: array<vec4<f32>>;
@group(0) @binding(4) var<uniform> p: Params;

fn collide_plane(pos0: vec3<f32>, vel0: vec3<f32>, top: f32, cx: f32, cz: f32, hx: f32, hz: f32) -> vec4<f32> {
    var pos=pos0; var vel=vel0;
    let inside=abs(pos.x-cx)<hx && abs(pos.z-cz)<hz;
    if inside && pos.y < top+p.dt_gravity.z && pos.y > top-0.20 && vel.y<0.0 {
        pos.y=top+p.dt_gravity.z;
        vel.y=-vel.y*p.dt_gravity.w;
        vel.xz*=p.lower.y;
    }
    return vec4<f32>(pos,0.0)+vec4<f32>(vel,0.0)*0.0;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i=id.x; let count=u32(p.emitter_dir.w);
    if i>=count { return; }
    let dt=p.dt_gravity.x;
    var pos=src[i].xyz;
    var vel=vel_src[i].xyz;
    vel.y-=p.dt_gravity.y*dt;
    pos+=vel*dt;

    // The same collision rule handles both boards. If a particle crosses an edge,
    // there is simply no supporting plane and gravity keeps acting.
    let upper_inside=abs(pos.x-p.board.x)<p.board.w && abs(pos.z-p.board.z)<p.board2.x;
    if upper_inside && pos.y<p.board.y+p.dt_gravity.z && pos.y>p.board.y-0.20 && vel.y<0.0 {
        pos.y=p.board.y+p.dt_gravity.z; vel.y=-vel.y*p.dt_gravity.w; vel.xz*=p.lower.y;
    }
    let lower_inside=abs(pos.x)<p.board2.w && abs(pos.z-p.board2.z)<p.lower.x;
    if lower_inside && pos.y<p.board2.y+p.dt_gravity.z && pos.y>p.board2.y-0.20 && vel.y<0.0 {
        pos.y=p.board2.y+p.dt_gravity.z; vel.y=-vel.y*p.dt_gravity.w; vel.xz*=p.lower.y;
    }

    // Recycle particles that have left the demonstration volume back to the source.
    if pos.y < -0.5 || abs(pos.x)>12.0 || abs(pos.z)>14.0 {
        let col=f32(i%32u); let row=f32((i/32u)%16u);
        pos=p.emitter.xyz+vec3<f32>((col-15.5)*0.025, f32((i/512u)%8u)*0.025, (row-7.5)*0.025);
        vel=p.emitter_dir.xyz;
    }
    dst[i]=vec4<f32>(pos,1.0);
    vel_dst[i]=vec4<f32>(vel,0.0);
}