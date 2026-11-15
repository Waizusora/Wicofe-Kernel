//! Limine bootstrap entry.
//!
//! This entry consumes protocol responses into fixed-capacity kernel-owned
//! storage. It still fail-stops after validation because diagnostics,
//! exception vectors, and memory management are not initialized.

use super::consume_limine_boot_info;
use super::info::{BootModule, MemoryRegion, MemoryRegionKind};
use super::limine::{
    LimineExecutableAddressResponse, LimineHhdmResponse, LimineMemmapResponse, LimineModuleResponse,
};
use super::requests::{
    LIMINE_BASE_REVISION_TAG, LIMINE_EXECUTABLE_ADDRESS_REQUEST, LIMINE_HHDM_REQUEST,
    LIMINE_MEMMAP_REQUEST, LIMINE_MODULE_REQUEST,
};

const EARLY_MEMORY_MAP_CAPACITY: usize = 256;
const EARLY_MODULE_CAPACITY: usize = 64;

const EMPTY_MEMORY_REGION: MemoryRegion = MemoryRegion {
    base: 0,
    length: 0,
    kind: MemoryRegionKind::Unknown,
    attributes: 0,
};
const EMPTY_BOOT_MODULE: BootModule = BootModule {
    physical_base: 0,
    length: 0,
};

// SAFETY: The bootstrap is entered once on the boot CPU. These buffers are
// exclusively borrowed during the one-time normalization phase and are not
// accessed concurrently by any other execution context.
static mut EARLY_MEMORY_MAP: [MemoryRegion; EARLY_MEMORY_MAP_CAPACITY] =
    [EMPTY_MEMORY_REGION; EARLY_MEMORY_MAP_CAPACITY];
static mut EARLY_MODULES: [BootModule; EARLY_MODULE_CAPACITY] =
    [EMPTY_BOOT_MODULE; EARLY_MODULE_CAPACITY];

unsafe extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

/// Rust entry called by the architecture-specific _start trampoline.
///
/// This function must never return to Limine's invalid return address.
#[unsafe(no_mangle)]
pub extern "C" fn wicofe_bootstrap() -> ! {
    // Resolve linker bounds without reading the bytes at either symbol.
    // SAFETY: The final linker scripts define both symbols as image boundaries.
    let (linker_start, linker_end) = unsafe {
        (
            core::ptr::addr_of!(__kernel_start) as u64,
            core::ptr::addr_of!(__kernel_end) as u64,
        )
    };

    // SAFETY: Limine has completed the handoff. The response records and all
    // nested pointers must be mapped/readable under the active HHDM. The
    // bootstrap is single-entry, so these static buffers are exclusively used.
    let boot_info = unsafe {
        let memory_map_storage = &mut *core::ptr::addr_of_mut!(EARLY_MEMORY_MAP);
        let module_storage = &mut *core::ptr::addr_of_mut!(EARLY_MODULES);

        consume_limine_boot_info(
            LIMINE_BASE_REVISION_TAG.is_supported(),
            LIMINE_MEMMAP_REQUEST
                .response_ptr()
                .cast::<LimineMemmapResponse>(),
            LIMINE_HHDM_REQUEST
                .response_ptr()
                .cast::<LimineHhdmResponse>(),
            LIMINE_EXECUTABLE_ADDRESS_REQUEST
                .response_ptr()
                .cast::<LimineExecutableAddressResponse>(),
            LIMINE_MODULE_REQUEST
                .response_ptr()
                .cast::<LimineModuleResponse>(),
            linker_start,
            linker_end,
            memory_map_storage,
            module_storage,
        )
    };

    // Keep the validated borrowed view alive until a later stage takes
    // ownership of early memory reservations. No bootloader memory is freed.
    if let Ok(_validated_boot_info) = boot_info {
        loop {
            core::hint::spin_loop();
        }
    }

    // Fail-stop on any malformed or unsupported handoff. A serial/firmware
    // diagnostic path will replace this silent halt in a later milestone.
    loop {
        core::hint::spin_loop();
    }
}
