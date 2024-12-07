use wgpu::util::DeviceExt;

use futures::channel::oneshot;
use std::{convert::TryInto, num::NonZeroU64};
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
                    | wgpu::Features::SPIRV_SHADER_PASSTHROUGH,
                required_limits: Default::default(),
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

pub async fn run_charcount_shader(
    input: &[u8],
    char: u8,
    nthreads: usize,
) -> Result<Vec<u32>, BufferAsyncError> {
    let timer = std::time::Instant::now();
    eprintln!("run_charcount_shader 0 {:?}", timer.elapsed());
    let adapter = init_adapter().await.expect("Failed to get adapter");
    let (device, queue) = init_device(&adapter)
        .await
        .expect("Failed to create device");

    let limits = adapter.limits();
    eprintln!("LIMITS = {:?}", limits);
    let shader_bytes: &[u8] = include_bytes!(env!("countchar.spv"));
    let module = load_shader_module(&device, shader_bytes);

    eprintln!("run_charcount_shader 1 {:?}", timer.elapsed());
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

    let s = limits.max_storage_buffer_binding_size / 16;
    // let s = 4096;

    let input_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("File Input"),
        size: s as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
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

    let mut offset = 0;
    let mut res: Vec<u32> = Vec::new();
    while offset < input.len() {
        let end = std::cmp::min(offset + s as usize, input.len());
        // if offset != 0 {
        //     // Map the input buffer into memory
        //     let (resolver, waiter) = oneshot::channel();
        //     input_buf
        //         .slice(..)
        //         .map_async(wgpu::MapMode::Write, move |res| {
        //             resolver.send(res).unwrap();
        //         });
        //     device.poll(wgpu::Maintain::Wait);
        //     waiter.await.unwrap()?;

        //     let (resolver, waiter) = oneshot::channel();
        //     data_len_buf
        //         .slice(..)
        //         .map_async(wgpu::MapMode::Write, move |res| {
        //             resolver.send(res).unwrap();
        //         });
        //     device.poll(wgpu::Maintain::Wait);
        //     waiter.await.unwrap()?;
        // }
        let slice = &input[offset..end];
        queue.write_buffer(&input_buf, 0, slice);
        queue.write_buffer(&data_len_buf, 0, &(slice.len() as u32).to_ne_bytes());
        let chunk_size: u32 = (slice.len() / nthreads) as u32 + 1;
        queue.write_buffer(&chunk_size_buf, 0, &chunk_size.to_ne_bytes());
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("GpuBuffer::write"),
        });
        queue.submit(Some(encoder.finish()));

        // Unmap buffers so that we can use them from the GPU
        // input_buf.unmap();
        // data_len_buf.unmap();

        eprintln!("run_charcount_shader 2 {:?}", timer.elapsed());
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

        eprintln!("run_charcount_shader 3 {:?}", timer.elapsed());
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.set_pipeline(&compute_pipeline);
            cpass.dispatch_workgroups(nthreads as u32, 1, 1);
        }

        eprintln!("run_charcount_shader 4 {:?}", timer.elapsed());
        encoder.copy_buffer_to_buffer(
            &output_buf,
            0,
            &readback_buffer,
            0,
            (nthreads * 4) as wgpu::BufferAddress,
        );

        eprintln!("run_charcount_shader 5 {:?}", timer.elapsed());
        queue.submit(Some(encoder.finish()));
        eprintln!("run_charcount_shader 6 {:?}", timer.elapsed());
        let buffer_slice = readback_buffer.slice(..);

        eprintln!("run_charcount_shader 7 {:?}", timer.elapsed());
        let (resolver, waiter) = oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
            resolver.send(res).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);
        eprintln!("run_charcount_shader 8 {:?}", timer.elapsed());

        waiter.await.unwrap()?;

        eprintln!("run_charcount_shader 9 {:?}", timer.elapsed());
        let x = buffer_slice
            .get_mapped_range()
            .chunks_exact(4)
            .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
            .collect::<Vec<_>>();

        // TODO(aneesh) do this async, or on the GPU
        let mut acc = 0;
        for v in &x {
            acc += v;
        }
        res.push(acc);

        readback_buffer.unmap();
        eprintln!("bytes={}-{}/{} acc={}", offset, end, input.len(), acc);
        eprintln!("  x={:?}", x);
        offset = end;
    }
    Ok(res)
}
