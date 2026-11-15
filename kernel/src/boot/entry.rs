//! Minimal freestanding Rust bootstrap entry.
//!
//! This stage validates that Limine negotiated the requested base revision and
//! supplied the mandatory response records. It intentionally stops afterward:
//! parsing/copying response data and initializing memory are later milestones.

use super::handoff::validate_limine_handoff;
use super::requests::{
    LIMINE_BASE_REVISION_TAG, LIMINE_EXECUTABLE_ADDRESS_REQUEST, LIMINE_HHDM_REQUEST,
    LIMINE_MEMMAP_REQUEST,
};

/// Rust entry called by the architecture-specific _start trampoline.
///
/// This function must never return to Limine's invalid return address.
#[unsafe(no_mangle)]
pub extern "C" fn wicofe_bootstrap() -> ! {
    let _handoff = validate_limine_handoff(
        LIMINE_BASE_REVISION_TAG.is_supported(),
        LIMINE_MEMMAP_REQUEST.response_ptr(),
        LIMINE_HHDM_REQUEST.response_ptr(),
        LIMINE_EXECUTABLE_ADDRESS_REQUEST.response_ptr(),
    );

    // Fail-stop until the next stage installs diagnostics and consumes the
    // response structures. No bootloader memory is reclaimed at this stage.
    loop {
        core::hint::spin_loop();
    }
}
