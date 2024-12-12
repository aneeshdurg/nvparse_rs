#![cfg_attr(target_arch = "spirv", no_std)]

pub extern crate kernelcodegen_macros as macros;
pub use macros::generate_kernel;

#[cfg(not(target_arch = "spirv"))]
pub use kernelcodegen_types::ComputeKernel;
#[cfg(not(target_arch = "spirv"))]
pub extern crate wgpu;
