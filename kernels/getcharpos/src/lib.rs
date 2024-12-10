#![cfg_attr(target_arch = "spirv", no_std)]
#![deny(warnings)]

use glam::UVec3;
use spirv_std::{glam, spirv};

#[spirv(compute(threads(64)))]
pub fn main_getcharpos(
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

    let mut out_index: usize = 0;
    for i in 0..index {
        out_index += n_lines_per_thread[i] as usize;
    }

    let start: usize = index * (*chunk_size as usize);
    let nelems: usize = core::cmp::min(*chunk_size, *data_len - start as u32) as usize;

    for i in start..(start + nelems) {
        if input[i] == *char {
            output[out_index + 1] = 1 + i as u32;
            out_index += 1;
        }
    }
}
