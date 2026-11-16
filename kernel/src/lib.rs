#![no_std]

//! Initial foundation for the WICOFE monolithic kernel.
//!
//! The crate is not yet a bootable kernel image. Protocol-specific entry
//! assembly, linker scripts, allocator implementations, and panic handling
//! are introduced in later boot-foundation milestones.

#[cfg(test)]
extern crate std;

pub mod arch;
pub mod boot;
pub mod hal;
pub mod memory;
pub mod pmm;

/// Kernel identity used by early diagnostics and build metadata.
pub const KERNEL_NAME: &str = "WICOFE";

/// Current development version of the kernel crate.
pub const KERNEL_VERSION: &str = env!("CARGO_PKG_VERSION");
