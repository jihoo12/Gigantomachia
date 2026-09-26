struct ParticleVertex {
 @builtin(position) clip: vec4<f32>,
 @location(0) world: vec3<f32>,
 @location(1) local: vec2<f32>,
 @location(2) speed: f32,
};
@group(1) @binding(0) var<storage,read> positions:array<vec4<f32>>;
@group(1) @binding(1) var<storage,read> velocities:array<vec4<f32>>;

fn corner(i:u32)->vec2<f32>{
 let c=array<vec2<f32>,6>(
  vec2<f32>(-1.0,-1.0),vec2<f32>(1.0,-1.0),vec2<f32>(1.0,1.0),
  vec2<f32>(-1.0,-1.0),vec2<f32>(1.0,1.0),vec2<f32>(-1.0,1.0));
 return c[i];
}
@vertex fn vs_main(@builtin(vertex_index) index:u32)->ParticleVertex{
 let particle=index/6u; let q=corner(index%6u); let center=positions[particle].xyz;
 let camera=scene.camera_time.xyz;
 let view=normalize(camera-center);
 let right=normalize(cross(vec3<f32>(0.0,1.0,0.0),view));
 let up=normalize(cross(view,right));
 let radius=0.032;
 let world=center+(right*q.x+up*q.y)*radius;
 var o:ParticleVertex;
 o.clip=scene.view_projection*vec4<f32>(world,1.0);
 o.world=world;o.local=q;o.speed=length(velocities[particle].xyz);
 return o;
}
@fragment fn fs_main(in:ParticleVertex)->@location(0) vec4<f32>{
 let r2=dot(in.local,in.local); if r2>1.0{discard;}
 let z=sqrt(max(1.0-r2,0.0));
 let n=normalize(vec3<f32>(in.local.x,in.local.y,z));
 let view=normalize(scene.camera_time.xyz-in.world);
 let fresnel=0.02037+0.97963*pow(1.0-max(dot(n,view),0.0),5.0);
 let base=vec3<f32>(0.08,0.32,0.42);
 let highlight=vec3<f32>(0.82,0.92,0.95);
 let color=mix(base,highlight,clamp(fresnel+in.speed*0.08,0.0,0.8));
 return vec4<f32>(color,0.62);
}