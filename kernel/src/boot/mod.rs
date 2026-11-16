//! Boot protocol boundary and normalized boot information.
//!
//! Protocol-specific response parsing belongs in isolated adapters. General
//! kernel code should consume only validated WICOFE-owned structures.

mod consume;
mod early_memory;
mod handoff;
mod info;
mod limine;
mod requests;
mod reservations;
mod validate;

#[cfg(target_os = "none")]
mod entry;

pub use consume::{LimineBootInfoError, consume_limine_boot_info};
pub use early_memory::{EarlyMemoryPlanError, EarlyMemoryRange, build_early_memory_plan};
pub use handoff::{BootHandoffError, validate_limine_handoff};
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
pub use requests::{
    BaseRevisionTag, LIMINE_BASE_REVISION, LIMINE_BASE_REVISION_TAG, LIMINE_COMMON_MAGIC,
    LIMINE_EXECUTABLE_ADDRESS_REQUEST, LIMINE_FRAMEBUFFER_REQUEST, LIMINE_HHDM_REQUEST,
    LIMINE_MEMMAP_REQUEST, LIMINE_MODULE_REQUEST, LIMINE_STACK_SIZE_REQUEST, LimineRequest,
    LimineStackSizeRequest,
};
pub use reservations::{
    BootReservation, BootReservationKind, ReservationError, build_boot_reservations,
    is_usable_unreserved_range,
};
pub use validate::{MemoryMapError, validate_memory_map};
