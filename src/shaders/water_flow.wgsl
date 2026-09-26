const W:u32=64u; const H:u32=128u;
struct Params{bounds:vec4<f32>,source:vec4<f32>,direction:vec4<f32>,upper:vec4<f32>,outlet:vec4<f32>,inflow:vec4<f32>};
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
 // Boundary flux arriving from the upper domain.  Until the upper domain gets its
 // own grid, depth is the physical nominal depth and velocity is gravity-driven
 // toward the lip; unlike the old pulse this transfers persistent mass + momentum.
 let lip_rel=world-p.source.xy;
 let producer=select(0.0,1.0,p.outlet.w<0.0);
 let nominal_depth=abs(p.outlet.w);
 let incoming_flux=nominal_depth*sqrt(2.0*9.81*max(nominal_depth,0.001));
 // Only the receiver uses the prescribed transfer. The producer must derive its
 // discharge from its own simulated depth and outward velocity at the lip.
 let dynamic_depth=max(nominal_depth+c.x,0.0);
 let outward=max(dot(vel,p.direction.xy),0.0);
 let producer_flux=dynamic_depth*max(outward,sqrt(2.0*9.81*dynamic_depth)*0.28);
 let transfer=injection*mix(incoming_flux,producer_flux,producer);
 // A producer domain has a real mass source on the side opposite the outlet.
 // This establishes a persistent source -> transport -> open-boundary flow instead
 // of draining an initially flat visual surface.
 let feed_rel=world-p.inflow.xy;
 let feed=exp(-dot(feed_rel,feed_rel)/max(p.inflow.z*p.inflow.z,0.01))*producer;
 let feed_rate=feed*max(p.inflow.w,0.0);
 // Inject momentum, not water volume.  A tiny zero-mean pulse excites ripples
 // without steadily raising the simulated surface.
 // Producer (upper) domain accelerates toward the open lip and loses height.
 // Receiver domain gains the same directed momentum and surface volume.
 vel+=p.direction.xy*transfer*mix(5.5,3.4,producer)*dt;
 h+=transfer*mix(0.16,-0.13,producer)*dt;
 h+=feed_rate*dt;
 // Give newly supplied mass a gentle downstream momentum. Pressure gradients then
 // transport it across the board; this is intentionally weaker than the lip acceleration.
 vel+=p.direction.xy*feed_rate*1.8*dt;
 // Drain the same transferred volume over a wider wake so the finite lower domain
 // does not accumulate water indefinitely.
 let wake=exp(-dot(lip_rel-p.direction.xy*0.55,lip_rel-p.direction.xy*0.55)/max(p.source.z*p.source.z*3.5,0.04));
 h-=wake*transfer*0.11*dt*(1.0-producer);

 let back=world-vel*dt;
 let buv=(back-(p.bounds.xy-p.bounds.zw))/(p.bounds.zw*2.0);
 let bx=i32(clamp(buv.x,0.0,1.0)*f32(W-1u));let by=i32(clamp(buv.y,0.0,1.0)*f32(H-1u));
 let compression=max(-div,0.0);
 var foam=max(state(bx,by).w*0.965,injection*0.32+compression*0.018);
 foam=clamp(foam,0.0,0.72);

 // Side walls are solid.  At the long ends damp the state so waves/foam can
 // leave the finite simulation instead of reflecting and accumulating forever.
 if gid.x==0u||gid.x+1u==W{vel.x=0.0;}
 // Producer outlet is an actual open boundary. Cells in the lip aperture keep their
 // outward characteristic instead of being damped/reflected by the generic edge rule.
 let outlet_delta=world-p.outlet.xy;
 let along=dot(outlet_delta,p.direction.xy);
 let lateral=abs(dot(outlet_delta,vec2<f32>(p.direction.y,-p.direction.x)));
 let cell_size=max(dx,dz);
 let at_lip=producer>0.5 && lateral<=p.outlet.z && abs(along)<=cell_size*1.75;
 if at_lip {
   vel=max(dot(vel,p.direction.xy),sqrt(2.0*9.81*max(nominal_depth+h,0.001))*0.28)*p.direction.xy
       +(vel-p.direction.xy*dot(vel,p.direction.xy))*0.65;
   h-=producer_flux*dt/max(cell_size,0.01)*0.22;
 }
 let edge_y=min(gid.y,H-1u-gid.y);
 if edge_y<5u && !at_lip {
   let open=f32(edge_y)/5.0;
   vel*=mix(0.72,1.0,open);h*=mix(0.45,1.0,open);foam*=mix(0.55,1.0,open);
 }
 dst[idx(gid.x,gid.y)]=vec4<f32>(clamp(h,-0.035,0.035),vel,foam);
}