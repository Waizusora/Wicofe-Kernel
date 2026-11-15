//! Limine memory-map response ABI and normalization adapter.
//!
//! The declarations here mirror the Limine Boot Protocol memory-map feature.
//! They are deliberately isolated from the architecture-neutral boot model.
//! Request placement/registration and the executable entry point are separate
//! boot-linker milestones.

use core::{mem, ptr};

use super::{validate_memory_map, MemoryMapError, MemoryRegion, MemoryRegionKind};

/// Limine memory-map entry type: usable RAM.
pub const LIMINE_MEMMAP_USABLE: u64 = 0;
/// Limine memory-map entry type: reserved.
pub const LIMINE_MEMMAP_RESERVED: u64 = 1;
/// Limine memory-map entry type: ACPI reclaimable.
pub const LIMINE_MEMMAP_ACPI_RECLAIMABLE: u64 = 2;
/// Limine memory-map entry type: ACPI NVS.
pub const LIMINE_MEMMAP_ACPI_NVS: u64 = 3;
/// Limine memory-map entry type: bad memory.
pub const LIMINE_MEMMAP_BAD_MEMORY: u64 = 4;
/// Limine memory-map entry type: bootloader reclaimable.
pub const LIMINE_MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;
/// Limine memory-map entry type: executable and modules.
pub const LIMINE_MEMMAP_EXECUTABLE_AND_MODULES: u64 = 6;
/// Limine memory-map entry type: framebuffer.
pub const LIMINE_MEMMAP_FRAMEBUFFER: u64 = 7;
/// Limine memory-map entry type: reserved and mapped.
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

/// Errors returned while consuming a Limine memory-map response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimineAdapterError {
    NullResponse,
    NullEntries,
    NullEntry,
    MisalignedPointer,
    CountDoesNotFit,
    OutputTooSmall,
    InvalidMemoryMap(MemoryMapError),
}

/// Convert a Limine memory-map type to the conservative WICOFE classification.
///
/// Unknown future values are retained as `Unknown`; PMM must not allocate them.
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
///
/// The caller must guarantee that:
/// - `response` points to a readable, correctly aligned
///   `LimineMemmapResponse` for the duration of this call;
/// - when `entry_count` is non-zero, `entries` points to an array containing
///   exactly at least `entry_count` readable, correctly aligned entry pointers;
/// - every non-null entry pointer refers to a readable, correctly aligned
///   `LimineMemmapEntry`;
/// - all response and entry memory remains mapped and live during the call.
///
/// Null and alignment checks below catch malformed descriptors, but cannot
/// prove that arbitrary bootloader-provided addresses are mapped/readable.
/// The caller must establish that property from the boot handoff contract.
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

    // SAFETY: Pointer validity and lifetime are part of this function's
    // documented caller contract; alignment and nullness were checked above.
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
        // SAFETY: The caller guarantees an entries array of at least count
        // elements. The count was checked to fit usize and output capacity.
        let entry_pointer = unsafe { ptr::read(response_value.entries.add(index)) };
        if entry_pointer.is_null() {
            return Err(LimineAdapterError::NullEntry);
        }
        if (entry_pointer as usize) % mem::align_of::<LimineMemmapEntry>() != 0 {
            return Err(LimineAdapterError::MisalignedPointer);
        }

        // SAFETY: The caller guarantees each non-null entry pointer is
        // readable and live; alignment and nullness were checked above.
        let entry = unsafe { ptr::read(entry_pointer) };
        *slot = MemoryRegion {
            base: entry.base,
            length: entry.length,
            kind: normalize_memory_kind(entry.kind),
            attributes: 0,
        };
    }

    let normalized = &output[..count];
    validate_memory_map(normalized).map_err(LimineAdapterError::InvalidMemoryMap)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_all_known_types_conservatively() {
        assert_eq!(normalize_memory_kind(0), MemoryRegionKind::Usable);
        assert_eq!(normalize_memory_kind(1), MemoryRegionKind::FirmwareReserved);
        assert_eq!(normalize_memory_kind(2), MemoryRegionKind::FirmwareReclaimable);
        assert_eq!(normalize_memory_kind(3), MemoryRegionKind::FirmwareReserved);
        assert_eq!(normalize_memory_kind(4), MemoryRegionKind::Bad);
        assert_eq!(normalize_memory_kind(5), MemoryRegionKind::BootloaderReclaimable);
        assert_eq!(normalize_memory_kind(6), MemoryRegionKind::KernelAndModules);
        assert_eq!(normalize_memory_kind(7), MemoryRegionKind::Device);
        assert_eq!(normalize_memory_kind(8), MemoryRegionKind::FirmwareReserved);
    }

    #[test]
    fn preserves_unknown_future_types_as_unallocatable_unknown() {
        assert_eq!(normalize_memory_kind(u64::MAX), MemoryRegionKind::Unknown);
    }

    #[test]
    fn limine_entry_has_expected_abi_layout() {
        assert_eq!(mem::size_of::<LimineMemmapEntry>(), 24);
        assert_eq!(mem::align_of::<LimineMemmapEntry>(), 8);
        assert_eq!(mem::size_of::<LimineMemmapResponse>(), 24);
        assert_eq!(mem::align_of::<LimineMemmapResponse>(), 8);
    }
}
