const W:u32=64u; const H:u32=128u;
struct Params{bounds:vec4<f32>,source:vec4<f32>,direction:vec4<f32>};
@group(0) @binding(0) var<storage,read> src:array<vec4<f32>>;
@group(0) @binding(1) var<storage,read_write> dst:array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p:Params;
fn idx(x:u32,y:u32)->u32{return y*W+x;}
fn state(x:i32,y:i32)->vec4<f32>{return src[idx(u32(clamp(x,0,i32(W)-1)),u32(clamp(y,0,i32(H)-1)))];}
@compute @workgroup_size(8,8) fn cs_main(@builtin(global_invocation_id) gid:vec3<u32>){
 if gid.x>=W||gid.y>=H{return;} let x=i32(gid.x);let y=i32(gid.y);let c=state(x,y);let l=state(x-1,y);let r=state(x+1,y);let d=state(x,y-1);let u=state(x,y+1);
 let dx=max(p.bounds.z*2.0/f32(W-1u),0.001);let dz=max(p.bounds.w*2.0/f32(H-1u),0.001);let dt=min(p.direction.z,1.0/45.0);
 let grad=vec2<f32>((r.x-l.x)/(2.0*dx),(u.x-d.x)/(2.0*dz));var vel=c.yz-grad*(9.81*dt);let div=(r.y-l.y)/(2.0*dx)+(u.z-d.z)/(2.0*dz);var h=c.x-div*dt*0.34;
 vel=mix(vel,(l.yz+r.yz+d.yz+u.yz)*0.25,0.055)*0.992;h=mix(h,(l.x+r.x+d.x+u.x)*0.25,0.018)*0.997;
 let uv=vec2<f32>(f32(gid.x)/f32(W-1u),f32(gid.y)/f32(H-1u));let world=p.bounds.xy+(uv*2.0-1.0)*p.bounds.zw;let rel=world-p.source.xy;let injection=exp(-dot(rel,rel)/max(p.source.z*p.source.z,0.01));
 vel+=p.direction.xy*injection*1.8*dt;h+=injection*0.20*dt;
 let back=world-vel*dt;let buv=(back-(p.bounds.xy-p.bounds.zw))/(p.bounds.zw*2.0);let bx=i32(clamp(buv.x,0.0,1.0)*f32(W-1u));let by=i32(clamp(buv.y,0.0,1.0)*f32(H-1u));
 let compression=max(-div,0.0);var foam=max(state(bx,by).w*0.986,injection*0.78+compression*0.055);foam=clamp(foam,0.0,1.0);if gid.x==0u||gid.x+1u==W{vel.x=0.0;}dst[idx(gid.x,gid.y)]=vec4<f32>(clamp(h,-0.12,0.12),vel,foam);
}