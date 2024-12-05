use wgpu::util::DeviceExt;

use futures::channel::oneshot;
use std::{convert::TryInto, num::NonZeroU64};
use wgpu::{BufferAsyncError, Device, Queue, RequestDeviceError, ShaderModule};

async fn init_device() -> Result<(Device, Queue), RequestDeviceError> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .expect("Failed to find an appropriate adapter");

    adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
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
        label: None,
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
    let (device, queue) = init_device().await.expect("Failed to create device");
    let shader_bytes: &[u8] = include_bytes!(env!("countchar.spv"));
    let module = load_shader_module(&device, shader_bytes);

    eprintln!("run_charcount_shader 1 {:?}", timer.elapsed());
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                },
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                },
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                count: None,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
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
        label: None,
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main_cc"),
        compilation_options: Default::default(),
        cache: None,
    });

    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (nthreads * 4) as wgpu::BufferAddress,
        // Can be read to the CPU, and can be copied from the shader's storage buffer
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let input_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("File Input"),
        contents: input,
        usage: wgpu::BufferUsages::STORAGE,
    });

    let chunk_size = (input.len() / nthreads) as u32 + 1;
    let chunk_size_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Chunk size"),
        contents: &(chunk_size as u32).to_ne_bytes(),
        usage: wgpu::BufferUsages::STORAGE,
    });
    eprintln!("  chunk_size={:?}", chunk_size);

    let char_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Character to match"),
        contents: &(char as u32).to_ne_bytes(),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let data_len_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("data_length"),
        contents: &(input.len() as u32).to_ne_bytes(),
        usage: wgpu::BufferUsages::STORAGE,
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
    let x = Ok(buffer_slice
        .get_mapped_range()
        .chunks_exact(4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
        .collect::<Vec<_>>());
    x
}
