//! GPU 3D fluid-world state.
//!
//! This is intentionally geometry-driven: particles know gravity and solid support,
//! not "waterfalls". Crossing a board edge naturally turns supported flow into free fall.
use super::Gpu;
use bytemuck::{Pod,Zeroable};
use wgpu::util::DeviceExt;

pub(super) const PARTICLE_COUNT:u32=8192;

#[repr(C)] #[derive(Clone,Copy,Pod,Zeroable)]
struct Params{dt_gravity:[f32;4],emitter:[f32;4],emitter_dir:[f32;4],board:[f32;4],board2:[f32;4],lower:[f32;4]}

pub(super) struct ParticleFluid {
 pipeline:wgpu::ComputePipeline,
 groups:[wgpu::BindGroup;2],
 current:usize,
 params:wgpu::Buffer,
}

impl ParticleFluid {
 pub fn new(gpu:&Gpu)->Self{
  let mut pos=vec![[0.0f32;4];PARTICLE_COUNT as usize];
  let mut vel=vec![[0.0f32;4];PARTICLE_COUNT as usize];
  for i in 0..PARTICLE_COUNT as usize {
   let col=(i%32) as f32; let row=((i/32)%16) as f32; let layer=((i/512)%8) as f32;
   pos[i]=[(col-15.5)*0.025,1.62+layer*0.025,-1.55+(row-7.5)*0.025,1.0];
   vel[i]=[0.0,0.0,0.72,0.0];
  }
  let make=|label,data:&Vec<[f32;4]>|gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor{
   label:Some(label),contents:bytemuck::cast_slice(data),usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC});
  let pa=make("fluid-particle-pos-a",&pos); let pb=make("fluid-particle-pos-b",&pos);
  let va=make("fluid-particle-vel-a",&vel); let vb=make("fluid-particle-vel-b",&vel);
  let params=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("fluid-particle-params"),size:std::mem::size_of::<Params>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
  let layout=gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:Some("fluid-particle-layout"),entries:&[
   (0,true),(1,false),(2,true),(3,false)].map(|(binding,read_only)|wgpu::BindGroupLayoutEntry{binding,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only},has_dynamic_offset:false,min_binding_size:None},count:None}).into_iter().chain([wgpu::BindGroupLayoutEntry{binding:4,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None}]).collect::<Vec<_>>().as_slice()});
  let group=|a:&wgpu::Buffer,b:&wgpu::Buffer,c:&wgpu::Buffer,d:&wgpu::Buffer|gpu.device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("fluid-particle-bindings"),layout:&layout,entries:&[
   wgpu::BindGroupEntry{binding:0,resource:a.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:b.as_entire_binding()},
   wgpu::BindGroupEntry{binding:2,resource:c.as_entire_binding()},wgpu::BindGroupEntry{binding:3,resource:d.as_entire_binding()},
   wgpu::BindGroupEntry{binding:4,resource:params.as_entire_binding()}]});
  let groups=[group(&pa,&pb,&va,&vb),group(&pb,&pa,&vb,&va)];
  let shader=gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("fluid-particles"),source:wgpu::ShaderSource::Wgsl(include_str!("../shaders/fluid_particles.wgsl").into())});
  let pl=gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("fluid-particle-pipeline-layout"),bind_group_layouts:&[&layout],push_constant_ranges:&[]});
  let pipeline=gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:Some("fluid-particle-simulation"),layout:Some(&pl),module:&shader,entry_point:Some("cs_main"),compilation_options:Default::default(),cache:None});
  Self{pipeline,groups,current:0,params}
 }
 pub fn update(&mut self,gpu:&Gpu,encoder:&mut wgpu::CommandEncoder){
  let p=Params{
   dt_gravity:[1.0/60.0,9.81,0.025,0.08],
   emitter:[0.0,1.62,-1.55,0.0],
   emitter_dir:[0.0,0.0,0.72,PARTICLE_COUNT as f32],
   board:[0.0,1.25,0.0,3.82],
   board2:[2.32,0.11,5.25,2.22],
   lower:[3.0,0.86,0.0,0.0],
  };
  gpu.queue.write_buffer(&self.params,0,bytemuck::bytes_of(&p));
  let mut pass=encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{label:Some("3d-fluid-particle-step"),timestamp_writes:None});
  pass.set_pipeline(&self.pipeline);pass.set_bind_group(0,&self.groups[self.current],&[]);pass.dispatch_workgroups(PARTICLE_COUNT.div_ceil(64),1,1);drop(pass);
  self.current^=1;
 }
}