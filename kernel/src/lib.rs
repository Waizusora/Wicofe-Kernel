#![no_std]

//! Initial architecture-neutral foundation for the WICOFE kernel.
//!
//! This crate intentionally contains no boot protocol, allocator, panic handler,
//! global allocator, or architecture-specific entry point yet. Those contracts
//! must be designed before the kernel can be linked into a bootable image.

/// Kernel identity used by early diagnostics and build metadata.
pub const KERNEL_NAME: &str = "WICOFE";

/// Current development version of the kernel crate.
pub const KERNEL_VERSION: &str = env!("CARGO_PKG_VERSION");
