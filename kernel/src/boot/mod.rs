//! Boot protocol boundary and normalized boot information.
//!
//! Protocol-specific response parsing belongs in architecture/boot adapters.
//! General kernel code should consume only validated WICOFE-owned structures.

mod info;

pub use info::{
    BootInfo, BootInfoError, BootModule, FramebufferInfo, KernelImage, MemoryRegion,
    MemoryRegionKind,
};
