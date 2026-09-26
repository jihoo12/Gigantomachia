//! GPU fluid dynamics core.
//!
//! This is the first engine-level fluid layer: a conservative shallow-water state
//! (surface displacement, horizontal velocity, foam) shared by rendering.  Free-surface
//! outflow/ballistic transfer will build on this state instead of scripted waterfall parts.
use super::Gpu;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use std::sync::mpsc;
pub(super) const FLOW_W: u32 = 64;
pub(super) const FLOW_H: u32 = 128;
#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)]
struct FlowParams { bounds:[f32;4], source:[f32;4], direction:[f32;4], upper:[f32;4], outlet:[f32;4], inflow:[f32;4] }
pub(super) struct FluidSimulation { pipeline:wgpu::ComputePipeline, bind_groups:[wgpu::BindGroup;2], upper_bind_groups:[wgpu::BindGroup;2], params:wgpu::Buffer, upper_params:wgpu::Buffer, upper_buffers:[wgpu::Buffer;2], debug_readback:wgpu::Buffer, current:usize, upper_current:usize, debug_frame:u32, debug_rx:Option<mpsc::Receiver<Result<(),wgpu::BufferAsyncError>>> }
impl FluidSimulation {
 pub fn layout(gpu:&Gpu)->wgpu::BindGroupLayout { gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:Some("water-flow-layout"),entries:&[
  wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::COMPUTE|wgpu::ShaderStages::VERTEX_FRAGMENT,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None},
  wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:false},has_dynamic_offset:false,min_binding_size:None},count:None},
  wgpu::BindGroupLayoutEntry{binding:2,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
 ]})}
 pub fn new(gpu:&Gpu,layout:&wgpu::BindGroupLayout)->Self {
  let zero=vec![[0.0f32;4];(FLOW_W*FLOW_H) as usize];
  let make=|label|gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:Some(label),contents:bytemuck::cast_slice(&zero),usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_SRC});
  let a=make("water-flow-a"); let b=make("water-flow-b");
  let upper_a=make("water-upper-flow-a"); let upper_b=make("water-upper-flow-b");
  let params=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("water-flow-params"),size:std::mem::size_of::<FlowParams>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
  let upper_params=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("water-upper-flow-params"),size:std::mem::size_of::<FlowParams>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
  let group=|label:&str,input:&wgpu::Buffer,output:&wgpu::Buffer,param:&wgpu::Buffer|gpu.device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some(label),layout,entries:&[
   wgpu::BindGroupEntry{binding:0,resource:input.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:output.as_entire_binding()},wgpu::BindGroupEntry{binding:2,resource:param.as_entire_binding()}]});
  let bind_groups=[group("water-flow-bindings",&a,&b,&params),group("water-flow-bindings",&b,&a,&params)];
  let upper_bind_groups=[group("water-upper-flow-bindings",&upper_a,&upper_b,&upper_params),group("water-upper-flow-bindings",&upper_b,&upper_a,&upper_params)];
  let shader=gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("water-flow-simulation"),source:wgpu::ShaderSource::Wgsl(include_str!("../shaders/water_flow.wgsl").into())});
  let pl=gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("water-flow-pipeline-layout"),bind_group_layouts:&[layout],push_constant_ranges:&[]});
  let pipeline=gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:Some("water-flow-simulation"),layout:Some(&pl),module:&shader,entry_point:Some("cs_main"),compilation_options:Default::default(),cache:None});
  let debug_readback=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("water-upper-debug-readback"),size:(FLOW_W*FLOW_H*16) as u64,usage:wgpu::BufferUsages::MAP_READ|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
  Self{pipeline,bind_groups,upper_bind_groups,params,upper_params,upper_buffers:[upper_a,upper_b],debug_readback,current:0,upper_current:0,debug_frame:0,debug_rx:None}
 }
 pub fn update(&mut self,gpu:&Gpu,encoder:&mut wgpu::CommandEncoder,scene:&crate::scene::Scene){
  // Finish the previous sparse readback without blocking the render loop.
  if let Some(rx)=&self.debug_rx {
   if let Ok(result)=rx.try_recv() {
    if result.is_ok() {
     let bytes=self.debug_readback.slice(..).get_mapped_range();
     let states:&[[f32;4]]=bytemuck::cast_slice(&bytes);
     let mut mass=0.0f32; let mut max_depth=-f32::INFINITY; let mut max_speed=0.0f32;
     let mut lip_depth=0.0f32; let mut lip_speed=0.0f32; let mut lip_flux=0.0f32; let mut lip_n=0u32;
     for (i,v) in states.iter().enumerate() {
      mass+=v[0]; max_depth=max_depth.max(v[0]); max_speed=max_speed.max((v[1]*v[1]+v[2]*v[2]).sqrt());
      let y=i as u32/FLOW_W; let x=i as u32%FLOW_W;
      if y+3>=FLOW_H && x>=FLOW_W/2-8 && x<=FLOW_W/2+8 {
       let speed=(v[1]*v[1]+v[2]*v[2]).sqrt();
       lip_depth+=v[0]; lip_speed+=speed; lip_flux+=max_depth.max(0.0)*speed; lip_n+=1;
      }
     }
     let n=lip_n.max(1) as f32;
     eprintln!("[fluid upper] mass={:.4} lip_depth={:.5} lip_speed={:.5} outflow~={:.5} max_depth={:.5} max_speed={:.5}",
      mass,lip_depth/n,lip_speed/n,lip_flux/n,max_depth,max_speed);
     drop(bytes); self.debug_readback.unmap();
    }
    self.debug_rx=None;
   }
  }
  let (Some(lower),Some(upper))=(scene.secondary_water,scene.water) else{return}; let Some(bounds)=lower.bounds else{return}; let Some(fall)=upper.waterfall else{return};
  let dir=fall.direction.normalize_or_zero(); let flight=(2.0*fall.drop.max(0.05)/9.81).sqrt(); let landing=glam::Vec2::new(fall.origin.x, fall.origin.z)+dir*(0.22+1.3*flight);
  let upper_bounds=upper.bounds.unwrap_or(bounds);
  let p=FlowParams{
   bounds:[bounds.center.x,bounds.center.y,bounds.half_extent.x,bounds.half_extent.y],
   source:[landing.x,landing.y,(fall.width*0.34).max(0.16),1.0],
   direction:[dir.x,dir.y,1.0/60.0,lower.time],
   upper:[upper_bounds.center.x,upper_bounds.center.y,upper_bounds.half_extent.x,upper_bounds.half_extent.y],
   // outlet.xy is the physical lip; z is half-width and w is nominal depth.
   outlet:[fall.origin.x,fall.origin.z,(fall.width*0.5).max(0.025),0.055],
   inflow:[0.0,0.0,0.0,0.0],
  };
  let upper_p=FlowParams{
   bounds:p.upper,
   source:[fall.origin.x,fall.origin.z,(fall.width*0.5).max(0.10),1.0],
   direction:[dir.x,dir.y,1.0/60.0,upper.time],
   upper:p.upper,
   // Negative w marks the producer domain: cells near the lip are accelerated
   // outward and drained, producing an actual simulated boundary outflow.
   outlet:[fall.origin.x,fall.origin.z,(fall.width*0.5).max(0.025),-0.055],
   // Feed water from the opposite side of the board. xyz = source position/radius,
   // w = target shallow-water depth supplied per second.
   inflow:[
    upper_bounds.center.x-dir.x*upper_bounds.half_extent.x*0.72,
    upper_bounds.center.y-dir.y*upper_bounds.half_extent.y*0.72,
    (fall.width*1.25).max(0.45),
    0.060,
   ],
  };
  gpu.queue.write_buffer(&self.params,0,bytemuck::bytes_of(&p));
  gpu.queue.write_buffer(&self.upper_params,0,bytemuck::bytes_of(&upper_p));
  {
   let mut pass=encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{label:Some("water-upper-fluid-simulation"),timestamp_writes:None});
   pass.set_pipeline(&self.pipeline); pass.set_bind_group(0,&self.upper_bind_groups[self.upper_current],&[]);
   pass.dispatch_workgroups(FLOW_W.div_ceil(8),FLOW_H.div_ceil(8),1);
  }
  self.upper_current^=1;
  self.debug_frame=self.debug_frame.wrapping_add(1);
  // Copy the current upper state periodically. Mapping/printing happens on a sparse
  // cadence so diagnostics do not become part of the simulation's hot path.
  if self.debug_frame%120==0 && self.debug_rx.is_none() {
   encoder.copy_buffer_to_buffer(&self.upper_buffers[self.upper_current],0,&self.debug_readback,0,(FLOW_W*FLOW_H*16) as u64);
   let (tx,rx)=mpsc::channel();
   self.debug_readback.slice(..).map_async(wgpu::MapMode::Read,move |r|{let _=tx.send(r);});
   self.debug_rx=Some(rx);
  }
  let mut pass=encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{label:Some("water-flow-simulation"),timestamp_writes:None}); pass.set_pipeline(&self.pipeline); pass.set_bind_group(0,&self.bind_groups[self.current],&[]); pass.dispatch_workgroups(FLOW_W.div_ceil(8),FLOW_H.div_ceil(8),1); drop(pass); self.current^=1;
 }
 pub fn binding(&self)->&wgpu::BindGroup{&self.bind_groups[self.current]}
 pub fn upper_binding(&self)->&wgpu::BindGroup{&self.upper_bind_groups[self.upper_current]}
}