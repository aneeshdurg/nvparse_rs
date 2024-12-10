use wgpu::util::DeviceExt;

use futures::channel::oneshot;
use std::sync::mpsc;
use std::thread;
use std::{convert::TryInto, num::NonZeroU64};
use tqdm::pbar;
use wgpu::{Adapter, BufferAsyncError, Device, Queue, RequestDeviceError, ShaderModule};

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

fn load_shader_module(device: &Device, shader_bytes: &[u8]) -> ShaderModule {
    let spirv = std::borrow::Cow::Owned(wgpu::util::make_spirv_raw(shader_bytes).into_owned());
    let shader_binary = wgpu::ShaderModuleDescriptorSpirV {
        label: Some("mymodule"),
        source: spirv,
    };

    // Load the shaders from disk
    unsafe { device.create_shader_module_spirv(&shader_binary) }
}

fn store_u32(queue: &Queue, buffer: &wgpu::Buffer, value: u32) {
    let bytes_per_u32 = std::num::NonZero::<u64>::new(4).unwrap();
    let mut write_view = queue.write_buffer_with(&buffer, 0, bytes_per_u32).unwrap();
    write_view.as_mut().clone_from_slice(&value.to_ne_bytes());
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

    let shader_bytes: &[u8] = include_bytes!(env!("countchar.spv"));
    let module = load_shader_module(&device, shader_bytes);

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mybindgroup"),
        entries: &[
            // XXX - some graphics cards do not support empty bind layout groups, so
            // create a dummy entry.
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                },
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Uniform,
                },
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Uniform,
                },
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Uniform,
                },
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                },
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mylayout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("MyPipeline"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main_cc"),
        compilation_options: Default::default(),
        cache: None,
    });

    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback_buffer"),
        size: (nthreads * 4) as wgpu::BufferAddress,
        // Can be read to the CPU, and can be copied from the shader's storage buffer
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

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
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            let input_buf: &wgpu::Buffer = &input_bufs[input_buf_id];
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: input_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: chunk_size_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: data_len_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: char_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: output_buf.as_entire_binding(),
                    },
                ],
            });
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.set_pipeline(&compute_pipeline);
            cpass.dispatch_workgroups(nthreads as u32 / 64, 1, 1);
        }

        // copy the output into a CPU readable buffer
        encoder.copy_buffer_to_buffer(
            &output_buf,
            0,
            &readback_buffer,
            0,
            (nthreads * 4) as wgpu::BufferAddress,
        );

        // Run the queued computation
        queue.submit(Some(encoder.finish()));

        // Map the readback_buffer to the CPU
        let buffer_slice = readback_buffer.slice(..);
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
        readback_buffer.unmap();

        // TODO(aneesh) do this async, or on the GPU - this isn't the bottleneck though, copying is
        for v in &x {
            acc += v;
        }

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
            println!("Compute time: {:?}", timer.elapsed());
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
