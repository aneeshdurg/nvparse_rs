#![cfg_attr(target_arch = "spirv", no_std)]
use glam::UVec3;
use kernelcodegen::generate_kernel;
use spirv_std::{glam, spirv};

#[spirv(compute(threads(64)))]
#[generate_kernel()]
pub fn main_cc(
    #[spirv(global_invocation_id)] id: UVec3,
    // residual is the buffer from the previous iteration - it's possible that there's some line at the
    // end of the buffer that is incomplete (i.e. the first line is =
    //  residual[residual_offset:] + input[:line_start_offsets[0]])
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _residual: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] _residual_len: &u32,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] _residual_line_start_offsets: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] _input: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 6)] _char: &u8,
    // min(chunk_lines, line_start_offsets.len() - chunk_lines * id.x) is the number of lines to
    // process per thread
    #[spirv(uniform, descriptor_set = 0, binding = 7)] _chunk_lines: &u32,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 8)] _line_start_offsets: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 9)] parsed: &mut [u32],
) {
    let index = id.x as usize;
    parsed[index] = 0;
}
