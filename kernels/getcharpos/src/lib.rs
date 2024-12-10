#![cfg_attr(target_arch = "spirv", no_std)]
#![deny(warnings)]

use glam::UVec3;
use spirv_std::{glam, spirv};

#[spirv(compute(threads(64)))]
pub fn main_getcharpos(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] x: &mut [u32],
) {
    x[id.x as usize] = 0;
}
