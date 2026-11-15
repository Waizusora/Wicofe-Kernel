//! Validation of the minimum Limine handoff contract.
//!
//! This layer checks only protocol negotiation and response availability. It
//! does not dereference bootloader pointers; response adapters perform that
//! work under an explicit unsafe memory-mapping/lifetime contract.

use core::ffi::c_void;

/// Required data missing or incompatible at the Limine handoff boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootHandoffError {
    UnsupportedBaseRevision,
    MissingMemoryMapResponse,
    MissingHhdmResponse,
    MissingExecutableAddressResponse,
}

/// Validate the minimum response set before any response is dereferenced.
///
/// Modules and framebuffer are optional. Memory map, HHDM and executable
/// address are mandatory for the current bootstrap design.
pub fn validate_limine_handoff(
    base_revision_supported: bool,
    memory_map_response: *mut c_void,
    hhdm_response: *mut c_void,
    executable_address_response: *mut c_void,
) -> Result<(), BootHandoffError> {
    if !base_revision_supported {
        return Err(BootHandoffError::UnsupportedBaseRevision);
    }
    if memory_map_response.is_null() {
        return Err(BootHandoffError::MissingMemoryMapResponse);
    }
    if hhdm_response.is_null() {
        return Err(BootHandoffError::MissingHhdmResponse);
    }
    if executable_address_response.is_null() {
        return Err(BootHandoffError::MissingExecutableAddressResponse);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    fn present() -> *mut c_void {
        8usize as *mut c_void
    }

    #[test]
    fn accepts_complete_required_handoff() {
        assert_eq!(
            validate_limine_handoff(true, present(), present(), present()),
            Ok(())
        );
    }

    #[test]
    fn rejects_unsupported_base_revision_first() {
        assert_eq!(
            validate_limine_handoff(false, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()),
            Err(BootHandoffError::UnsupportedBaseRevision)
        );
    }

    #[test]
    fn rejects_missing_memory_map() {
        assert_eq!(
            validate_limine_handoff(true, ptr::null_mut(), present(), present()),
            Err(BootHandoffError::MissingMemoryMapResponse)
        );
    }

    #[test]
    fn rejects_missing_hhdm() {
        assert_eq!(
            validate_limine_handoff(true, present(), ptr::null_mut(), present()),
            Err(BootHandoffError::MissingHhdmResponse)
        );
    }

    #[test]
    fn rejects_missing_executable_address() {
        assert_eq!(
            validate_limine_handoff(true, present(), present(), ptr::null_mut()),
            Err(BootHandoffError::MissingExecutableAddressResponse)
        );
    }
}
