use wgpu::util::DeviceExt;

use futures::channel::oneshot;
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
) -> Result<u32, BufferAsyncError> {
    let timer = std::time::Instant::now();
    let adapter = init_adapter().await.expect("Failed to get adapter");
    let (device, queue) = init_device(&adapter)
        .await
        .expect("Failed to create device");

    let limits = adapter.limits();
    // eprintln!("LIMITS = {:?}", limits);
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

    // TODO(aneesh) why do we need the arbitrary constant to reduce the size by? Using the value
    // from limits directly throws an error saying that it's too big.
    let max_buffer_size = limits.max_storage_buffer_binding_size / 16;

    let input_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("File Input"),
        size: max_buffer_size as wgpu::BufferAddress,
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

    eprintln!("Initialization time: {:?}", timer.elapsed());

    let timer = std::time::Instant::now();

    let mut offset = 0;
    let mut acc = 0;
    let mut pbar = pbar(Some(input.len()));
    while offset < input.len() {
        let end = std::cmp::min(offset + max_buffer_size as usize, input.len());
        // The data to operate on for this iteration
        let slice = &input[offset..end];

        // Write the slice, length of slice, and number of elements per thread to the GPU buffers
        let mut write_view = queue
            .write_buffer_with(&input_buf, 0, (slice.len() as u64).try_into().unwrap())
            .unwrap();
        write_view.as_mut().clone_from_slice(slice);
        drop(write_view);

        let data_len: u32 = slice.len() as u32;
        let mut write_view = queue
            .write_buffer_with(&data_len_buf, 0, std::num::NonZero::<u64>::new(4).unwrap())
            .unwrap();
        write_view
            .as_mut()
            .clone_from_slice(&data_len.to_ne_bytes());
        drop(write_view);

        let chunk_size: u32 = (slice.len() / nthreads) as u32 + 1;
        let mut write_view = queue
            .write_buffer_with(
                &chunk_size_buf,
                0,
                std::num::NonZero::<u64>::new(4).unwrap(),
            )
            .unwrap();
        write_view
            .as_mut()
            .clone_from_slice(&chunk_size.to_ne_bytes());
        drop(write_view);
        // Note that the buffers above aren't actually "written" until queue.submit is called

        // Create the compute pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("do compute"),
        });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.set_pipeline(&compute_pipeline);
            cpass.dispatch_workgroups(nthreads as u32, 1, 1);
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
        waiter.await.unwrap()?;

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

        offset = end;
        let _ = pbar.update(slice.len());
    }
    drop(pbar);
    println!("Compute time: {:?}", timer.elapsed());
    Ok(acc)
}
