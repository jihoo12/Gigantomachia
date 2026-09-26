//! GPU execution of the engine-level 3D FluidWorld.
use super::Gpu;
use crate::fluid::{FluidCollider, FluidWorld};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub(super) const PARTICLE_COUNT: u32 = 8192;
const MAX_COLLIDERS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dt_gravity: [f32; 4],
    emitter_min: [f32; 4],
    emitter_max: [f32; 4],
    emitter_velocity: [f32; 4],
    counts: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuAabb {
    min: [f32; 4],
    max: [f32; 4],
}

pub(super) struct ParticleFluid {
    pipeline: wgpu::ComputePipeline,
    render_pipeline: wgpu::RenderPipeline,
    groups: [wgpu::BindGroup; 2],
    render_groups: [wgpu::BindGroup; 2],
    current: usize,
    params: wgpu::Buffer,
    colliders: wgpu::Buffer,
}

impl ParticleFluid {
    pub fn new(gpu: &Gpu, scene_layout: &wgpu::BindGroupLayout) -> Self {
        let pos = vec![[0.0f32; 4]; PARTICLE_COUNT as usize];
        let vel = vec![[0.0f32; 4]; PARTICLE_COUNT as usize];
        let make = |label, data: &Vec<[f32; 4]>| gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label), contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        let pa=make("fluid-particle-pos-a",&pos); let pb=make("fluid-particle-pos-b",&pos);
        let va=make("fluid-particle-vel-a",&vel); let vb=make("fluid-particle-vel-b",&vel);
        let params=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("fluid-particle-params"),size:std::mem::size_of::<Params>() as u64,usage:wgpu::BufferUsages::UNIFORM|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        let colliders=gpu.device.create_buffer(&wgpu::BufferDescriptor{label:Some("fluid-colliders"),size:(MAX_COLLIDERS*std::mem::size_of::<GpuAabb>()) as u64,usage:wgpu::BufferUsages::STORAGE|wgpu::BufferUsages::COPY_DST,mapped_at_creation:false});
        let layout=gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:Some("fluid-particle-layout"),entries:&[
            (0,true),(1,false),(2,true),(3,false),(5,true)].map(|(binding,read_only)|wgpu::BindGroupLayoutEntry{binding,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only},has_dynamic_offset:false,min_binding_size:None},count:None}).into_iter().chain([wgpu::BindGroupLayoutEntry{binding:4,visibility:wgpu::ShaderStages::COMPUTE,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Uniform,has_dynamic_offset:false,min_binding_size:None},count:None}]).collect::<Vec<_>>().as_slice()});
        let group=|a:&wgpu::Buffer,b:&wgpu::Buffer,c:&wgpu::Buffer,d:&wgpu::Buffer|gpu.device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("fluid-particle-bindings"),layout:&layout,entries:&[
            wgpu::BindGroupEntry{binding:0,resource:a.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:b.as_entire_binding()},
            wgpu::BindGroupEntry{binding:2,resource:c.as_entire_binding()},wgpu::BindGroupEntry{binding:3,resource:d.as_entire_binding()},
            wgpu::BindGroupEntry{binding:4,resource:params.as_entire_binding()},wgpu::BindGroupEntry{binding:5,resource:colliders.as_entire_binding()}]});
        let groups=[group(&pa,&pb,&va,&vb),group(&pb,&pa,&vb,&va)];
        let render_layout=gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor{label:Some("fluid-particle-render-layout"),entries:&[
            wgpu::BindGroupLayoutEntry{binding:0,visibility:wgpu::ShaderStages::VERTEX,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None},
            wgpu::BindGroupLayoutEntry{binding:1,visibility:wgpu::ShaderStages::VERTEX,ty:wgpu::BindingType::Buffer{ty:wgpu::BufferBindingType::Storage{read_only:true},has_dynamic_offset:false,min_binding_size:None},count:None}]});
        let rg=|p:&wgpu::Buffer,v:&wgpu::Buffer|gpu.device.create_bind_group(&wgpu::BindGroupDescriptor{label:Some("fluid-particle-render-bindings"),layout:&render_layout,entries:&[wgpu::BindGroupEntry{binding:0,resource:p.as_entire_binding()},wgpu::BindGroupEntry{binding:1,resource:v.as_entire_binding()}]});
        let render_groups=[rg(&pa,&va),rg(&pb,&vb)];
        let shader=gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("fluid-particles"),source:wgpu::ShaderSource::Wgsl(include_str!("../shaders/fluid_particles.wgsl").into())});
        let pl=gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("fluid-particle-pipeline-layout"),bind_group_layouts:&[&layout],push_constant_ranges:&[]});
        let pipeline=gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor{label:Some("fluid-particle-simulation"),layout:Some(&pl),module:&shader,entry_point:Some("cs_main"),compilation_options:Default::default(),cache:None});
        let render_shader=gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor{label:Some("fluid-particle-render"),source:wgpu::ShaderSource::Wgsl(format!("{}\n{}",include_str!("../shaders/common.wgsl"),include_str!("../shaders/fluid_particles_render.wgsl")).into())});
        let rpl=gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor{label:Some("fluid-particle-render-pipeline-layout"),bind_group_layouts:&[scene_layout,&render_layout],push_constant_ranges:&[]});
        let render_pipeline=gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:Some("fluid-particle-debug-render"),layout:Some(&rpl),vertex:wgpu::VertexState{module:&render_shader,entry_point:Some("vs_main"),buffers:&[],compilation_options:Default::default()},fragment:Some(wgpu::FragmentState{module:&render_shader,entry_point:Some("fs_main"),targets:&[Some(wgpu::ColorTargetState{format:super::targets::HDR_FORMAT,blend:Some(wgpu::BlendState::ALPHA_BLENDING),write_mask:wgpu::ColorWrites::ALL})],compilation_options:Default::default()}),primitive:wgpu::PrimitiveState{cull_mode:None,..Default::default()},depth_stencil:Some(wgpu::DepthStencilState{format:super::DEPTH_FORMAT,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None});
        Self{pipeline,render_pipeline,groups,render_groups,current:0,params,colliders}
    }

    pub fn update(&mut self,gpu:&Gpu,encoder:&mut wgpu::CommandEncoder,world:Option<&FluidWorld>){
        let Some(world)=world else { return };
        let Some(emitter)=world.emitters.first() else { return };
        let mut gpu_colliders=Vec::with_capacity(world.colliders.len().min(MAX_COLLIDERS));
        for collider in world.colliders.iter().take(MAX_COLLIDERS) {
            let FluidCollider::Box(bounds)=*collider;
            let min=bounds.min(); let max=bounds.max();
            gpu_colliders.push(GpuAabb{min:[min.x,min.y,min.z,0.0],max:[max.x,max.y,max.z,0.0]});
        }
        if !gpu_colliders.is_empty(){gpu.queue.write_buffer(&self.colliders,0,bytemuck::cast_slice(&gpu_colliders));}
        let min=emitter.volume.min(); let max=emitter.volume.max();
        let p=Params{
            dt_gravity:[1.0/60.0,9.81,0.025,0.08],
            emitter_min:[min.x,min.y,min.z,emitter.rate],
            emitter_max:[max.x,max.y,max.z,0.0],
            emitter_velocity:[emitter.velocity.x,emitter.velocity.y,emitter.velocity.z,0.0],
            counts:[gpu_colliders.len() as u32,PARTICLE_COUNT,0,0],
        };
        gpu.queue.write_buffer(&self.params,0,bytemuck::bytes_of(&p));
        let mut pass=encoder.begin_compute_pass(&wgpu::ComputePassDescriptor{label:Some("3d-fluid-particle-step"),timestamp_writes:None});
        pass.set_pipeline(&self.pipeline);pass.set_bind_group(0,&self.groups[self.current],&[]);pass.dispatch_workgroups(PARTICLE_COUNT.div_ceil(64),1,1);drop(pass);
        self.current^=1;
    }
    pub fn encode<'a>(&'a self,pass:&mut wgpu::RenderPass<'a>){
        pass.set_pipeline(&self.render_pipeline);pass.set_bind_group(1,&self.render_groups[self.current],&[]);pass.draw(0..PARTICLE_COUNT*6,0..1);
    }
}
