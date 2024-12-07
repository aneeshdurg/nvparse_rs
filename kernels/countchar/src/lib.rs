#![cfg_attr(target_arch = "spirv", no_std)]
// #![deny(warnings)]
use glam::UVec3;
use spirv_std::{glam, spirv};

fn min(a: u32, b: u32) -> u32 {
    if a < b {
        a
    } else {
        b
    }
}

fn to_bytes(a: u32) -> [u8; 4] {
    let mut v = [0u8; 4];
    let mut a = a;
    v[0] = (a % 256) as u8;
    a /= 256;
    v[1] = (a % 256) as u8;
    a /= 256;
    v[2] = (a % 256) as u8;
    a /= 256;
    v[3] = (a % 256) as u8;
    v
}

// LocalSize/numthreads of (x = 64, y = 1, z = 1)
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

    let mut start: usize = index * (*chunk_size as usize);
    let mut nelems: usize = min(*chunk_size, *data_len - start as u32) as usize;

    let mut i = 0;
    for i in start..(start + nelems) {
        if input[i] == *char {
            count[index] += 1;

        }
    }
}
