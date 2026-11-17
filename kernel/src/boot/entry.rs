//! Limine bootstrap entry.
//!
//! This entry consumes protocol responses into fixed-capacity kernel-owned
//! storage, reports early milestones over COM1 on x86-64, and initializes the
//! bootstrap PMM. It then fail-stops until VMM and exception handling exist.

use super::consume_limine_boot_info;
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
use super::{
    BootInfo, BootReservation, EarlyMemoryRange, build_boot_reservations, build_early_memory_plan,
    plan_pmm_metadata_placement,
};

#[cfg(target_arch = "x86_64")]
fn early_serial_out(port: u16, value: u8) {
    // SAFETY: The bootstrap runs in ring 0; COM1 is the conventional debug UART.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn early_serial_init() {
    early_serial_out(0x3f9, 0x00);
    early_serial_out(0x3fb, 0x80);
    early_serial_out(0x3f8, 0x01);
    early_serial_out(0x3f9, 0x00);
    early_serial_out(0x3fb, 0x03);
    early_serial_out(0x3fa, 0xc7);
    early_serial_out(0x3fc, 0x0b);
}

#[cfg(target_arch = "x86_64")]
fn early_serial_write_str(value: &str) {
    for byte in value.bytes() {
        if byte == b'\n' {
            early_serial_write_byte(b'\r');
        }
        early_serial_write_byte(byte);
    }
}

#[cfg(target_arch = "x86_64")]
fn early_serial_write_byte(byte: u8) {
    for _ in 0..100_000 {
        let status: u8;
        // SAFETY: Read the 16550 UART line-status register at COM1.
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") 0x3fd_u16,
                out("al") status,
                options(nomem, nostack, preserves_flags)
            );
        }
        if status & 0x20 != 0 {
            early_serial_out(0x3f8, byte);
            return;
        }
        core::hint::spin_loop();
    }
}

#[cfg(target_arch = "aarch64")]
fn early_serial_init() {}

#[cfg(target_arch = "aarch64")]
fn early_serial_write_str(_: &str) {}

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

/// Exercise one real PMM allocation/free cycle before enabling later stages.
/// The frame is tracked only in PMM metadata; this does not write to the frame.
fn verify_pmm_alloc_free(pmm: &mut Pmm<'_>) -> bool {
    const OWNER_TAG: u64 = 0x5749_434f_4645;
    let free_before = pmm.free_frames();
    let frame = match pmm.allocate_frame(OWNER_TAG) {
        Ok(frame) => frame,
        Err(_) => return false,
    };
    let metadata_valid = match pmm.frame_metadata(frame) {
        Ok(metadata) => metadata.owner_tag == OWNER_TAG,
        Err(_) => false,
    };
    let released = pmm.free_frame(frame).is_ok();
    metadata_valid && released && pmm.free_frames() == free_before && pmm.allocated_frames() == 0
}

/// Rust entry called by the architecture-specific _start trampoline.
///
/// This function must never return to Limine's invalid return address.
#[unsafe(no_mangle)]
pub extern "C" fn wicofe_bootstrap() -> ! {
    early_serial_init();
    early_serial_write_str("WICOFE: BOOTSTRAP_ENTERED\n");

    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: _start entered at ring 0 with interrupts disabled. The
        // inherited mappings cover the resident GDT, TSS, and IST stack.
        unsafe { crate::arch::x86_64::gdt::initialize() };
        early_serial_write_str("WICOFE: GDT_TSS_READY\n");

        if unsafe { crate::arch::x86_64::interrupts::initialize_and_load() }.is_err() {
            early_serial_write_str("WICOFE: IDT_INIT_FAILED\n");
            loop {
                // SAFETY: Initialization failure is terminal; IF remains clear.
                unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
            }
        }
        early_serial_write_str("WICOFE: IDT_READY\n");
        // SAFETY: Bootstrap runs at ring 0 with IF clear and no other CPU or
        // driver accesses the legacy PIC yet. Keep all IRQ lines masked.
        let pic_masks = unsafe { crate::arch::x86_64::pic::initialize() };
        if pic_masks.master != crate::arch::x86_64::pic::MASK_ALL
            || pic_masks.slave != crate::arch::x86_64::pic::MASK_ALL
        {
            early_serial_write_str("WICOFE: PIC_INIT_FAILED\\n");
            loop {
                // SAFETY: PIC initialization failure is terminal; IF remains clear.
                unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
            }
        }
        early_serial_write_str("WICOFE: PIC_READY_MASKED\\n");
        early_serial_write_str("WICOFE: IRQ_DISPATCH_READY_MASKED\\n");
    }
    // Resolve linker bounds without reading the bytes at either symbol.
    // SAFETY: The final linker scripts define both symbols as image boundaries.
    let (linker_start, linker_end) = (
        core::ptr::addr_of!(__kernel_start) as u64,
        core::ptr::addr_of!(__kernel_end) as u64,
    );

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
    match boot_info {
        Ok(validated_boot_info) => {
            early_serial_write_str("WICOFE: BOOTINFO_VALID\n");
            // SAFETY: This is the single Limine bootstrap CPU. No secondary CPU
            // or interrupt handler uses PMM before the kernel enables them.
            match unsafe { initialize_bootstrap_pmm(&validated_boot_info) } {
                Ok(mut pmm) => {
                    early_serial_write_str("WICOFE: PMM_READY\n");
                    if !verify_pmm_alloc_free(&mut pmm) {
                        early_serial_write_str("WICOFE: PMM_ALLOC_FREE_FAILED\n");
                    } else {
                        early_serial_write_str("WICOFE: PMM_ALLOC_FREE_OK\n");
                        #[cfg(feature = "exception-smoke-int3")]
                        {
                            early_serial_write_str("WICOFE: EXCEPTION_SMOKE_INT3\n");
                            // SAFETY: This build-only test intentionally raises #BP
                            // after the IDT and PMM are initialized. The fatal
                            // exception dispatcher must never return from this trap.
                            unsafe {
                                core::arch::asm!("int3");
                            }
                        }
                        #[cfg(feature = "exception-smoke-ud2")]
                        {
                            early_serial_write_str("WICOFE: EXCEPTION_SMOKE_UD2\n");
                            // SAFETY: This build-only test intentionally executes
                            // an invalid opcode to raise #UD after IDT and PMM setup.
                            unsafe {
                                core::arch::asm!("ud2");
                            }
                        }
                        #[cfg(feature = "exception-smoke-double-fault")]
                        {
                            early_serial_write_str("WICOFE: EXCEPTION_SMOKE_DOUBLE_FAULT\\n");
                            // SAFETY: This dedicated smoke build intentionally
                            // corrupts #UD/#GP selectors to force escalation to #DF.
                            unsafe {
                                crate::arch::x86_64::interrupts::prepare_double_fault_smoke();
                                core::arch::asm!("ud2");
                            }
                        }
                        #[cfg(feature = "exception-smoke-page-fault")]
                        {
                            const UNMAPPED_TEST_ADDRESS: usize = 0x0000_4000_0000_0000;
                            early_serial_write_str("WICOFE: EXCEPTION_SMOKE_PAGE_FAULT\n");
                            // SAFETY: This test-only canonical address is deliberately
                            // outside the kernel, bootloader, and HHDM mappings.
                            // The volatile read must raise #PF; the dispatcher records CR2.
                            unsafe {
                                core::ptr::read_volatile(UNMAPPED_TEST_ADDRESS as *const u64);
                            }
                        }
                        // Keep allocator metadata alive. General allocation remains
                        // disabled until VMM and diagnostics are initialized.
                        loop {
                            core::hint::black_box(&mut pmm);
                            core::hint::spin_loop();
                        }
                    }
                }
                Err(_) => early_serial_write_str("WICOFE: PMM_INIT_FAILED\n"),
            }
        }
        Err(_) => early_serial_write_str("WICOFE: BOOTINFO_INVALID\n"),
    }

    early_serial_write_str("WICOFE: BOOT_FAILED\n");
    loop {
        core::hint::spin_loop();
    }
}
