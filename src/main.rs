use clap::Parser;
use memmap::MmapOptions;
use std::fs::File;

pub mod driver;

fn cpu_count_char(data: &[u8], char: u8) -> u32 {
    let mut acc = 0;
    for c in data {
        acc += if *c == char { 1 } else { 0 };
    }
    acc
}

fn run_count_char(data: &[u8], char: u8, nthreads: usize) -> u32 {
    futures::executor::block_on(driver::run_charcount_shader(data, char, nthreads)).unwrap()
}

fn count_char(nthreads: usize, data: &[u8], char: u8) -> Result<u32, Box<dyn std::error::Error>> {
    if data.len() < (2 * nthreads) {
        // Insufficient parallelism, reduce on CPU
        Ok(cpu_count_char(data, char))
    } else {
        Ok(run_count_char(data, char, nthreads))
    }
}

#[derive(Parser)]
struct Args {
    filename: String,

    #[arg(short, long, default_value = "65535")]
    nthreads: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let file = File::open(&args.filename)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };

    let nlines = count_char(args.nthreads, &mmap, b'\n')?;
    println!("{}", nlines);
    Ok(())
}
