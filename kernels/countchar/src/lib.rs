#![cfg_attr(target_arch = "spirv", no_std)]
use glam::UVec3;
use kernelcodegen::generate_kernel;
use spirv_std::{arch, glam, memory, spirv};

#[generate_kernel()]
#[spirv(compute(threads(256)))]
pub fn main_cc(
    #[spirv(local_invocation_id)] lid: UVec3,
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] input: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] chunk_size: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 2)] data_len: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 3)] char: &u8,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] count: &mut [u32],
) {
    let index = id.x as usize;
    let lindex = lid.x as usize;

    let start: usize = index * (*chunk_size as usize);

    let mut acc = 0;
    for i in start..(start + *chunk_size as usize) {
        if i < *data_len as usize && input[i] == *char {
            acc += 1;
        }
    }

    // Each thread per workgroup adds to a unique index in the output - this needs to be
    // synchronized across all workgroup instances though.
    unsafe {
        arch::atomic_i_add::<
            u32,
            { memory::Scope::Device as u32 },
            { memory::Semantics::OUTPUT_MEMORY.bits() },
        >(&mut count[lindex], acc)
    };
}
