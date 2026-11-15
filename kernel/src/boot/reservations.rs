//! Explicit early physical-memory reservations for PMM bootstrap.
//!
//! This module records ranges that must remain unavailable to general
//! allocation. It does not mutate the firmware memory map or free memory.

use super::info::{BootInfo, BootInfoError, MemoryRegion, MemoryRegionKind};

/// Why a physical range is reserved during early boot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootReservationKind {
    KernelImage,
    BootModule,
    Framebuffer,
}

/// A half-open physical range that must not be returned by the PMM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootReservation {
    pub base: u64,
    pub length: u64,
    pub kind: BootReservationKind,
}

impl BootReservation {
    pub const fn end(self) -> Option<u64> {
        self.base.checked_add(self.length)
    }
}

/// Errors while materializing the bounded early reservation list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReservationError {
    InvalidBootInfo(BootInfoError),
    InsufficientCapacity { required: usize, available: usize },
}

/// Build a complete reservation list in caller-owned storage.
///
/// The output is not modified unless the input BootInfo validates and the
/// output has enough capacity. Non-usable memory-map classes remain
/// unavailable independently; this list records the explicit live objects
/// that later PMM initialization must preserve.
pub fn build_boot_reservations(
    info: &BootInfo<'_>,
    output: &mut [BootReservation],
) -> Result<usize, ReservationError> {
    info.validate().map_err(ReservationError::InvalidBootInfo)?;

    let framebuffer_count = if info.framebuffer.is_some() { 1 } else { 0 };
    let required = 1 + info.modules.len() + framebuffer_count;
    if output.len() < required {
        return Err(ReservationError::InsufficientCapacity {
            required,
            available: output.len(),
        });
    }

    let mut count = 0;
    output[count] = BootReservation {
        base: info.kernel_image.physical_base,
        length: info.kernel_image.length,
        kind: BootReservationKind::KernelImage,
    };
    count += 1;

    for module in info.modules {
        output[count] = BootReservation {
            base: module.physical_base,
            length: module.length,
            kind: BootReservationKind::BootModule,
        };
        count += 1;
    }

    if let Some(framebuffer) = info.framebuffer {
        output[count] = BootReservation {
            base: framebuffer.physical_base,
            length: framebuffer.byte_length,
            kind: BootReservationKind::Framebuffer,
        };
        count += 1;
    }

    Ok(count)
}

/// Check whether a physical range is fully backed by usable memory and does
/// not intersect any explicit reservation. Adjacent usable regions may
/// jointly cover the candidate range.
pub fn is_usable_unreserved_range(
    memory_map: &[MemoryRegion],
    reservations: &[BootReservation],
    base: u64,
    length: u64,
) -> bool {
    let Some(end) = base.checked_add(length) else {
        return false;
    };
    if length == 0 || !range_is_covered_by_usable(memory_map, base, end) {
        return false;
    }

    for reservation in reservations {
        let Some(reservation_end) = reservation.end() else {
            return false;
        };
        if reservation.length == 0 || (base < reservation_end && reservation.base < end) {
            return false;
        }
    }

    true
}

fn range_is_covered_by_usable(regions: &[MemoryRegion], base: u64, end: u64) -> bool {
    let mut cursor = base;
    while cursor < end {
        let mut furthest = cursor;
        for region in regions {
            if region.kind != MemoryRegionKind::Usable {
                continue;
            }
            let Some(region_end) = region.end() else {
                return false;
            };
            if region.base <= cursor && cursor < region_end && region_end > furthest {
                furthest = region_end;
            }
        }
        if furthest == cursor {
            return false;
        }
        cursor = furthest.min(end);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::{BootModule, FramebufferInfo, KernelImage};

    const KERNEL: KernelImage = KernelImage {
        physical_base: 0x20_0000,
        virtual_base: 0xffff_ffff_8020_0000,
        length: 0x10_0000,
    };

    fn sample_info<'a>(
        memory_map: &'a [MemoryRegion],
        modules: &'a [BootModule],
        framebuffer: Option<FramebufferInfo>,
    ) -> BootInfo<'a> {
        BootInfo {
            memory_map,
            kernel_image: KERNEL,
            modules,
            hhdm_offset: None,
            framebuffer,
        }
    }

    #[test]
    fn builds_kernel_module_and_framebuffer_reservations() {
        let memory_map = [
            MemoryRegion {
                base: 0x1000,
                length: 0x1000,
                kind: MemoryRegionKind::Usable,
                attributes: 0,
            },
            MemoryRegion {
                base: 0x20_0000,
                length: 0x40_0000,
                kind: MemoryRegionKind::KernelAndModules,
                attributes: 0,
            },
            MemoryRegion {
                base: 0x8000_0000,
                length: 0x20_000,
                kind: MemoryRegionKind::Device,
                attributes: 0,
            },
        ];
        let modules = [BootModule {
            physical_base: 0x31_0000,
            length: 0x2000,
        }];
        let framebuffer = Some(FramebufferInfo {
            physical_base: 0x8000_0000,
            byte_length: 0x20_000,
            width: 640,
            height: 480,
            pitch: 2560,
            bits_per_pixel: 32,
        });
        let info = sample_info(&memory_map, &modules, framebuffer);
        let mut output = [BootReservation {
            base: 0,
            length: 0,
            kind: BootReservationKind::KernelImage,
        }; 3];

        let count = build_boot_reservations(&info, &mut output).unwrap();

        assert_eq!(count, 3);
        assert_eq!(output[0].kind, BootReservationKind::KernelImage);
        assert_eq!(output[1].kind, BootReservationKind::BootModule);
        assert_eq!(output[2].kind, BootReservationKind::Framebuffer);
    }

    #[test]
    fn rejects_insufficient_output_without_writing() {
        let memory_map = [MemoryRegion {
            base: 0x20_0000,
            length: 0x40_0000,
            kind: MemoryRegionKind::KernelAndModules,
            attributes: 0,
        }];
        let modules = [BootModule {
            physical_base: 0x31_0000,
            length: 0x2000,
        }];
        let info = sample_info(&memory_map, &modules, None);
        let sentinel = BootReservation {
            base: 7,
            length: 9,
            kind: BootReservationKind::BootModule,
        };
        let mut output = [sentinel; 1];

        assert_eq!(
            build_boot_reservations(&info, &mut output),
            Err(ReservationError::InsufficientCapacity {
                required: 2,
                available: 1,
            })
        );
        assert_eq!(output, [sentinel]);
    }

    #[test]
    fn usable_query_rejects_reserved_and_unmapped_ranges() {
        let memory_map = [MemoryRegion {
            base: 0x1000,
            length: 0x8000,
            kind: MemoryRegionKind::Usable,
            attributes: 0,
        }];
        let reservations = [BootReservation {
            base: 0x3000,
            length: 0x1000,
            kind: BootReservationKind::BootModule,
        }];

        assert!(is_usable_unreserved_range(
            &memory_map,
            &reservations,
            0x1000,
            0x1000
        ));
        assert!(!is_usable_unreserved_range(
            &memory_map,
            &reservations,
            0x2800,
            0x1000
        ));
        assert!(!is_usable_unreserved_range(
            &memory_map,
            &reservations,
            0x8000,
            0x2000
        ));
        assert!(!is_usable_unreserved_range(
            &memory_map,
            &reservations,
            u64::MAX - 3,
            8
        ));
    }

    #[test]
    fn adjacent_usable_regions_can_cover_candidate() {
        let memory_map = [
            MemoryRegion {
                base: 0x1000,
                length: 0x1000,
                kind: MemoryRegionKind::Usable,
                attributes: 0,
            },
            MemoryRegion {
                base: 0x2000,
                length: 0x1000,
                kind: MemoryRegionKind::Usable,
                attributes: 0,
            },
        ];
        assert!(is_usable_unreserved_range(&memory_map, &[], 0x1800, 0x1000));
    }
}
