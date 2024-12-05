use clap::Parser;
use gpgpu::*;
use memmap::MmapOptions;
use std::fs::File;

pub mod example;

// fn get_size() -> Result<u32, Box<dyn std::error::Error>> {
//     if let Ok(size_var) = std::env::var("COMPUTESIZE") {
//         return Ok(size_var.parse()?);
//     };
//     Ok(10000000)
// }
//
fn cpu_reduce(data: Vec<u32>) -> u32 {
    let mut acc: u32 = 0;
    for d in data {
        acc = acc.wrapping_add(d);
    }
    acc
}

// fn reduce(data: Vec<u32>) -> Result<u32, Box<dyn std::error::Error>> {
//     /// Max number of threads per workgroup
//     const MAX_WG_SIZE: usize = 65535;
//
//     if data.len() < (2 * MAX_WG_SIZE) {
//         // Insufficient parallelism, reduce on CPU
//         Ok(cpu_reduce(data))
//     } else {
//         // Framework initialization
//         let fw = Framework::default();
//
//         let chunk_size = (data.len() / MAX_WG_SIZE) as u32 + 1;
//         // GPU buffer creation
//         let buf_a = GpuBuffer::<u32>::from_slice(&fw, &data);
//         let buf_chunk_size = GpuBuffer::<u32>::from_slice(&fw, &[chunk_size]);
//         let buf_data_len = GpuBuffer::<u32>::from_slice(&fw, &[data.len() as u32]);
//         let buf_c = GpuBuffer::<u32>::with_capacity(&fw, MAX_WG_SIZE as u64);
//
//         let shader = Shader::from_wgsl_file(&fw, "./sum.wgsl")?;
//         // Descriptor set and program creation
//         let desc = DescriptorSet::default()
//             .bind_buffer(&buf_a, GpuBufferUsage::ReadOnly)
//             .bind_buffer(&buf_chunk_size, GpuBufferUsage::ReadOnly)
//             .bind_buffer(&buf_data_len, GpuBufferUsage::ReadOnly)
//             .bind_buffer(&buf_c, GpuBufferUsage::ReadWrite);
//         let program = Program::new(&shader, "main").add_descriptor_set(desc); // Entry point
//         let kern = Kernel::new(&fw, program);
//
//         let mut buf_c_cpu = vec![0u32; MAX_WG_SIZE];
//         kern.enqueue(MAX_WG_SIZE as u32, 1, 1);
//         buf_c.read_blocking(&mut buf_c_cpu)?;
//         Ok(cpu_reduce(buf_c_cpu))
//     }
// }

fn cpu_count_char(data: &[u8], char: u8) -> u32 {
    let mut acc = 0;
    for c in data {
        acc += if *c == char { 1 } else { 0 };
    }
    acc
}

fn count_char(nthreads: usize, data: &[u8], char: u8) -> Result<u32, Box<dyn std::error::Error>> {
    if data.len() < (2 * nthreads) {
        // Insufficient parallelism, reduce on CPU
        Ok(cpu_count_char(data, char))
    } else {
        // Framework initialization
        let fw = Framework::default();

        // TODO get this dynamically from wgpu::Device::Limits
        const MAX_CAPACITY: u64 = 1073741824;

        // GPU buffer creation
        let buf_input = GpuBuffer::<u8>::with_capacity(&fw, MAX_CAPACITY);
        let buf_chunk_size = GpuBuffer::<u32>::with_capacity(&fw, 1);
        let buf_data_len = GpuBuffer::<u32>::with_capacity(&fw, 1);
        let buf_char = GpuBuffer::<u32>::from_slice(&fw, &[char as u32]);
        let buf_c = GpuBuffer::<u32>::with_capacity(&fw, nthreads as u64);

        let shader = Shader::from_wgsl_file(&fw, "./countchar.wgsl")?;

        // Descriptor set and program creation
        let desc = DescriptorSet::default()
            .bind_buffer(&buf_input, GpuBufferUsage::ReadOnly)
            .bind_buffer(&buf_chunk_size, GpuBufferUsage::ReadOnly)
            .bind_buffer(&buf_data_len, GpuBufferUsage::ReadOnly)
            .bind_buffer(&buf_char, GpuBufferUsage::ReadOnly)
            .bind_buffer(&buf_c, GpuBufferUsage::ReadWrite);
        let program = Program::new(&shader, "main").add_descriptor_set(desc); // Entry point
        let kern = Kernel::new(&fw, program);

        let mut acc = 0;
        let mut offset = 0;
        while offset < data.len() {
            let end = std::cmp::min(offset + (MAX_CAPACITY as usize), data.len());
            let data_len = end - offset;

            if data_len < (2 * nthreads) {
                acc += cpu_count_char(&data[offset..end], char);
                offset = end;
                continue;
            }

            buf_input.write(&data[offset..end])?;

            let chunk_size = (data_len / nthreads) as u32 + 1;
            buf_chunk_size.write(&[chunk_size])?;
            buf_data_len.write(&[data_len as u32])?;

            let mut buf_c_cpu = vec![0u32; nthreads];
            kern.enqueue(nthreads as u32, 1, 1);
            buf_c.read_blocking(&mut buf_c_cpu)?;
            // eprintln!("{:?}", buf_c_cpu);
            acc += cpu_reduce(buf_c_cpu);

            offset = end;
        }
        Ok(acc)
    }
}

#[derive(Parser)]
struct Args {
    filename: String,

    #[arg(short, long, default_value = "65535")]
    nthreads: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let cpu_data = (0..get_size()?).into_iter().collect::<Vec<u32>>();
    // if let Ok(val) = std::env::var("ONLYCPU") {
    //     if val == "1" {
    //         println!("{}", cpu_reduce(cpu_data));
    //         return Ok(());
    //     }
    // }
    // println!("{}", reduce(cpu_data)?);
    example::main();

    let args = Args::parse();

    let file = File::open(&args.filename)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };

    let nlines = count_char(args.nthreads, &mmap, b'\n')?;
    println!("{}", nlines);
    Ok(())
}
