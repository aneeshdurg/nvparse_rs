#![feature(async_closure)]
use clap::Parser;
use memmap::MmapOptions;
use std::fs::File;

pub mod driver;

fn cpu_count_char(data: &[u8], char: u8) -> u32 {
    let mut acc = 0;
    // let mut pbar = tqdm::pbar(Some(data.len()));
    // let mut i = 0;
    // let mut last_update = 0;
    for c in data {
        acc += if *c == char { 1 } else { 0 };
        // i += 1;
        // if i % 1024 == 0 {
        //     let _ = pbar.update(1024);
        //     last_update = i;
        // }
    }
    // let _ = pbar.update(data.len() - last_update);
    acc
}

fn run_count_char(data: &[u8], char: u8) -> u32 {
    futures::executor::block_on(driver::run_charcount_shader(data, char)).unwrap()
}

fn count_char(data: &[u8], char: u8) -> Result<u32, Box<dyn std::error::Error>> {
    // if data.len() < (2 * nthreads) {
    //     // Insufficient parallelism, reduce on CPU
    //     Ok(cpu_count_char(data, char))
    // } else {
    Ok(run_count_char(data, char))
    // }
}

#[derive(Parser)]
struct Args {
    filename: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let file = File::open(&args.filename)?;
    let mmap = unsafe { MmapOptions::new().map(&file)? };

    let nlines = count_char(&mmap, b'\n')?;

    let timer = std::time::Instant::now();
    let cpures = cpu_count_char(&mmap, b'\n');
    eprintln!("CPU time: {:?} (res={})", timer.elapsed(), cpures);

    println!("{}", nlines);
    Ok(())
}
