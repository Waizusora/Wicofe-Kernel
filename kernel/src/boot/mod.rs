//! Boot protocol boundary and normalized boot information.
//!
//! Protocol-specific response parsing belongs in isolated adapters. General
//! kernel code should consume only validated WICOFE-owned structures.

mod info;
mod limine;
mod validate;

pub use info::{
    BootInfo, BootInfoError, BootModule, FramebufferInfo, KernelImage, KernelImageBoundsError,
    MemoryRegion, MemoryRegionKind,
};
pub use limine::{
    ExecutableAddress, HhdmOffset, LIMINE_MEMMAP_ACPI_NVS, LIMINE_MEMMAP_ACPI_RECLAIMABLE,
    LIMINE_MEMMAP_BAD_MEMORY, LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE,
    LIMINE_MEMMAP_EXECUTABLE_AND_MODULES, LIMINE_MEMMAP_FRAMEBUFFER, LIMINE_MEMMAP_RESERVED,
    LIMINE_MEMMAP_RESERVED_MAPPED, LIMINE_MEMMAP_USABLE, LimineAdapterError,
    LimineExecutableAddressResponse, LimineFile, LimineHhdmResponse, LimineMemmapEntry,
    LimineMemmapResponse, LimineModuleResponse, LimineUuid, normalize_executable_address_response,
    normalize_hhdm_response, normalize_memory_kind, normalize_memory_map_response,
    normalize_module_response,
};
pub use validate::{MemoryMapError, validate_memory_map};
