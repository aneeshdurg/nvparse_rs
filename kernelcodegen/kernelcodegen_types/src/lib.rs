use glam::UVec3;
use wgpu::{BindGroupLayout, ComputePipeline, PipelineLayout};

#[cfg(not(target_arch = "spirv"))]
pub struct Generated {
    pub bind_group_layout: BindGroupLayout,
    pub pipeline_layout: PipelineLayout,
    pub compute_pipeline: ComputePipeline,
    pub workgroup_dim: UVec3,
}
