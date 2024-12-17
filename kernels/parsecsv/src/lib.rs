#![cfg_attr(target_arch = "spirv", no_std)]
use glam::UVec3;
use kernelcodegen::generate_kernel;
use spirv_std::{glam, spirv};

fn parse_u32(input: &[u8], start_offset: usize, end_offset: usize) -> u32 {
    let mut val: u32 = 0;
    for i in start_offset..end_offset {
        let b = input[i];
        val *= 10;
        if b < b'0' || b > b'9' {
            return u32::max_value();
        }
        val += (b - b'0') as u32;
    };
    val
}

#[generate_kernel()]
#[spirv(compute(threads(256)))]
pub fn main_cc(
    #[spirv(global_invocation_id)] id: UVec3,
    // residual is the buffer from the previous iteration - it's possible that there's some line at the
    // end of the buffer that is incomplete (i.e. the first line is =
    //  residual[residual_offset:] + input[:line_start_offsets[0]])
    #[spirv(storage_buffer, descriptor_set = 0, binding = 0)] _residual: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 1)] _residual_len: &u32,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] input: &mut [u8],
    #[spirv(uniform, descriptor_set = 0, binding = 3)] input_len: &u32,
    #[spirv(uniform, descriptor_set = 0, binding = 4)] delimiter: &u8,
    // min(chunk_lines, line_start_offsets.len() - chunk_lines * id.x) is the number of lines to
    // process per thread
    #[spirv(uniform, descriptor_set = 0, binding = 5)] chunk_lines: &u32,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 6)] line_start_offsets: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 7)] parsed: &mut [u32],
) {
    let index = (id.x * *chunk_lines) as usize;
    for i in 0..(*chunk_lines as usize) {
        if (index + i) >= (line_start_offsets.len() - 1) {
            break;
        }

        let start_offset = line_start_offsets[index + i] as usize;
        let res = if start_offset == 0 {
            // TODO access fields from residual
            u32::max_value()
        } else {
            // start_offset as u32
            let mut end_offset = start_offset;
            let mut found = true;
            let mut s = 0;
            while input[end_offset] != *delimiter && input[end_offset] != b'\n' {
                end_offset += 1;
                s += 1;
                if end_offset >= (*input_len as usize) {
                    found = false;
                    break;
                }
            }

            if found {
                parse_u32(input, start_offset, end_offset)
            } else {
                1111000000 + (start_offset as u32)
            }
        };
        parsed[index + i] = res;
    }
}
