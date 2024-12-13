#![cfg_attr(target_arch = "spirv", no_std)]
use glam::UVec3;
use kernelcodegen::generate_kernel;
use spirv_std::{arch, glam, spirv};

#[generate_kernel()]
#[spirv(compute(threads(256)))]
pub fn main_cc(
    #[spirv(local_invocation_id)] lid: UVec3,
    #[spirv(global_invocation_id)] id: UVec3,
    // #[spirv(workgroup)] shared: &mut [u32; 1024],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] input: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] chunk_size: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 2)] data_len: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 3)] char: &u8,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] count: &mut [u32],
) {
    let index = id.x as usize;

    let start: usize = index * (*chunk_size as usize);
    let nelems: usize = core::cmp::min(*chunk_size, *data_len - start as u32) as usize;

    let mut acc = 0;
    for i in start..(start + nelems) {
        if input[i] == *char {
            acc += 1;
        }
    }
    count[lid.x as usize] += acc;

    // unsafe { arch::workgroup_memory_barrier_with_group_sync() };
    // if lid.x == 0 {
    //     count[lid.x as usize] += shared.iter().fold(0, |acc, e| acc + e);
    // }
}
