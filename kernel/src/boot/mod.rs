//! Boot protocol boundary and normalized boot information.
//!
//! Protocol-specific response parsing belongs in isolated adapters. General
//! kernel code should consume only validated WICOFE-owned structures.

mod info;
mod limine;
mod validate;

pub use info::{
    BootInfo, BootInfoError, BootModule, FramebufferInfo, KernelImage, MemoryRegion,
    MemoryRegionKind,
};
pub use limine::{
    normalize_executable_address_response, normalize_hhdm_response,
    normalize_memory_kind, normalize_memory_map_response, ExecutableAddress,
    HhdmOffset, LimineAdapterError, LimineExecutableAddressResponse,
    LimineHhdmResponse, LimineMemmapEntry, LimineMemmapResponse,
    LIMINE_MEMMAP_ACPI_NVS, LIMINE_MEMMAP_ACPI_RECLAIMABLE,
    LIMINE_MEMMAP_BAD_MEMORY, LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE,
    LIMINE_MEMMAP_EXECUTABLE_AND_MODULES, LIMINE_MEMMAP_FRAMEBUFFER,
    LIMINE_MEMMAP_RESERVED, LIMINE_MEMMAP_RESERVED_MAPPED, LIMINE_MEMMAP_USABLE,
};
pub use validate::{validate_memory_map, MemoryMapError};
