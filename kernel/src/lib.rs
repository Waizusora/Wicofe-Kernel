#![no_std]

//! Initial foundation for the WICOFE monolithic kernel.
//!
//! The crate contains the initial freestanding kernel foundation. The x86-64
//! Limine BIOS image is linked and QEMU-boot-tested through PMM allocation/free;
//! AArch64 runtime boot, VMM, exceptions, SMP, and the kernel heap remain pending.

#[cfg(test)]
extern crate std;

pub mod arch;
pub mod boot;
pub mod hal;
pub mod memory;
pub mod pmm;
pub mod vmm;

/// Kernel identity used by early diagnostics and build metadata.
pub const KERNEL_NAME: &str = "WICOFE";

/// Current development version of the kernel crate.
pub const KERNEL_VERSION: &str = env!("CARGO_PKG_VERSION");
