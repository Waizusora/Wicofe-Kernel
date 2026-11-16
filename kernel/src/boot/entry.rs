//! Limine bootstrap entry.
//!
//! This entry consumes protocol responses into fixed-capacity kernel-owned
//! storage. It still fail-stops after validation because diagnostics,
//! exception vectors, and memory management are not initialized.

use super::consume_limine_boot_info;
use super::{
    BootInfo, BootReservation, EarlyMemoryRange, build_boot_reservations, build_early_memory_plan,
    plan_pmm_metadata_placement,
};
use super::info::{BootModule, MemoryRegion, MemoryRegionKind};
use super::limine::{
    LimineExecutableAddressResponse, LimineHhdmResponse, LimineMemmapResponse, LimineModuleResponse,
};
use crate::pmm::{Pmm, PmmError};
use core::num::NonZeroU64;

use super::requests::{
    LIMINE_BASE_REVISION_TAG, LIMINE_EXECUTABLE_ADDRESS_REQUEST, LIMINE_HHDM_REQUEST,
    LIMINE_MEMMAP_REQUEST, LIMINE_MODULE_REQUEST,
};

const EARLY_MEMORY_MAP_CAPACITY: usize = 256;
const EARLY_MODULE_CAPACITY: usize = 64;

const EARLY_MEMORY_PLAN_CAPACITY: usize = EARLY_MEMORY_MAP_CAPACITY;
const PMM_RANGE_CAPACITY: usize = EARLY_MEMORY_PLAN_CAPACITY;
const BOOT_RESERVATION_CAPACITY: usize = 2 + EARLY_MODULE_CAPACITY;

const EMPTY_EARLY_RANGE: EarlyMemoryRange = EarlyMemoryRange { base: 0, length: 0 };
const EMPTY_RESERVATION: BootReservation = BootReservation {
    base: 0,
    length: 0,
    kind: super::reservations::BootReservationKind::KernelImage,
};

static mut BOOT_RESERVATIONS: [BootReservation; BOOT_RESERVATION_CAPACITY] =
    [EMPTY_RESERVATION; BOOT_RESERVATION_CAPACITY];
static mut EARLY_MEMORY_PLAN: [EarlyMemoryRange; EARLY_MEMORY_PLAN_CAPACITY] =
    [EMPTY_EARLY_RANGE; EARLY_MEMORY_PLAN_CAPACITY];
static mut PMM_CANDIDATE_RANGES: [EarlyMemoryRange; PMM_RANGE_CAPACITY] =
    [EMPTY_EARLY_RANGE; PMM_RANGE_CAPACITY];

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

/// Build the validated reservation/early-memory pipeline and activate the
/// initial PMM using its physical metadata backing through the Limine HHDM.
///
/// # Safety
/// The bootloader HHDM must remain active and map the complete selected PMM
/// metadata extent as writable memory. The static planning buffers are part
/// of the kernel image and are reserved by the kernel-image reservation.
unsafe fn initialize_bootstrap_pmm(boot_info: &BootInfo<'_>) -> Result<Pmm<'static>, PmmError> {
    const PAGE_SIZE: NonZeroU64 = match NonZeroU64::new(4096) {
        Some(value) => value,
        None => panic!("page size must be non-zero"),
    };
    const FRAME_METADATA_SIZE: NonZeroU64 = match NonZeroU64::new(16) {
        Some(value) => value,
        None => panic!("frame metadata size must be non-zero"),
    };

    let hhdm_offset = boot_info.hhdm_offset.ok_or(PmmError::InvalidHhdmAddress)?;

    // SAFETY: The bootstrap executes once on the boot CPU before SMP startup.
    let reservations: &'static mut [BootReservation; BOOT_RESERVATION_CAPACITY] =
        unsafe { &mut *core::ptr::addr_of_mut!(BOOT_RESERVATIONS) };
    let reservation_count =
        build_boot_reservations(boot_info, reservations).map_err(|_| PmmError::InvalidRanges)?;

    // SAFETY: These static buffers are exclusively owned by this one-time
    // bootstrap invocation and are part of the reserved kernel image.
    let early_plan: &'static mut [EarlyMemoryRange; EARLY_MEMORY_PLAN_CAPACITY] =
        unsafe { &mut *core::ptr::addr_of_mut!(EARLY_MEMORY_PLAN) };
    let early_count = build_early_memory_plan(
        boot_info.memory_map,
        &reservations[..reservation_count],
        PAGE_SIZE,
        early_plan,
    )
    .map_err(|_| PmmError::InvalidRanges)?;

    // SAFETY: Same single-entry ownership guarantee as the early-plan buffer.
    let pmm_ranges: &'static mut [EarlyMemoryRange; PMM_RANGE_CAPACITY] =
        unsafe { &mut *core::ptr::addr_of_mut!(PMM_CANDIDATE_RANGES) };
    let (layout, pmm_range_count) = plan_pmm_metadata_placement(
        &early_plan[..early_count],
        PAGE_SIZE,
        FRAME_METADATA_SIZE,
        pmm_ranges,
    )
    .map_err(|_| PmmError::InvalidMetadataLayout)?;

    // SAFETY: Limine's HHDM response is active for this handoff. The selected
    // metadata extent is inside a validated Usable range and removed from the
    // candidate list before the PMM receives it. It is exclusively reserved
    // and remains mapped for the lifetime of this bootstrap PMM.
    unsafe {
        Pmm::from_hhdm(
            &pmm_ranges[..pmm_range_count],
            PAGE_SIZE,
            hhdm_offset,
            &layout,
        )
    }
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
    if let Ok(validated_boot_info) = boot_info {
        // SAFETY: This is the single Limine bootstrap CPU. No secondary CPU
        // or interrupt handler uses PMM before the kernel enables them.
        if let Ok(mut pmm) = unsafe { initialize_bootstrap_pmm(&validated_boot_info) } {
            // Keep the initialized allocator and its metadata borrows alive.
            // General allocation remains disabled until VMM and diagnostics
            // are initialized in later milestones.
            loop {
                core::hint::black_box(&mut pmm);
                core::hint::spin_loop();
            }
        }
    }

    // Fail-stop on any malformed or unsupported handoff. A serial/firmware
    // diagnostic path will replace this silent halt in a later milestone.
    loop {
        core::hint::spin_loop();
    }
}
