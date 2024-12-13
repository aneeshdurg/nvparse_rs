use wgpu::util::DeviceExt;

use futures::channel::oneshot;
use std::ops::RangeBounds;
use std::sync::mpsc;
use std::thread;
use std::convert::TryInto;
use tqdm::pbar;
use wgpu::{Adapter, BufferAsyncError, Device, Queue, RequestDeviceError};

async fn init_adapter() -> Option<Adapter> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
}

async fn init_device(adapter: &Adapter) -> Result<(Device, Queue), RequestDeviceError> {
    adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("mydevice"),
                required_features: wgpu::Features::TIMESTAMP_QUERY
                    | wgpu::Features::MAPPABLE_PRIMARY_BUFFERS
                    | wgpu::Features::SPIRV_SHADER_PASSTHROUGH,
                required_limits: adapter.limits(),
                memory_hints: Default::default(),
            },
            None,
        )
        .await
}

fn store_u32(queue: &Queue, buffer: &wgpu::Buffer, value: u32) {
    let bytes_per_u32 = std::num::NonZero::<u64>::new(4).unwrap();
    let mut write_view = queue.write_buffer_with(&buffer, 0, bytes_per_u32).unwrap();
    write_view.as_mut().clone_from_slice(&value.to_ne_bytes());
}

fn bind_buffers_and_run(
    encoder: &mut wgpu::CommandEncoder,
    device: &Device,
    compute_pipeline: &wgpu::ComputePipeline,
    layout: &wgpu::BindGroupLayout,
    buffers: &[&wgpu::Buffer],
    workgroups: (u32, u32, u32),
) {
    let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: None,
        timestamp_writes: None,
    });

    let entries: &Vec<wgpu::BindGroupEntry<'_>> = &buffers
        .iter()
        .enumerate()
        .map(|(i, b)| wgpu::BindGroupEntry {
            binding: i as u32,
            resource: b.as_entire_binding(),
        })
        .collect();
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries,
    });
    cpass.set_bind_group(0, &bind_group, &[]);
    cpass.set_pipeline(&compute_pipeline);
    cpass.dispatch_workgroups(workgroups.0, workgroups.1, workgroups.2);
}

fn read_buffer<S: RangeBounds<wgpu::BufferAddress>>(
    device: &Device,
    buffer: &wgpu::Buffer,
    range: S,
) -> Vec<u32> {
    // Map the readback_buffer to the CPU
    let buffer_slice = buffer.slice(range);
    let (resolver, waiter) = oneshot::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
        resolver.send(res).unwrap();
    });
    // Wait for the buffer to be mapped and ready for reading
    device.poll(wgpu::Maintain::Wait);
    futures::executor::block_on(waiter)
        .unwrap()
        .expect("Mapping failed");

    // Copy from GPU to CPU
    let x = buffer_slice
        .get_mapped_range()
        .chunks_exact(4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
        .collect::<Vec<_>>();
    // Unmap the GPU buffer so that it can be re-used in the next iteration
    buffer.unmap();
    x
}

fn consume_buffer(
    nthreads: usize,
    total_len: usize,
    device: std::sync::Arc<Device>,
    queue: &Queue,
    input_bufs: &std::sync::Arc<Vec<wgpu::Buffer>>,
    char: u8,
    receiver: mpsc::Receiver<(usize, usize, usize)>,
    free_buffer: mpsc::Sender<usize>,
) -> u32 {
    let mut acc = 0;
    let mut compute_pbar = pbar(Some(total_len));

    let countchar_gen = countchar::codegen::new(&device, include_bytes!(env!("countchar.spv")));
    let _parsecsv_gen = parsecsv::codegen::new(&device, include_bytes!(env!("parsecsv.spv")));
    let getcharpos_gen = getcharpos::codegen::new(&device, include_bytes!(env!("getcharpos.spv")));

    let chunk_size_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Chunk size"),
        size: 4,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let char_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Character to match"),
        contents: &[char],
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let data_len_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("data_length"),
        size: 4,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let output_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("count (output)"),
        size: (nthreads * 4) as wgpu::BufferAddress,
        // Can be read to the CPU, and can be copied from the shader's storage buffer
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::MAP_READ
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    loop {
        let (offset, end, input_buf_id) = receiver.recv().unwrap();
        let data_len = (end - offset) as u32;
        // For storing a single u32 into a buffer, the intermediate copy isn't expensive
        store_u32(&queue, &data_len_buf, data_len);
        let chunk_size: u32 = (data_len / nthreads as u32) + 1;
        store_u32(&queue, &chunk_size_buf, chunk_size);

        // Create the compute pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("do compute"),
        });
        bind_buffers_and_run(
            &mut encoder,
            &device,
            &countchar_gen.compute_pipeline,
            &countchar_gen.bind_group_layout,
            &[
                &input_bufs[input_buf_id],
                &chunk_size_buf,
                &data_len_buf,
                &char_buf,
                &output_buf,
            ],
            (nthreads as u32 / countchar_gen.workgroup_dim.0, 1, 1),
        );

        // Run the queued computation
        queue.submit(Some(encoder.finish()));

        let nlines_per_thread = read_buffer(&device, &output_buf, ..);
        let nlines = nlines_per_thread.iter().fold(0, |acc, e| acc + *e);
        acc += nlines;

        let charpos_output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("charpos output"),
            size: ((nlines + 1) * 4) as wgpu::BufferAddress,
            // Can be read to the CPU, and can be copied from the shader's storage buffer
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::MAP_READ
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create the compute pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("get char positions"),
        });
        bind_buffers_and_run(
            &mut encoder,
            &device,
            &getcharpos_gen.compute_pipeline,
            &getcharpos_gen.bind_group_layout,
            &[
                &input_bufs[input_buf_id],
                &chunk_size_buf,
                &data_len_buf,
                &char_buf,
                &output_buf,
                &charpos_output_buf,
            ],
            (nthreads as u32 / 64, 1, 1),
        );

        // Run the queued computation
        queue.submit(Some(encoder.finish()));

        let _ = compute_pbar.update(data_len as usize);

        if end == total_len {
            break;
        }
        // Mark the input buffer as ready for writing again
        free_buffer
            .send(input_buf_id)
            .expect("semaphore add failed");
    }

    drop(compute_pbar);
    acc
}

pub async fn run_charcount_shader(
    input: &[u8],
    char: u8,
    nthreads: usize,
) -> Result<u32, BufferAsyncError> {
    let total_len = input.len();

    let adapter = init_adapter().await.expect("Failed to get adapter");
    let (device, queue) = init_device(&adapter)
        .await
        .expect("Failed to create device");
    let device = std::sync::Arc::new(device);

    let limits = adapter.limits();
    // eprintln!("LIMITS = {:?}", limits);
    // Using a smaller size here seems to have better performance. Maybe because it provides more
    // opportunities for compute to overlap with IO, hiding the latency?
    let max_buffer_size = limits.max_storage_buffer_binding_size / 16;
    println!("max_buffer_size {}", max_buffer_size);
    const N_INPUT_BUFS: usize = 8;
    let mut input_bufs = Vec::new();
    for i in 0..N_INPUT_BUFS {
        input_bufs.push(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("File Input {}", i)),
            size: max_buffer_size as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::MAP_WRITE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    let input_bufs = std::sync::Arc::new(input_bufs);

    // This channel marks input buffs in the vector above as "free" for writing or "allocated" for
    // compute. A producer will need to allocate buffers and transfer them to the consumer, which
    // will then free the buffer.
    let (free_buffer, allocate_buffer) = mpsc::channel();
    for i in 0..input_bufs.len() {
        free_buffer
            .send(i)
            .expect("semaphore initialization failed");
    }

    let (sender, receiver) = mpsc::channel();

    // Takes filled in buffers and run the compute kernel on the GPU
    let consumer = {
        let input_bufs = input_bufs.clone();
        let device = device.clone();
        thread::spawn(move || -> u32 {
            let timer = std::time::Instant::now();
            let res = consume_buffer(
                nthreads,
                total_len,
                device,
                &queue,
                &input_bufs,
                char,
                receiver,
                free_buffer,
            );
            eprintln!("GPU time: {:?} (res={})", timer.elapsed(), res);
            res
        })
    };

    // Copy chunks into buffers that aren't currently in-use
    let mut offset = 0;
    while offset < total_len {
        // Get a buffer that is not in use
        let input_buf_id = allocate_buffer.recv().unwrap();
        let end = std::cmp::min(offset + max_buffer_size as usize, total_len);
        let slice = &input[offset..end];

        let input_buf = &input_bufs[input_buf_id];
        // Map the input buffer into memory to avoid intermediate copying
        let (resolver, waiter) = oneshot::channel();
        let input_slice = input_buf.slice(0..(slice.len() as u64));
        input_slice.map_async(wgpu::MapMode::Write, move |res| {
            resolver.send(res).unwrap();
        });
        // Wait for the buffer to be mapped and ready for writing
        device.poll(wgpu::Maintain::Wait);
        waiter.await.unwrap().expect("mapping input buffer failed");
        input_slice.get_mapped_range_mut().clone_from_slice(slice);
        // Unmap the GPU buffer so that it can be used in the shader
        input_buf.unmap();

        sender
            .send((offset, end, input_buf_id))
            .expect("send failed");

        offset = end;
    }

    let acc = consumer.join().expect("Thread failed");
    Ok(acc)
}
