#![cfg_attr(target_arch = "spirv", no_std)]
#![deny(warnings)]
use glam::UVec3;
use spirv_std::{glam, spirv};

#[cfg(not(target_arch = "spirv"))]
pub mod codegen {
    use crate::glam::UVec3;
    use core::num::NonZeroU64;
    use kernelcodegen_types::Generated;
    use wgpu::Device;

    pub fn new(device: &Device, shader_bytes: &[u8]) -> Generated {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("countchar_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                        ty: wgpu::BufferBindingType::Uniform,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    count: None,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                        ty: wgpu::BufferBindingType::Uniform,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    count: None,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                        ty: wgpu::BufferBindingType::Uniform,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    count: None,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                    },
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mylayout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let spirv = std::borrow::Cow::Owned(wgpu::util::make_spirv_raw(shader_bytes).into_owned());
        let shader_binary = wgpu::ShaderModuleDescriptorSpirV {
            label: Some("countchar"),
            source: spirv,
        };
        // Load the shaders from disk
        let module = unsafe { device.create_shader_module_spirv(&shader_binary) };

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("countchar_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main_cc"),
            compilation_options: Default::default(),
            cache: None,
        });

        let workgroup_dim = UVec3::from((64, 1, 1));

        Generated {
            bind_group_layout,
            pipeline_layout,
            compute_pipeline,
            workgroup_dim,
        }
    }
}

#[spirv(compute(threads(64)))]
pub fn main_cc(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] input: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] chunk_size: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 2)] data_len: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 3)] char: &u8,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] count: &mut [u32],
) {
    let index = id.x as usize;
    count[index] = 0;

    let start: usize = index * (*chunk_size as usize);
    let nelems: usize = core::cmp::min(*chunk_size, *data_len - start as u32) as usize;

    for i in start..(start + nelems) {
        if input[i] == *char {
            count[index] += 1;
        }
    }
}
