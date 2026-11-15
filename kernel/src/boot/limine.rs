//! Limine response ABI declarations and allocation-free normalization adapters.
//!
//! Protocol-specific structures stay isolated here. General kernel code uses
//! WICOFE-owned address and boot-information types. Request registration,
//! linker retention, and architecture entry are separate milestones.

use core::{mem, ptr};

use crate::memory::{PhysAddr, VirtAddr};

use super::{validate_memory_map, BootModule, MemoryMapError, MemoryRegion, MemoryRegionKind};

pub const LIMINE_MEMMAP_USABLE: u64 = 0;
pub const LIMINE_MEMMAP_RESERVED: u64 = 1;
pub const LIMINE_MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
pub const LIMINE_MEMMAP_ACPI_NVS: u64 = 3;
pub const LIMINE_MEMMAP_BAD_MEMORY: u64 = 4;
pub const LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
pub const LIMINE_MEMMAP_EXECUTABLE_AND_MODULES: u64 = 6;
pub const LIMINE_MEMMAP_FRAMEBUFFER: u64 = 7;
pub const LIMINE_MEMMAP_RESERVED_MAPPED: u64 = 8;

/// ABI representation of `struct limine_memmap_entry`.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineMemmapEntry {
    pub base: u64,
    pub length: u64,
    pub kind: u64,
}

/// ABI representation of `struct limine_memmap_response`.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineMemmapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *const *const LimineMemmapEntry,
}

/// ABI representation of `struct limine_hhdm_response`.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineHhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

/// ABI representation of `struct limine_executable_address_response`.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineExecutableAddressResponse {
    pub revision: u64,
    pub physical_base: u64,
    pub virtual_base: u64,
}

/// ABI representation of Limine's 16-byte UUID structure.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineUuid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}

/// ABI representation of `struct limine_file`.
///
/// Pointer fields supplied by Limine are HHDM virtual pointers. This adapter
/// consumes only `address` and `size`; it does not retain `path` or
/// `string`, whose contents reside in bootloader-reclaimable memory.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineFile {
    pub revision: u64,
    pub address: *const u8,
    pub size: u64,
    pub path: *const u8,
    pub string: *const u8,
    pub media_type: u32,
    pub unused: u32,
    pub tftp_ipv4: [u8; 4],
    pub tftp_port: u32,
    pub partition_index: u32,
    pub mbr_disk_id: u32,
    pub gpt_disk_uuid: LimineUuid,
    pub gpt_part_uuid: LimineUuid,
    pub part_uuid: LimineUuid,
}

/// ABI representation of `struct limine_module_response`.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct LimineModuleResponse {
    pub revision: u64,
    pub module_count: u64,
    pub modules: *const *const LimineFile,
}

/// HHDM virtual offset. This is not a physical or dereferenceable address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct HhdmOffset(u64);

impl HhdmOffset {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Normalized base addresses for the loaded executable image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutableAddress {
    pub physical_base: PhysAddr,
    pub virtual_base: VirtAddr,
}

/// Errors returned while consuming Limine responses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimineAdapterError {
    NullResponse,
    NullEntries,
    NullEntry,
    NullModules,
    NullModule,
    NullModuleAddress,
    MisalignedPointer,
    MisalignedModuleAddress,
    ModuleAddressOutsideHhdm,
    EmptyModule,
    ModuleRangeOverflow,
    OverlappingModules,
    CountDoesNotFit,
    OutputTooSmall,
    InvalidMemoryMap(MemoryMapError),
}

/// Convert a Limine memory-map type to the conservative WICOFE classification.
pub const fn normalize_memory_kind(kind: u64) -> MemoryRegionKind {
    match kind {
        LIMINE_MEMMAP_USABLE => MemoryRegionKind::Usable,
        LIMINE_MEMMAP_RESERVED => MemoryRegionKind::FirmwareReserved,
        LIMINE_MEMMAP_ACPI_RECLAIMABLE => MemoryRegionKind::FirmwareReclaimable,
        LIMINE_MEMMAP_ACPI_NVS | LIMINE_MEMMAP_RESERVED_MAPPED => {
            MemoryRegionKind::FirmwareReserved
        }
        LIMINE_MEMMAP_BAD_MEMORY => MemoryRegionKind::Bad,
        LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE => MemoryRegionKind::BootloaderReclaimable,
        LIMINE_MEMMAP_EXECUTABLE_AND_MODULES => MemoryRegionKind::KernelAndModules,
        LIMINE_MEMMAP_FRAMEBUFFER => MemoryRegionKind::Device,
        _ => MemoryRegionKind::Unknown,
    }
}

/// Copy and normalize a Limine memory-map response into caller-owned storage.
///
/// # Safety
/// The caller must guarantee that the response, its pointer array, and every
/// non-null entry pointer are readable, correctly aligned, mapped, and live
/// for the duration of this call. Null/alignment checks cannot prove mapping
/// or readability of arbitrary addresses.
pub unsafe fn normalize_memory_map_response(
    response: *const LimineMemmapResponse,
    output: &mut [MemoryRegion],
) -> Result<usize, LimineAdapterError> {
    if response.is_null() {
        return Err(LimineAdapterError::NullResponse);
    }
    if (response as usize) % mem::align_of::<LimineMemmapResponse>() != 0 {
        return Err(LimineAdapterError::MisalignedPointer);
    }

    // SAFETY: Pointer validity and lifetime are the caller's responsibility.
    let response_value = unsafe { ptr::read(response) };
    let count = usize::try_from(response_value.entry_count)
        .map_err(|_| LimineAdapterError::CountDoesNotFit)?;

    if count > output.len() {
        return Err(LimineAdapterError::OutputTooSmall);
    }
    if count != 0 && response_value.entries.is_null() {
        return Err(LimineAdapterError::NullEntries);
    }
    if count != 0
        && (response_value.entries as usize) % mem::align_of::<*const LimineMemmapEntry>() != 0
    {
        return Err(LimineAdapterError::MisalignedPointer);
    }

    for (index, slot) in output.iter_mut().take(count).enumerate() {
        // SAFETY: The caller guarantees an array with at least count entries.
        let entry_pointer = unsafe { ptr::read(response_value.entries.add(index)) };
        if entry_pointer.is_null() {
            return Err(LimineAdapterError::NullEntry);
        }
        if (entry_pointer as usize) % mem::align_of::<LimineMemmapEntry>() != 0 {
            return Err(LimineAdapterError::MisalignedPointer);
        }
        // SAFETY: The caller guarantees each entry is readable and live.
        let entry = unsafe { ptr::read(entry_pointer) };
        *slot = MemoryRegion {
            base: entry.base,
            length: entry.length,
            kind: normalize_memory_kind(entry.kind),
            attributes: 0,
        };
    }

    validate_memory_map(&output[..count]).map_err(LimineAdapterError::InvalidMemoryMap)?;
    Ok(count)
}

/// Normalize a Limine HHDM response to a typed virtual offset.
///
/// # Safety
/// `response` must point to a readable, correctly aligned response that
/// remains live for this call. The caller must establish that it is mapped.
pub unsafe fn normalize_hhdm_response(
    response: *const LimineHhdmResponse,
) -> Result<HhdmOffset, LimineAdapterError> {
    if response.is_null() {
        return Err(LimineAdapterError::NullResponse);
    }
    if (response as usize) % mem::align_of::<LimineHhdmResponse>() != 0 {
        return Err(LimineAdapterError::MisalignedPointer);
    }
    // SAFETY: Readability and lifetime are guaranteed by the caller.
    let value = unsafe { ptr::read(response) };
    Ok(HhdmOffset::new(value.offset))
}

/// Normalize the physical and virtual base addresses of the loaded executable.
///
/// The response does not include an image length. Obtain that independently
/// from trusted linker symbols or validated executable-format metadata.
///
/// # Safety
/// `response` must point to a readable, correctly aligned response that
/// remains live for this call. The caller must establish that it is mapped.
pub unsafe fn normalize_executable_address_response(
    response: *const LimineExecutableAddressResponse,
) -> Result<ExecutableAddress, LimineAdapterError> {
    if response.is_null() {
        return Err(LimineAdapterError::NullResponse);
    }
    if (response as usize) % mem::align_of::<LimineExecutableAddressResponse>() != 0 {
        return Err(LimineAdapterError::MisalignedPointer);
    }
    // SAFETY: Readability and lifetime are guaranteed by the caller.
    let value = unsafe { ptr::read(response) };
    Ok(ExecutableAddress {
        physical_base: PhysAddr::new(value.physical_base),
        virtual_base: VirtAddr::new(value.virtual_base),
    })
}

/// Copy module physical ranges into caller-owned storage.
///
/// Limine's `limine_file.address` is a virtual pointer with the HHDM offset
/// already applied. This adapter subtracts that offset to produce the
/// physical base stored in `BootModule`. The protocol guarantees module
/// buffers are 4 KiB aligned. Path/string metadata is intentionally not
/// retained; it must be copied separately before bootloader memory is freed.
///
/// A missing module response is represented by a null pointer and returned as
/// `NullResponse`; callers may treat that as an empty optional module list.
///
/// # Safety
/// The response, its module-pointer array, and every non-null file structure
/// must be readable, correctly aligned, mapped, and live for this call. The
/// caller must also ensure the supplied HHDM offset is the one used by Limine.
pub unsafe fn normalize_module_response(
    response: *const LimineModuleResponse,
    hhdm_offset: HhdmOffset,
    output: &mut [BootModule],
) -> Result<usize, LimineAdapterError> {
    if response.is_null() {
        return Err(LimineAdapterError::NullResponse);
    }
    if (response as usize) % mem::align_of::<LimineModuleResponse>() != 0 {
        return Err(LimineAdapterError::MisalignedPointer);
    }

    // SAFETY: Response readability and lifetime are guaranteed by the caller.
    let response_value = unsafe { ptr::read(response) };
    let count = usize::try_from(response_value.module_count)
        .map_err(|_| LimineAdapterError::CountDoesNotFit)?;
    if count > output.len() {
        return Err(LimineAdapterError::OutputTooSmall);
    }
    if count != 0 && response_value.modules.is_null() {
        return Err(LimineAdapterError::NullModules);
    }
    if count != 0 && (response_value.modules as usize) % mem::align_of::<*const LimineFile>() != 0 {
        return Err(LimineAdapterError::MisalignedPointer);
    }

    const MODULE_ALIGNMENT: u64 = 4096;
    for (index, slot) in output.iter_mut().take(count).enumerate() {
        // SAFETY: The caller guarantees an array with at least count entries.
        let file_pointer = unsafe { ptr::read(response_value.modules.add(index)) };
        if file_pointer.is_null() {
            return Err(LimineAdapterError::NullModule);
        }
        if (file_pointer as usize) % mem::align_of::<LimineFile>() != 0 {
            return Err(LimineAdapterError::MisalignedPointer);
        }
        // SAFETY: The caller guarantees each file structure is readable/live.
        let file = unsafe { ptr::read(file_pointer) };

        if file.address.is_null() {
            return Err(LimineAdapterError::NullModuleAddress);
        }
        let virtual_address = file.address as usize as u64;
        if virtual_address % MODULE_ALIGNMENT != 0 {
            return Err(LimineAdapterError::MisalignedModuleAddress);
        }
        let physical_base = virtual_address
            .checked_sub(hhdm_offset.get())
            .ok_or(LimineAdapterError::ModuleAddressOutsideHhdm)?;
        if physical_base % MODULE_ALIGNMENT != 0 {
            return Err(LimineAdapterError::MisalignedModuleAddress);
        }
        if file.size == 0 {
            return Err(LimineAdapterError::EmptyModule);
        }
        let end = physical_base
            .checked_add(file.size)
            .ok_or(LimineAdapterError::ModuleRangeOverflow)?;

        for previous in &output[..index] {
            let previous_end = previous
                .physical_base
                .checked_add(previous.length)
                .ok_or(LimineAdapterError::ModuleRangeOverflow)?;
            if physical_base < previous_end && previous.physical_base < end {
                return Err(LimineAdapterError::OverlappingModules);
            }
        }

        *slot = BootModule {
            physical_base,
            length: file.size,
        };
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HHDM: u64 = 0xffff_8000_0000_0000;

    fn empty_file() -> LimineFile {
        LimineFile {
            revision: 0,
            address: core::ptr::null(),
            size: 0,
            path: core::ptr::null(),
            string: core::ptr::null(),
            media_type: 0,
            unused: 0,
            tftp_ipv4: [0; 4],
            tftp_port: 0,
            partition_index: 0,
            mbr_disk_id: 0,
            gpt_disk_uuid: LimineUuid {
                a: 0,
                b: 0,
                c: 0,
                d: [0; 8],
            },
            gpt_part_uuid: LimineUuid {
                a: 0,
                b: 0,
                c: 0,
                d: [0; 8],
            },
            part_uuid: LimineUuid {
                a: 0,
                b: 0,
                c: 0,
                d: [0; 8],
            },
        }
    }

    #[test]
    fn maps_known_memory_types_conservatively() {
        assert_eq!(normalize_memory_kind(0), MemoryRegionKind::Usable);
        assert_eq!(normalize_memory_kind(1), MemoryRegionKind::FirmwareReserved);
        assert_eq!(
            normalize_memory_kind(2),
            MemoryRegionKind::FirmwareReclaimable
        );
        assert_eq!(normalize_memory_kind(3), MemoryRegionKind::FirmwareReserved);
        assert_eq!(normalize_memory_kind(4), MemoryRegionKind::Bad);
        assert_eq!(
            normalize_memory_kind(5),
            MemoryRegionKind::BootloaderReclaimable
        );
        assert_eq!(normalize_memory_kind(6), MemoryRegionKind::KernelAndModules);
        assert_eq!(normalize_memory_kind(7), MemoryRegionKind::Device);
        assert_eq!(normalize_memory_kind(8), MemoryRegionKind::FirmwareReserved);
    }

    #[test]
    fn preserves_unknown_future_memory_types() {
        assert_eq!(normalize_memory_kind(u64::MAX), MemoryRegionKind::Unknown);
    }

    #[test]
    fn limine_response_structures_match_protocol_layout() {
        assert_eq!(mem::size_of::<LimineMemmapEntry>(), 24);
        assert_eq!(mem::align_of::<LimineMemmapEntry>(), 8);
        assert_eq!(mem::size_of::<LimineMemmapResponse>(), 24);
        assert_eq!(mem::align_of::<LimineMemmapResponse>(), 8);
        assert_eq!(mem::size_of::<LimineHhdmResponse>(), 16);
        assert_eq!(mem::align_of::<LimineHhdmResponse>(), 8);
        assert_eq!(mem::size_of::<LimineExecutableAddressResponse>(), 24);
        assert_eq!(mem::align_of::<LimineExecutableAddressResponse>(), 8);
        assert_eq!(mem::size_of::<LimineUuid>(), 16);
        assert_eq!(mem::size_of::<LimineFile>(), 112);
        assert_eq!(mem::align_of::<LimineFile>(), 8);
        assert_eq!(mem::size_of::<LimineModuleResponse>(), 24);
        assert_eq!(mem::align_of::<LimineModuleResponse>(), 8);
    }

    #[test]
    fn normalizes_hhdm_offset_without_treating_it_as_physical() {
        let response = LimineHhdmResponse {
            revision: 0,
            offset: HHDM,
        };
        // SAFETY: The local response is aligned, readable, and live.
        let offset = unsafe { normalize_hhdm_response(&response) }.unwrap();
        assert_eq!(offset, HhdmOffset::new(HHDM));
    }

    #[test]
    fn normalizes_executable_bases_into_distinct_address_types() {
        let response = LimineExecutableAddressResponse {
            revision: 0,
            physical_base: 0x20_0000,
            virtual_base: 0xffff_ffff_8020_0000,
        };
        // SAFETY: The local response is aligned, readable, and live.
        let address = unsafe { normalize_executable_address_response(&response) }.unwrap();
        assert_eq!(address.physical_base, PhysAddr::new(0x20_0000));
        assert_eq!(address.virtual_base, VirtAddr::new(0xffff_ffff_8020_0000));
    }

    #[test]
    fn normalizes_module_hhdm_pointer_to_physical_range() {
        let mut file = empty_file();
        file.address = (HHDM + 0x40_0000) as usize as *const u8;
        file.size = 0x3000;
        let file_pointer = &file as *const LimineFile;
        let pointers = [file_pointer];
        let response = LimineModuleResponse {
            revision: 0,
            module_count: 1,
            modules: pointers.as_ptr(),
        };
        let mut output = [BootModule {
            physical_base: 0,
            length: 0,
        }];
        // SAFETY: Response, pointer array and file are local readable values.
        let count =
            unsafe { normalize_module_response(&response, HhdmOffset::new(HHDM), &mut output) }
                .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            output[0],
            BootModule {
                physical_base: 0x40_0000,
                length: 0x3000
            }
        );
    }

    #[test]
    fn accepts_empty_module_response_with_null_array() {
        let response = LimineModuleResponse {
            revision: 0,
            module_count: 0,
            modules: core::ptr::null(),
        };
        let mut output = [];
        // SAFETY: The local response is readable and aligned.
        assert_eq!(
            unsafe { normalize_module_response(&response, HhdmOffset::new(HHDM), &mut output) },
            Ok(0)
        );
    }

    #[test]
    fn rejects_invalid_module_descriptors() {
        let mut file = empty_file();
        file.address = (HHDM + 0x40_0000) as usize as *const u8;
        file.size = 0;
        let pointers = [&file as *const LimineFile];
        let response = LimineModuleResponse {
            revision: 0,
            module_count: 1,
            modules: pointers.as_ptr(),
        };
        let mut output = [BootModule {
            physical_base: 0,
            length: 0,
        }];
        // SAFETY: All response objects are local and readable.
        assert_eq!(
            unsafe { normalize_module_response(&response, HhdmOffset::new(HHDM), &mut output) },
            Err(LimineAdapterError::EmptyModule)
        );

        file.size = 0x1000;
        file.address = (HHDM + 0x40_0001) as usize as *const u8;
        // SAFETY: All response objects are local and readable.
        assert_eq!(
            unsafe { normalize_module_response(&response, HhdmOffset::new(HHDM), &mut output) },
            Err(LimineAdapterError::MisalignedModuleAddress)
        );

        file.address = 0x1000usize as *const u8;
        // SAFETY: All response objects are local and readable.
        assert_eq!(
            unsafe { normalize_module_response(&response, HhdmOffset::new(HHDM), &mut output) },
            Err(LimineAdapterError::ModuleAddressOutsideHhdm)
        );
    }

    #[test]
    fn rejects_null_and_misaligned_single_response_pointers() {
        // SAFETY: Null is rejected before dereferencing.
        assert_eq!(
            unsafe { normalize_hhdm_response(core::ptr::null()) },
            Err(LimineAdapterError::NullResponse)
        );
        // SAFETY: Misalignment is rejected before dereferencing.
        assert_eq!(
            unsafe { normalize_hhdm_response(1usize as *const LimineHhdmResponse) },
            Err(LimineAdapterError::MisalignedPointer)
        );
        // SAFETY: Null is rejected before dereferencing.
        assert_eq!(
            unsafe { normalize_executable_address_response(core::ptr::null()) },
            Err(LimineAdapterError::NullResponse)
        );
        // SAFETY: Misalignment is rejected before dereferencing.
        assert_eq!(
            unsafe {
                normalize_executable_address_response(
                    1usize as *const LimineExecutableAddressResponse,
                )
            },
            Err(LimineAdapterError::MisalignedPointer)
        );
    }

    #[test]
    fn rejects_null_and_misaligned_memory_map_response_pointers() {
        // SAFETY: Null is rejected before dereferencing.
        assert_eq!(
            unsafe { normalize_memory_map_response(core::ptr::null(), &mut []) },
            Err(LimineAdapterError::NullResponse)
        );
        // SAFETY: Misalignment is rejected before dereferencing.
        assert_eq!(
            unsafe {
                normalize_memory_map_response(1usize as *const LimineMemmapResponse, &mut [])
            },
            Err(LimineAdapterError::MisalignedPointer)
        );
    }

    #[test]
    fn rejects_null_module_response_pointer() {
        let mut output = [];
        // SAFETY: Null is rejected before dereferencing.
        assert_eq!(
            unsafe {
                normalize_module_response(core::ptr::null(), HhdmOffset::new(HHDM), &mut output)
            },
            Err(LimineAdapterError::NullResponse)
        );
    }
}
