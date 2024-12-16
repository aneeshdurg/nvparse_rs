#![cfg_attr(target_arch = "spirv", no_std)]
#![deny(warnings)]

use glam::UVec3;
use kernelcodegen::generate_kernel;
use spirv_std::{arch, glam, memory, spirv};

#[generate_kernel()]
#[spirv(compute(threads(256)))]
pub fn main_getcharpos(
    #[spirv(local_invocation_id)] lid: UVec3,
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] input: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] chunk_size: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 2)] data_len: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 3)] char: &u8,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] n_lines_per_thread: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] output: &mut [u32],
) {
    let index = id.x as usize;
    if index == 0 {
        output[0] = 0;
    }

    let lindex = lid.x as usize;

    let out_start = (0..lindex)
        .into_iter()
        .fold(0, |acc, i| acc + n_lines_per_thread[i]) as usize;
    unsafe { arch::device_memory_barrier_with_group_sync() };

    let start: usize = index * (*chunk_size as usize);
    let nelems: usize = core::cmp::min(*chunk_size, *data_len - start as u32) as usize;

    for i in start..(start + nelems) {
        if input[i] == *char {
            let out_index = unsafe {
                arch::atomic_i_sub::<
                    u32,
                    { memory::Scope::Device as u32 },
                    { memory::Semantics::OUTPUT_MEMORY.bits() },
                >(&mut output[lindex], 1)
            } - 1;
            let out_index = out_index as usize;

            output[out_start + out_index] = 1 + i as u32;
        }
    }
}
