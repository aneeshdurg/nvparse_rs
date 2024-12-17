use wgpu::util::DeviceExt;

use futures::channel::oneshot;
use std::convert::TryInto;
use std::ops::RangeBounds;
use std::sync::mpsc;
use std::thread;
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
    // let mut required_limits = adapter.limits();
    // required_limits.max_storage_buffer_binding_size = 2<<30 - 1;
    // required_limits.max_buffer_size = 2<<30 - 1;
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
    total_len: usize,
    device: std::sync::Arc<Device>,
    queue: &Queue,
    input_bufs: &std::sync::Arc<Vec<wgpu::Buffer>>,
    char: u8,
    receiver: mpsc::Receiver<(usize, usize, usize)>,
    free_buffer: mpsc::Sender<usize>,
) -> u32 {
    let limits = device.limits();

    let mut acc = 0;
    let mut compute_pbar = pbar(Some(total_len));

    let timer = std::time::Instant::now();

    let countchar_gen = countchar::codegen::new(&device, include_bytes!(env!("countchar.spv")));
    let parsecsv_gen = parsecsv::codegen::new(&device, include_bytes!(env!("parsecsv.spv")));
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

    let delimeter_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Character to match"),
        contents: &[b'|'],
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
        size: (countchar_gen.workgroup_dim.0 * 4) as wgpu::BufferAddress,
        // Can be read to the CPU, and can be copied from the shader's storage buffer
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::MAP_READ
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let nlines_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("nlines_per_thread"),
        size: (countchar_gen.workgroup_dim.0 * 4) as wgpu::BufferAddress,
        // Can be read to the CPU, and can be copied from the shader's storage buffer
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::MAP_READ
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let setup_dur = timer.elapsed();
    let mut encoder_dur = std::time::Duration::ZERO;
    let mut submit_dur = std::time::Duration::ZERO;
    let mut output_dur = std::time::Duration::ZERO;
    let mut wait_dur = std::time::Duration::ZERO;
    let mut write_uniform_dur = std::time::Duration::ZERO;

    let mut max_chunk_size = 0;

    loop {
        let timer = std::time::Instant::now();
        let (offset, end, input_buf_id) = receiver.recv().unwrap();
        wait_dur += timer.elapsed();
        let timer = std::time::Instant::now();
        let data_len = (end - offset) as u32;
        // For storing a single u32 into a buffer, the intermediate copy isn't expensive
        store_u32(&queue, &data_len_buf, data_len);
        let n_dispatches = std::cmp::min(
            1 + data_len / countchar_gen.workgroup_dim.0,
            limits.max_compute_workgroups_per_dimension,
        );
        let chunk_size: u32 = data_len / (n_dispatches * countchar_gen.workgroup_dim.0) + 1;
        max_chunk_size = std::cmp::max(chunk_size, max_chunk_size);
        store_u32(&queue, &chunk_size_buf, chunk_size);
        write_uniform_dur += timer.elapsed();

        let timer = std::time::Instant::now();

        // Create the compute pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("do compute"),
        });
        let dispatch = (n_dispatches, 1, 1);
        // eprintln!("dispatch={:?} chunk_size={:?} data_len={:?}", dispatch, chunk_size, data_len);
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
            dispatch,
        );

        encoder.copy_buffer_to_buffer(&output_buf, 0, &nlines_buf, 0, nlines_buf.size());

        encoder_dur += timer.elapsed();
        let timer = std::time::Instant::now();

        // Run the queued computation
        queue.submit(Some(encoder.finish()));

        submit_dur += timer.elapsed();

        let output_timer = std::time::Instant::now();
        let nlines_per_thread = read_buffer(&device, &output_buf, ..);
        let nlines = nlines_per_thread.iter().fold(0, |acc, e| acc + *e);
        acc += nlines;
        output_dur += output_timer.elapsed();

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
                &nlines_buf,
                &output_buf,
                &charpos_output_buf,
            ],
            dispatch,
        );

        // Run the queued computation
        queue.submit(Some(encoder.finish()));

        device.poll(wgpu::Maintain::Wait);
        eprintln!("Staring encode for parsecsv");

        let lines_per_thread = std::cmp::max(1, nlines / (dispatch.0 * parsecsv_gen.workgroup_dim.0));
        store_u32(&queue, &chunk_size_buf, lines_per_thread);

        eprintln!("parsecsv.0");

        let col0output_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("parsed column0 output"),
            size: ((nlines + 1) * 4) as wgpu::BufferAddress,
            // Can be read to the CPU, and can be copied from the shader's storage buffer
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::MAP_READ
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        eprintln!("parsecsv.1");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("parse CSV"),
        });
        bind_buffers_and_run(
            &mut encoder,
            &device,
            &parsecsv_gen.compute_pipeline,
            &parsecsv_gen.bind_group_layout,
            &[
                &input_bufs[input_buf_id + 1],
                &data_len_buf,
                &input_bufs[input_buf_id],
                &data_len_buf,
                &delimeter_buf,
                &chunk_size_buf,
                &charpos_output_buf,
                &col0output_buf,
            ],
            dispatch,
        );
        eprintln!("parsecsv.2");

        // Run the queued computation
        queue.submit(Some(encoder.finish()));

        eprintln!("parsecsv.3");
        let x = read_buffer(&device, &col0output_buf, ..);
        for el in x {
            println!("{}", el);
        }
        eprintln!("parsecsv.4");

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

    eprintln!("setup_dur: {:?}", setup_dur);
    eprintln!("wait_dur: {:?}", setup_dur);
    eprintln!("write_uniform_dur: {:?}", encoder_dur);
    eprintln!("encoder_dur: {:?}", encoder_dur);
    eprintln!("submit_dur: {:?}", submit_dur);
    eprintln!("output_dur: {:?}", output_dur);
    eprintln!("max_chunk_size: {:?}", max_chunk_size);

    acc
}

pub async fn run_charcount_shader(input: &[u8], char: u8) -> Result<u32, BufferAsyncError> {
    let total_len = input.len();

    let adapter = init_adapter().await.expect("Failed to get adapter");
    let (device, queue) = init_device(&adapter)
        .await
        .expect("Failed to create device");
    let device = std::sync::Arc::new(device);

    let limits = device.limits();
    // eprintln!("LIMITS = {:?}", limits);
    // Using a smaller size here seems to have better performance. Maybe because it provides more
    // opportunities for compute to overlap with IO, hiding the latency?
    let max_buffer_size = limits.max_storage_buffer_binding_size / 8;
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
    // This channel is used to send tasks from the producer to the consumer. Each task includes a
    // buffer id to identify which buffer should be bound to the GPU's compute pipeline
    let (sender, receiver) = mpsc::channel();

    // Takes filled in buffers and run the compute kernel on the GPU
    let consumer = {
        let input_bufs = input_bufs.clone();
        let device = device.clone();
        thread::spawn(move || -> u32 {
            let timer = std::time::Instant::now();
            let res = consume_buffer(
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

    let mut write_time = std::time::Duration::ZERO;

    // Copy chunks into buffers that aren't currently in-use
    let mut offset = 0;
    while offset < total_len {
        // Get a buffer that is not in use
        let input_buf_id = allocate_buffer.recv().unwrap();
        let end = std::cmp::min(offset + max_buffer_size as usize, total_len);
        let slice = &input[offset..end];

        let timer = std::time::Instant::now();
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
        write_time += timer.elapsed();

        sender
            .send((offset, end, input_buf_id))
            .expect("send failed");

        offset = end;
    }

    eprintln!("write time: {:?}", write_time);

    let acc = consumer.join().expect("Thread failed");
    Ok(acc)
}
