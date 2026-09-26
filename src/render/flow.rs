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
struct FlowParams { bounds:[f32;4], source:[f32;4], direction:[f32;4], upper:[f32;4], outlet:[f32;4], inflow:[f32;4] }
pub(super) struct FluidSimulation { pipeline:wgpu::ComputePipeline, bind_groups:[wgpu::BindGroup;2], upper_bind_groups:[wgpu::BindGroup;2], params:wgpu::Buffer, upper_params:wgpu::Buffer, upper_buffers:[wgpu::Buffer;2], debug_readback:wgpu::Buffer, current:usize, upper_current:usize, debug_frame:u32 }
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
  Self{pipeline,bind_groups,upper_bind_groups,params,upper_params,upper_buffers:[upper_a,upper_b],debug_readback,current:0,upper_current:0,debug_frame:0}
 }
 pub fn update(&mut self,_gpu:&Gpu,_encoder:&mut wgpu::CommandEncoder,_scene:&crate::scene::Scene){
  // Legacy 2.5D surface state is intentionally frozen while the water implementation
  // moves to the 3D FluidWorld. It no longer creates or transfers waterfall outflow.
 }
 pub fn binding(&self)->&wgpu::BindGroup{&self.bind_groups[self.current]}
 pub fn upper_binding(&self)->&wgpu::BindGroup{&self.upper_bind_groups[self.upper_current]}
}