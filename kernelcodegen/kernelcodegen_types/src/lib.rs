#[cfg(not(target_arch = "spirv"))]
use wgpu::{BindGroupLayout, ComputePipeline, PipelineLayout};

#[cfg(not(target_arch = "spirv"))]
#[derive(Debug)]
pub struct ComputeKernel {
    pub bind_group_layout: BindGroupLayout,
    pub pipeline_layout: PipelineLayout,
    pub compute_pipeline: ComputePipeline,
    pub workgroup_dim: (u32, u32, u32),
}
