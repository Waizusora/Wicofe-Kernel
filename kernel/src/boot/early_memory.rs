//! Allocation-free planning of page-aligned usable physical memory.
//!
//! This produces candidate ranges only. It does not initialize a PMM, claim
//! ownership, map memory, or make any range allocatable by itself.

use core::num::NonZeroU64;

use super::{BootReservation, MemoryMapError, MemoryRegion, MemoryRegionKind, validate_memory_map};

/// A page-aligned, half-open physical range eligible for later PMM metadata
/// placement or frame tracking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EarlyMemoryRange {
    pub base: u64,
    pub length: u64,
}

impl EarlyMemoryRange {
    pub const fn end(self) -> Option<u64> {
        self.base.checked_add(self.length)
    }
}

/// Errors while deriving the bounded early usable-memory plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EarlyMemoryPlanError {
    InvalidMemoryMap(MemoryMapError),
    InvalidPageSize,
    InvalidReservation,
    InsufficientCapacity { required: usize, available: usize },
}

/// Build sorted, page-aligned usable ranges after subtracting reservations.
///
/// The input memory map may be unsorted but must contain no overlapping
/// descriptors. Reservations must be non-empty, non-overflowing, and
/// pairwise disjoint. Only `Usable` regions are considered; reclaimable,
/// reserved, device, and unknown classes are excluded.
///
/// The function performs a counting pass before writing, so errors never
/// partially modify `output`. It uses no heap allocation. Adjacent usable
/// descriptors remain separate plan entries. The result is a planning
/// artifact, not a PMM allocator or an ownership transfer.
pub fn build_early_memory_plan(
    memory_map: &[MemoryRegion],
    reservations: &[BootReservation],
    page_size: NonZeroU64,
    output: &mut [EarlyMemoryRange],
) -> Result<usize, EarlyMemoryPlanError> {
    if !page_size.get().is_power_of_two() {
        return Err(EarlyMemoryPlanError::InvalidPageSize);
    }
    validate_memory_map(memory_map).map_err(EarlyMemoryPlanError::InvalidMemoryMap)?;
    validate_reservations(reservations)?;

    let mut required = 0usize;
    visit_eligible_ranges(memory_map, reservations, page_size.get(), |_| {
        required += 1;
    });

    if output.len() < required {
        return Err(EarlyMemoryPlanError::InsufficientCapacity {
            required,
            available: output.len(),
        });
    }

    let mut written = 0usize;
    visit_eligible_ranges(memory_map, reservations, page_size.get(), |range| {
        output[written] = range;
        written += 1;
    });
    output[..written].sort_unstable_by_key(|range| range.base);
    Ok(written)
}

fn validate_reservations(reservations: &[BootReservation]) -> Result<(), EarlyMemoryPlanError> {
    for (index, reservation) in reservations.iter().enumerate() {
        if reservation.length == 0 || reservation.end().is_none() {
            return Err(EarlyMemoryPlanError::InvalidReservation);
        }
        let end = reservation.end().expect("checked above");
        for other in &reservations[..index] {
            let other_end = other
                .end()
                .ok_or(EarlyMemoryPlanError::InvalidReservation)?;
            if other.length == 0 || (reservation.base < other_end && other.base < end) {
                return Err(EarlyMemoryPlanError::InvalidReservation);
            }
        }
    }
    Ok(())
}

fn visit_eligible_ranges(
    memory_map: &[MemoryRegion],
    reservations: &[BootReservation],
    page_size: u64,
    mut visit: impl FnMut(EarlyMemoryRange),
) {
    for region in memory_map {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let region_end = region.base + region.length;
        let mut cursor = region.base;

        while cursor < region_end {
            let mut covering_end = cursor;
            let mut next_reservation = region_end;

            for reservation in reservations {
                let reservation_end = reservation.base + reservation.length;
                if reservation.base <= cursor && cursor < reservation_end {
                    covering_end = covering_end.max(reservation_end);
                } else if reservation.base > cursor && reservation.base < next_reservation {
                    next_reservation = reservation.base;
                }
            }

            if covering_end > cursor {
                cursor = covering_end.min(region_end);
                continue;
            }

            let segment_end = next_reservation;
            let aligned_start = align_up(cursor, page_size);
            let aligned_end = segment_end - (segment_end % page_size);
            if let Some(aligned_start) = aligned_start {
                if aligned_start < aligned_end {
                    visit(EarlyMemoryRange {
                        base: aligned_start,
                        length: aligned_end - aligned_start,
                    });
                }
            }
            cursor = segment_end;
        }
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    let remainder = value % alignment;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(alignment - remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::BootReservationKind;

    const PAGE: NonZeroU64 = match NonZeroU64::new(0x1000) {
        Some(value) => value,
        None => panic!("page size must be non-zero"),
    };

    const fn region(base: u64, length: u64, kind: MemoryRegionKind) -> MemoryRegion {
        MemoryRegion {
            base,
            length,
            kind,
            attributes: 0,
        }
    }

    const fn reservation(base: u64, length: u64) -> BootReservation {
        BootReservation {
            base,
            length,
            kind: BootReservationKind::BootModule,
        }
    }

    #[test]
    fn subtracts_reservations_and_aligns_inward() {
        let map = [region(0x1003, 0x9000, MemoryRegionKind::Usable)];
        let reserved = [reservation(0x4000, 0x2000)];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 2];
        let count = build_early_memory_plan(&map, &reserved, PAGE, &mut output).unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            output[0],
            EarlyMemoryRange {
                base: 0x2000,
                length: 0x2000
            }
        );
        assert_eq!(
            output[1],
            EarlyMemoryRange {
                base: 0x6000,
                length: 0x4000
            }
        );
    }

    #[test]
    fn sorts_output_for_unsorted_memory_map() {
        let map = [
            region(0x8000, 0x2000, MemoryRegionKind::Usable),
            region(0x1000, 0x3000, MemoryRegionKind::Usable),
        ];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 2];
        let count = build_early_memory_plan(&map, &[], PAGE, &mut output).unwrap();
        assert_eq!(count, 2);
        assert_eq!(output[0].base, 0x1000);
        assert_eq!(output[1].base, 0x8000);
    }

    #[test]
    fn excludes_non_usable_memory_classes() {
        let map = [
            region(0x1000, 0x2000, MemoryRegionKind::Usable),
            region(0x4000, 0x2000, MemoryRegionKind::BootloaderReclaimable),
            region(0x7000, 0x2000, MemoryRegionKind::FirmwareReclaimable),
            region(0xa000, 0x2000, MemoryRegionKind::Device),
        ];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 4];
        let count = build_early_memory_plan(&map, &[], PAGE, &mut output).unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            output[0],
            EarlyMemoryRange {
                base: 0x1000,
                length: 0x2000
            }
        );
    }

    #[test]
    fn adjacent_regions_remain_separate() {
        let map = [
            region(0x1000, 0x1000, MemoryRegionKind::Usable),
            region(0x2000, 0x1000, MemoryRegionKind::Usable),
        ];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 2];
        let count = build_early_memory_plan(&map, &[], PAGE, &mut output).unwrap();
        assert_eq!(count, 2);
        assert_eq!(output[0].base + output[0].length, output[1].base);
    }

    #[test]
    fn insufficient_capacity_does_not_modify_output() {
        let map = [region(0x1000, 0x5000, MemoryRegionKind::Usable)];
        let reserved = [reservation(0x3000, 0x1000)];
        let sentinel = EarlyMemoryRange { base: 7, length: 9 };
        let mut output = [sentinel; 1];
        assert_eq!(
            build_early_memory_plan(&map, &reserved, PAGE, &mut output),
            Err(EarlyMemoryPlanError::InsufficientCapacity {
                required: 2,
                available: 1
            })
        );
        assert_eq!(output, [sentinel]);
    }

    #[test]
    fn rejects_invalid_page_size_and_reservations() {
        let map = [region(0x1000, 0x4000, MemoryRegionKind::Usable)];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 2];
        assert_eq!(
            build_early_memory_plan(&map, &[], NonZeroU64::new(3000).unwrap(), &mut output),
            Err(EarlyMemoryPlanError::InvalidPageSize)
        );
        assert_eq!(
            build_early_memory_plan(&map, &[reservation(0x2000, 0),], PAGE, &mut output),
            Err(EarlyMemoryPlanError::InvalidReservation)
        );
        assert_eq!(
            build_early_memory_plan(
                &map,
                &[reservation(0x2000, 0x2000), reservation(0x3000, 0x1000)],
                PAGE,
                &mut output
            ),
            Err(EarlyMemoryPlanError::InvalidReservation)
        );
    }

    #[test]
    fn rejects_overflowing_memory_map() {
        let map = [region(u64::MAX - 0xff, 0x200, MemoryRegionKind::Usable)];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 1];
        assert!(matches!(
            build_early_memory_plan(&map, &[], PAGE, &mut output),
            Err(EarlyMemoryPlanError::InvalidMemoryMap(
                MemoryMapError::AddressOverflow
            ))
        ));
    }

    #[test]
    fn handles_reservation_covering_region_start() {
        let map = [region(0x1000, 0x5000, MemoryRegionKind::Usable)];
        let reserved = [reservation(0x1000, 0x2000)];
        let mut output = [EarlyMemoryRange { base: 0, length: 0 }; 1];
        let count = build_early_memory_plan(&map, &reserved, PAGE, &mut output).unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            output[0],
            EarlyMemoryRange {
                base: 0x3000,
                length: 0x3000
            }
        );
    }
}
