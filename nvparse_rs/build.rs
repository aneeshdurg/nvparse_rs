use spirv_builder::{Capability, SpirvBuilder};

fn main() {
    for kernel in std::fs::read_dir("../kernels").expect("Error finding kernels folder") {
        let path = kernel.expect("Invalid path in kernels folder").path();
        let compile_res = SpirvBuilder::new(&path, "spirv-unknown-vulkan1.1")
            .capability(Capability::Int8)
            .build()
            .expect("Kernel failed to compile");

        eprintln!("COMPILE RESULT: {:?}", compile_res);
    }
}
