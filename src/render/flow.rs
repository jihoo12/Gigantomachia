//! GPU fluid dynamics core.
//!
//! This is the first engine-level fluid layer: a conservative shallow-water state
//! (surface displacement, horizontal velocity, foam) shared by rendering.  Free-surface
//! outflow/ballistic transfer will build on this state instead of scripted waterfall parts.
use super::Gpu;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
pub(super) const FLOW_W: u32 = 64;
pub(super) const FLOW_H: u32 = 128;
#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)]
struct FlowParams { bounds:[f32;4], source:[f32;4], direction:[f32;4] }
pub(super) struct FluidSimulation { pipeline:wgpu::ComputePipeline, bind_groups:[wgpu::BindGroup;2], params:wgpu::Buffer, current:usize }
impl FluidSimulation {
 pub fn layout(gpu:&Gpu)->wgpu::BindGroupLayout { gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:Some("water-flow-layout"),entries:&[
  wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::COMPUTE|wgpu::ShaderStages::VERTEX_FRAGMENT,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None},
  wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:false},has_dynamic_offset:false,min_binding_size:None},count:None},
  wgpu::BindGroupLayoutEntry{binding:2,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None},
 ]})}
 pub fn new(gpu:&Gpu,layout:&wgpu::BindGroupLayout)->Self {
  let zero=vec![[0.0f32;4];(FLOW_W*FLOW_H) as usize];
  let make=|label|gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor{label:Some(label),contents:bytemuck::cast_slice(&zero),usage:wgpu::BufferUsages::STORAGE});
  let a=make("water-flow-a"); let b=make("water-flow-b");
  let params=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("water-flow-params"),size:std::mem::size_of::<FlowParams>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
  let group=|input:&wgpu::Buffer,output:&wgpu::Buffer|gpu.device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("water-flow-bindings"),layout,entries:&[
   wgpu::BindGroupEntry{binding:0,resource:input.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:output.as_entire_binding()},wgpu::BindGroupEntry{binding:2,resource:params.as_entire_binding()}]});
  let bind_groups=[group(&a,&b),group(&b,&a)];
  let shader=gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("water-flow-simulation"),source:wgpu::ShaderSource::Wgsl(include_str!("../shaders/water_flow.wgsl").into())});
  let pl=gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("water-flow-pipeline-layout"),bind_group_layouts:&[layout],push_constant_ranges:&[]});
  let pipeline=gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:Some("water-flow-simulation"),layout:Some(&pl),module:&shader,entry_point:Some("cs_main"),compilation_options:Default::default(),cache:None});
  Self{pipeline,bind_groups,params,current:0}
 }
 pub fn update(&mut self,gpu:&Gpu,encoder:&mut wgpu::CommandEncoder,scene:&crate::scene::Scene){
  let (Some(lower),Some(upper))=(scene.secondary_water,scene.water) else{return}; let Some(bounds)=lower.bounds else{return}; let Some(fall)=upper.waterfall else{return};
  let dir=fall.direction.normalize_or_zero(); let flight=(2.0*fall.drop.max(0.05)/9.81).sqrt(); let landing=glam::Vec2::new(fall.origin.x, fall.origin.z)+dir*(0.22+1.3*flight);
  let p=FlowParams{bounds:[bounds.center.x,bounds.center.y,bounds.half_extent.x,bounds.half_extent.y],source:[landing.x,landing.y,(fall.width*0.34).max(0.16),1.0],direction:[dir.x,dir.y,1.0/60.0,lower.time]};
  gpu.queue.write_buffer(&self.params,0,bytemuck::bytes_of(&p));
  let mut pass=encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{label:Some("water-flow-simulation"),timestamp_writes:None}); pass.set_pipeline(&self.pipeline); pass.set_bind_group(0,&self.bind_groups[self.current],&[]); pass.dispatch_workgroups(FLOW_W.div_ceil(8),FLOW_H.div_ceil(8),1); drop(pass); self.current^=1;
 }
 pub fn binding(&self)->&wgpu::BindGroup{&self.bind_groups[self.current]}
}