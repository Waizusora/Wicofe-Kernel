//! Protocol-independent validation for normalized boot memory maps.
//!
//! This layer validates arithmetic and range relationships only. A protocol
//! adapter must still validate response pointers before dereferencing them.

use super::MemoryRegion;

/// Errors found while validating a normalized physical memory map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryMapError {
    EmptyRegion,
    AddressOverflow,
    OverlappingRegions,
}

/// Validate all regions without allocating or assuming that input is sorted.
///
/// Adjacent half-open ranges are valid. Any overlap is rejected, including
/// overlaps between regions with different kinds. Unknown region kinds remain
/// valid descriptors; callers must treat them as unavailable for allocation.
pub fn validate_memory_map(regions: &[MemoryRegion]) -> Result<(), MemoryMapError> {
    for (index, region) in regions.iter().enumerate() {
        if region.length == 0 {
            return Err(MemoryMapError::EmptyRegion);
        }
        let end = region
            .base
            .checked_add(region.length)
            .ok_or(MemoryMapError::AddressOverflow)?;

        // A quadratic scan is intentional: boot maps are small, this avoids
        // allocation, and it also detects overlaps in unsorted input.
        for other in &regions[..index] {
            let other_end = other
                .base
                .checked_add(other.length)
                .ok_or(MemoryMapError::AddressOverflow)?;
            if region.base < other_end && other.base < end {
                return Err(MemoryMapError::OverlappingRegions);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::MemoryRegionKind;

    const fn region(base: u64, length: u64) -> MemoryRegion {
        MemoryRegion {
            base,
            length,
            kind: MemoryRegionKind::Usable,
            attributes: 0,
        }
    }

    #[test]
    fn accepts_sorted_and_unsorted_non_overlapping_regions() {
        let regions = [region(0x4000, 0x1000), region(0x1000, 0x2000)];
        assert_eq!(validate_memory_map(&regions), Ok(()));
    }

    #[test]
    fn accepts_adjacent_half_open_regions() {
        let regions = [region(0x1000, 0x1000), region(0x2000, 0x1000)];
        assert_eq!(validate_memory_map(&regions), Ok(()));
    }

    #[test]
    fn rejects_overlapping_regions_even_when_kinds_differ() {
        let regions = [
            region(0x1000, 0x3000),
            MemoryRegion {
                base: 0x2000,
                length: 0x1000,
                kind: MemoryRegionKind::FirmwareReserved,
                attributes: 0,
            },
        ];
        assert_eq!(
            validate_memory_map(&regions),
            Err(MemoryMapError::OverlappingRegions)
        );
    }

    #[test]
    fn rejects_zero_length_regions() {
        assert_eq!(
            validate_memory_map(&[region(0x1000, 0)]),
            Err(MemoryMapError::EmptyRegion)
        );
    }

    #[test]
    fn rejects_address_overflow() {
        assert_eq!(
            validate_memory_map(&[region(u64::MAX - 0xff, 0x100)]),
            Err(MemoryMapError::AddressOverflow)
        );
    }
}
