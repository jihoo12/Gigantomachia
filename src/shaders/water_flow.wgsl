const W:u32=64u; const H:u32=128u;
struct Params{bounds:vec4<f32>,source:vec4<f32>,direction:vec4<f32>};
@group(0) @binding(0) var<storage,read> src:array<vec4<f32>>;
@group(0) @binding(1) var<storage,read_write> dst:array<vec4<f32>>;
@group(0) @binding(2) var<uniform> p:Params;
fn idx(x:u32,y:u32)->u32{return y*W+x;}
fn state(x:i32,y:i32)->vec4<f32>{return src[idx(u32(clamp(x,0,i32(W)-1)),u32(clamp(y,0,i32(H)-1)))];}
@compute @workgroup_size(8,8) fn cs_main(@builtin(global_invocation_id) gid:vec3<u32>){
 if gid.x>=W||gid.y>=H{return;}
 let x=i32(gid.x);let y=i32(gid.y);let c=state(x,y);let l=state(x-1,y);let r=state(x+1,y);let d=state(x,y-1);let u=state(x,y+1);
 let dx=max(p.bounds.z*2.0/f32(W-1u),0.001);let dz=max(p.bounds.w*2.0/f32(H-1u),0.001);let dt=min(p.direction.z,1.0/60.0);

 // Damped linear shallow-water step.  The previous version continuously added
 // positive height at the impact point, so mass accumulated until the clamped
 // checkerboard mode dominated the entire receiving channel.
 let grad=vec2<f32>((r.x-l.x)/(2.0*dx),(u.x-d.x)/(2.0*dz));
 var vel=c.yz-grad*(4.8*dt);
 let div=(r.y-l.y)/(2.0*dx)+(u.z-d.z)/(2.0*dz);
 var h=c.x-div*dt*0.18;
 vel=mix(vel,(l.yz+r.yz+d.yz+u.yz)*0.25,0.12)*0.965;
 h=mix(h,(l.x+r.x+d.x+u.x)*0.25,0.075)*0.955;

 let uv=vec2<f32>(f32(gid.x)/f32(W-1u),f32(gid.y)/f32(H-1u));
 let world=p.bounds.xy+(uv*2.0-1.0)*p.bounds.zw;
 let rel=world-p.source.xy;
 let injection=exp(-dot(rel,rel)/max(p.source.z*p.source.z,0.01));
 // Inject momentum, not water volume.  A tiny zero-mean pulse excites ripples
 // without steadily raising the simulated surface.
 vel+=p.direction.xy*injection*0.62*dt;
 h+=injection*sin(p.direction.w*7.3)*0.010*dt;

 let back=world-vel*dt;
 let buv=(back-(p.bounds.xy-p.bounds.zw))/(p.bounds.zw*2.0);
 let bx=i32(clamp(buv.x,0.0,1.0)*f32(W-1u));let by=i32(clamp(buv.y,0.0,1.0)*f32(H-1u));
 let compression=max(-div,0.0);
 var foam=max(state(bx,by).w*0.965,injection*0.32+compression*0.018);
 foam=clamp(foam,0.0,0.72);

 // Side walls are solid.  At the long ends damp the state so waves/foam can
 // leave the finite simulation instead of reflecting and accumulating forever.
 if gid.x==0u||gid.x+1u==W{vel.x=0.0;}
 let edge_y=min(gid.y,H-1u-gid.y);
 if edge_y<5u {
   let open=f32(edge_y)/5.0;
   vel*=mix(0.72,1.0,open);h*=mix(0.45,1.0,open);foam*=mix(0.55,1.0,open);
 }
 dst[idx(gid.x,gid.y)]=vec4<f32>(clamp(h,-0.035,0.035),vel,foam);
}