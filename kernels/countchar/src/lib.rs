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

fn to_bytes(a: u32) -> [u32; 4] {
    let mut v = [0u32; 4];
    let mut a = a;
    v[0] = a % 256;
    a /= 256;
    v[1] = a % 256;
    a /= 256;
    v[2] = a % 256;
    a /= 256;
    v[3] = a % 256;
    v
}

// LocalSize/numthreads of (x = 64, y = 1, z = 1)
#[spirv(compute(threads(64)))]
pub fn main_cc(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] input: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] chunk_size: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] data_len: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] char: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] count: &mut [u32],
) {
    let index = id.x as usize;
    count[index] = 0;

    let mut start: usize = index * (chunk_size[0] as usize);
    let mut nelems: usize = min(chunk_size[0], data_len[0] - start as u32) as usize;

    let rem = start % 4;
    if rem != 0 {
        let src: u32 = input[start / 4];

        let c = to_bytes(src);
        for i in 0..4 {
            if rem <= i {
                if (c[i] as u32) == char[0] {
                    count[index] += 1;
                }
            }
        }

        start = start + 4 - rem;
        nelems = nelems - (4 - rem);
    }

    let mut i = 0;
    while i < nelems {
        let src: u32 = input[(start + i) / 4];
        let c = to_bytes(src);
        for j in 0..4 {
            if (i + j) < nelems && (c[j] as u32) == char[0] {
                count[index] += 1;
            }
        }

        i += 4;
    }
}
