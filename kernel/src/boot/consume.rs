//! End-to-end, allocation-free consumption of the mandatory Limine responses.
//!
//! This boundary turns protocol-specific pointers into borrowed, validated
//! WICOFE-owned boot information. It does not reclaim bootloader memory.

use core::ffi::c_void;

use super::{
    BootHandoffError, BootInfo, BootInfoError, BootModule, KernelImage, KernelImageBoundsError,
    LimineAdapterError, LimineExecutableAddressResponse, LimineHhdmResponse, LimineMemmapResponse,
    LimineModuleResponse, MemoryRegion, normalize_executable_address_response,
    normalize_hhdm_response, normalize_memory_map_response, normalize_module_response,
    validate_limine_handoff,
};

/// Failures while assembling normalized boot information from Limine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimineBootInfoError {
    Handoff(BootHandoffError),
    MemoryMap(LimineAdapterError),
    Hhdm(LimineAdapterError),
    ExecutableAddress(LimineAdapterError),
    Modules(LimineAdapterError),
    EmptyMemoryMap,
    KernelImageBounds(KernelImageBoundsError),
    ExecutableVirtualBaseMismatch,
    InvalidBootInfo(BootInfoError),
}

/// Consume Limine responses into caller-owned fixed storage.
///
/// A null module response is treated as an empty optional module list.
/// Framebuffer decoding and module metadata copying remain deferred.
///
/// The linker bounds must be the runtime virtual addresses of the kernel image
/// boundary symbols.
///
/// # Safety
/// The caller must establish that every non-null response pointer, every
/// nested memory-map/module pointer, and every referenced structure is mapped,
/// readable, correctly aligned, and live for this call. The Limine HHDM must
/// be active for all pointer dereferences. Output slices must remain
/// exclusively borrowed for the returned BootInfo lifetime. Null/alignment
/// checks cannot prove that an arbitrary non-null address is readable.
#[allow(clippy::too_many_arguments)]
pub unsafe fn consume_limine_boot_info<'a>(
    base_revision_supported: bool,
    memory_map_response: *const LimineMemmapResponse,
    hhdm_response: *const LimineHhdmResponse,
    executable_address_response: *const LimineExecutableAddressResponse,
    module_response: *const LimineModuleResponse,
    linker_virtual_start: u64,
    linker_virtual_end: u64,
    memory_map_storage: &'a mut [MemoryRegion],
    module_storage: &'a mut [BootModule],
) -> Result<BootInfo<'a>, LimineBootInfoError> {
    validate_limine_handoff(
        base_revision_supported,
        memory_map_response.cast_mut().cast::<c_void>(),
        hhdm_response.cast_mut().cast::<c_void>(),
        executable_address_response.cast_mut().cast::<c_void>(),
    )
    .map_err(LimineBootInfoError::Handoff)?;

    // SAFETY: The caller upholds the response-pointer contract above.
    let hhdm =
        unsafe { normalize_hhdm_response(hhdm_response) }.map_err(LimineBootInfoError::Hhdm)?;
    // SAFETY: The caller upholds the response-pointer contract above.
    let executable = unsafe { normalize_executable_address_response(executable_address_response) }
        .map_err(LimineBootInfoError::ExecutableAddress)?;

    if executable.virtual_base.get() != linker_virtual_start {
        return Err(LimineBootInfoError::ExecutableVirtualBaseMismatch);
    }

    // SAFETY: The caller upholds the response-pointer contract above.
    let memory_map_count =
        unsafe { normalize_memory_map_response(memory_map_response, memory_map_storage) }
            .map_err(LimineBootInfoError::MemoryMap)?;
    if memory_map_count == 0 {
        return Err(LimineBootInfoError::EmptyMemoryMap);
    }

    let module_count = if module_response.is_null() {
        0
    } else {
        // SAFETY: The caller upholds the optional response-pointer contract.
        unsafe { normalize_module_response(module_response, hhdm, module_storage) }
            .map_err(LimineBootInfoError::Modules)?
    };

    let kernel_image = KernelImage::from_linker_bounds(
        executable.physical_base.get(),
        linker_virtual_start,
        linker_virtual_end,
    )
    .map_err(LimineBootInfoError::KernelImageBounds)?;

    let info = BootInfo {
        memory_map: &memory_map_storage[..memory_map_count],
        kernel_image,
        modules: &module_storage[..module_count],
        hhdm_offset: Some(hhdm.get()),
        framebuffer: None,
    };
    info.validate()
        .map_err(LimineBootInfoError::InvalidBootInfo)?;

    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    #[test]
    fn consumes_required_responses_into_borrowed_boot_info() {
        let entry = super::super::LimineMemmapEntry {
            base: 0x1000,
            length: 0x20_0000,
            kind: super::super::LIMINE_MEMMAP_USABLE,
        };
        let entry_pointer = &entry as *const super::super::LimineMemmapEntry;
        let entry_pointers = [entry_pointer];
        let memory_map_response = LimineMemmapResponse {
            revision: 0,
            entry_count: 1,
            entries: entry_pointers.as_ptr(),
        };
        let hhdm_response = LimineHhdmResponse {
            revision: 0,
            offset: 0xffff_8000_0000_0000,
        };
        let executable_response = LimineExecutableAddressResponse {
            revision: 0,
            physical_base: 0x20_0000,
            virtual_base: 0xffff_ffff_8000_0000,
        };
        let mut memory_map_storage = [MemoryRegion {
            base: 0,
            length: 0,
            kind: super::super::MemoryRegionKind::Unknown,
            attributes: 0,
        }];
        let mut module_storage = [];
        // SAFETY: All response objects and nested pointers refer to aligned
        // local values that remain alive throughout this call.
        let info = unsafe {
            consume_limine_boot_info(
                true,
                &memory_map_response,
                &hhdm_response,
                &executable_response,
                ptr::null(),
                0xffff_ffff_8000_0000,
                0xffff_ffff_8010_0000,
                &mut memory_map_storage,
                &mut module_storage,
            )
        }
        .unwrap();

        assert_eq!(info.memory_map.len(), 1);
        assert_eq!(info.memory_map[0].base, 0x1000);
        assert_eq!(info.modules.len(), 0);
        assert_eq!(info.kernel_image.physical_base, 0x20_0000);
        assert_eq!(info.kernel_image.length, 0x10_0000);
        assert_eq!(info.hhdm_offset, Some(0xffff_8000_0000_0000));
    }

    #[test]
    fn rejects_missing_required_response_before_dereferencing() {
        let mut memory_map_storage = [];
        let mut module_storage = [];
        // SAFETY: Handoff validation returns before dereferencing null pointers.
        let result = unsafe {
            consume_limine_boot_info(
                true,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                0xffff_ffff_8000_0000,
                0xffff_ffff_8000_1000,
                &mut memory_map_storage,
                &mut module_storage,
            )
        };
        assert!(matches!(
            result,
            Err(LimineBootInfoError::Handoff(
                BootHandoffError::MissingMemoryMapResponse
            ))
        ));
    }
}
